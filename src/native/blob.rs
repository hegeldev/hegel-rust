// Failure blobs: a portable, copy-pasteable reproducer for a failing test
// case — a base64 string encoding its choice sequence.
//
// A blob encodes the *choice sequence* of a (usually minimal) failing test
// case so it can be replayed deterministically — pasted into a
// `#[hegel::reproduce_failure("…")]` attribute, fed to
// `Settings::reproduce_failure`, or handed across the C ABI.
//
// # Format
//
//   base64( prefix_byte ++ payload )
//
// where `payload` is [`serialize_choices`] of the choice sequence and the
// `prefix_byte` selects how it is stored:
//
//   - `0` (`PREFIX_RAW`):  `payload` is the raw `serialize_choices` bytes.
//   - `1` (`PREFIX_ZLIB`): `payload` is the zlib compression of those bytes.
//
// [`encode_failure`] computes both and keeps whichever is shorter — for the
// tiny choice sequences a shrunk counterexample usually has, the zlib header
// overhead loses and the raw form wins (so most blobs carry prefix `0`); for
// large sequences the compressed form wins. The inner `serialize_choices`
// encoding (see [`crate::native::database`]) is Hegel's own, so a blob is
// only portable between matching Hegel versions.
//
// [`decode_failure`] reverses every step and returns `None` on *any*
// malformation (bad base64, unknown prefix byte, corrupt zlib stream, or a
// payload [`deserialize_choices`] rejects). Callers treat `None` as "this
// blob can't be replayed" and panic.

use crate::native::core::ChoiceValue;
use crate::native::database::{deserialize_choices, serialize_choices};

/// `payload` is the raw [`serialize_choices`] output.
const PREFIX_RAW: u8 = 0;
/// `payload` is the zlib compression of the [`serialize_choices`] output.
const PREFIX_ZLIB: u8 = 1;

/// zlib compression level used by [`encode_failure`]. 6 is the zlib default.
const ZLIB_LEVEL: u8 = 6;

/// Encode a choice sequence into a failure blob (see the module docs for the
/// format). The returned string is safe to embed in source as a string
/// literal and to round-trip through [`decode_failure`].
pub fn encode_failure(choices: &[ChoiceValue]) -> String {
    let raw = serialize_choices(choices);
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, ZLIB_LEVEL);

    let (prefix, body) = if compressed.len() < raw.len() {
        (PREFIX_ZLIB, compressed)
    } else {
        (PREFIX_RAW, raw)
    };

    let mut payload = Vec::with_capacity(body.len() + 1);
    payload.push(prefix);
    payload.extend_from_slice(&body);
    base64_encode(&payload)
}

/// Decode a failure blob produced by [`encode_failure`] back into a choice
/// sequence, or `None` if the blob is malformed, truncated, compressed with a
/// corrupt stream, carries an unknown prefix byte, or decodes to bytes
/// [`deserialize_choices`] rejects.
pub fn decode_failure(blob: &str) -> Option<Vec<ChoiceValue>> {
    let bytes = base64_decode(blob)?;
    let (&prefix, rest) = bytes.split_first()?;
    let raw = match prefix {
        PREFIX_RAW => rest.to_vec(),
        PREFIX_ZLIB => miniz_oxide::inflate::decompress_to_vec_zlib(rest).ok()?,
        _ => return None,
    };
    deserialize_choices(&raw)
}

/// Standard base64 alphabet (RFC 4648), with `=` padding.
const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode bytes as a padded standard-alphabet base64 string.
fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(B64_ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64_ALPHABET[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_ALPHABET[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Decode a single base64 digit to its 6-bit value, or `None` for any byte
/// outside the standard alphabet (including `=`, handled separately).
fn base64_value(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decode a padded standard-alphabet base64 string, or `None` if the input
/// length isn't a multiple of 4, contains a non-alphabet byte, or uses `=`
/// padding anywhere but the tail of the final quad.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.as_bytes();
    if s.len() % 4 != 0 {
        return None;
    }
    let n_chunks = s.len() / 4;
    let mut out = Vec::with_capacity(n_chunks * 3);
    for (i, chunk) in s.chunks(4).enumerate() {
        let last = i + 1 == n_chunks;
        let c0 = base64_value(chunk[0])?;
        let c1 = base64_value(chunk[1])?;
        let pad2 = chunk[2] == b'=';
        let pad3 = chunk[3] == b'=';
        // Padding may appear only at the tail of the last quad, and a
        // padded third position forces a padded fourth ("=X" is invalid).
        if (pad2 || pad3) && !last {
            return None;
        }
        if pad2 && !pad3 {
            return None;
        }
        let c2 = if pad2 { 0 } else { base64_value(chunk[2])? };
        let c3 = if pad3 { 0 } else { base64_value(chunk[3])? };
        let n =
            (u32::from(c0) << 18) | (u32::from(c1) << 12) | (u32::from(c2) << 6) | u32::from(c3);
        out.push((n >> 16) as u8);
        if !pad2 {
            out.push((n >> 8) as u8);
        }
        if !pad3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
#[path = "../../tests/embedded/native/blob_tests.rs"]
mod tests;

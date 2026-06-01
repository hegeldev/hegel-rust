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

use crate::native::base64::{base64_decode, base64_encode};
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

#[cfg(test)]
#[path = "../../tests/embedded/native/blob_tests.rs"]
mod tests;

//! Failure blobs: a portable, copy-pasteable reproducer for a failing test
//! case — a base64 string encoding its choice sequence.
//!
//! A blob encodes the *choice sequence* of a (usually minimal) failing test
//! case so it can be replayed deterministically — pasted into a
//! `#[hegel::reproduce_failure("…")]` attribute, fed to
//! `Hegel::reproduce_failure`, or handed across the C ABI.
//!
//! # Format
//!
//! ```text
//! base64( prefix_byte ++ payload )
//! ```
//!
//! where the `prefix_byte` selects the payload's meaning and storage:
//!
//! - `0` (`PREFIX_RAW`):  `payload` is the raw [`serialize_choices`] bytes of
//!   one choice sequence.
//! - `1` (`PREFIX_ZLIB`): `payload` is the zlib compression of those bytes.
//! - `2` (`PREFIX_ND_RAW`) / `3` (`PREFIX_ND_ZLIB`): `payload` is the raw /
//!   zlib-compressed [nondeterministic replay state](NdReproState) — the
//!   incumbent timeline plus its captured pool, an entropy seed, and a
//!   continuation budget, so a flaky failure can be replayed until it
//!   reproduces rather than exactly once.
//!
//! [`encode_failure`] computes both and keeps whichever is shorter — for the
//! tiny choice sequences a shrunk counterexample usually has, the zlib header
//! overhead loses and the raw form wins (so most blobs carry prefix `0`); for
//! large sequences the compressed form wins. The inner `serialize_choices`
//! encoding (see [`crate::native::database`]) is Hegel's own, so it is only
//! guaranteed to reproduce a failure within a specific version of Hegel.
//!
//! [`decode_blob`] reverses every step and returns `None` on *any*
//! malformation (bad base64, unknown prefix byte, corrupt zlib stream, or a
//! payload the inner decoder rejects). Callers treat `None` as "this blob
//! can't be replayed" and panic. Compressed payloads decode under the
//! [`MAX_DECOMPRESSED_LEN`] bound, and a stream that inflates past it counts
//! as malformed.
//!
//! The nondeterministic state bytes double as the version-2 **database
//! entry** format (decision 8: ND-ness is carried by the representation).
//! They open with a `u32::MAX` choice count no genuine [`serialize_choices`]
//! output can start with, so a pre-v2 reader's [`deserialize_choices`]
//! rejects them as corrupt instead of misreading them.

use crate::native::base64::{base64_decode, base64_encode};
use crate::native::core::ChoiceValue;
use crate::native::database::{deserialize_choices, serialize_choices};
use alloc::string::String;
use alloc::vec::Vec;

/// `payload` is the raw [`serialize_choices`] output.
const PREFIX_RAW: u8 = 0;
/// `payload` is the zlib compression of the [`serialize_choices`] output.
const PREFIX_ZLIB: u8 = 1;
/// `payload` is the raw [`encode_nd_state`] output.
const PREFIX_ND_RAW: u8 = 2;
/// `payload` is the zlib compression of the [`encode_nd_state`] output.
const PREFIX_ND_ZLIB: u8 = 3;

/// Leading bytes of the nondeterministic state format: an impossible
/// [`serialize_choices`] count, so pre-v2 readers reject the entry.
const ND_STATE_MAGIC: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
/// Version byte following [`ND_STATE_MAGIC`].
const ND_STATE_VERSION: u8 = 2;
/// Sanity cap on the timeline count a decoded state may claim. It is
/// deliberately looser than the write-side pool cap
/// ([`POOL_CAP`](crate::native::nd::POOL_CAP), 10) so that raising the pool
/// cap later does not invalidate stored corpora: entries written with more
/// timelines than the current cap remain decodable.
const ND_STATE_MAX_TIMELINES: u32 = 64;

/// zlib compression level used by [`encode_failure`]. 6 is the zlib default.
const ZLIB_LEVEL: u8 = 6;

/// Upper bound on the decompressed size of a zlib payload, so a hostile blob
/// cannot force an arbitrarily large allocation. The largest choice-only
/// state the decoder would otherwise accept is [`ND_STATE_MAX_TIMELINES`]
/// (64) timelines × [`BUFFER_SIZE`](crate::native::core::BUFFER_SIZE) (8192)
/// choices × [`serialize_choices`]' ~17-byte per-choice sizing = 8.5 MiB.
/// 16 MiB leaves comparable headroom for content-carrying choices (bytes and
/// strings also serialize their payloads).
const MAX_DECOMPRESSED_LEN: usize = 16 << 20;

/// Encode a choice sequence into a failure blob (see the module docs for the
/// format). The returned string is safe to embed in source as a string
/// literal and to round-trip through [`decode_blob`].
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

/// The replay state a nondeterministic failure persists (blob prefix 2/3,
/// or a version-2 database entry): every stored timeline, an entropy seed
/// for a deterministic single replay, and the continuation budget beyond
/// the incumbent's length.
pub(crate) struct NdReproState {
    /// Stored failing timelines, incumbent first.
    pub(crate) timelines: Vec<Vec<ChoiceValue>>,
    /// Seed for the fresh draws a single blob replay may need past a
    /// divergence.
    pub(crate) entropy: u64,
    /// Fresh-draw budget beyond the incumbent's flattened length.
    pub(crate) extension: u32,
}

impl NdReproState {
    pub(crate) fn incumbent(&self) -> &[ChoiceValue] {
        &self.timelines[0]
    }
}

/// Encode nondeterministic replay state (see the module docs). The output
/// is both the version-2 database entry format and the payload behind blob
/// prefixes 2/3.
pub(crate) fn encode_nd_state(state: &NdReproState) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&ND_STATE_MAGIC);
    buf.push(ND_STATE_VERSION);
    buf.extend_from_slice(&state.entropy.to_le_bytes());
    buf.extend_from_slice(&state.extension.to_le_bytes());
    buf.extend_from_slice(&(state.timelines.len() as u32).to_le_bytes());
    for timeline in &state.timelines {
        let bytes = serialize_choices(timeline);
        buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&bytes);
    }
    buf
}

/// Decode [`encode_nd_state`] output, or `None` on any malformation —
/// wrong magic or version, truncation, a zero or absurd timeline count, or
/// a timeline [`deserialize_choices`] rejects.
pub(crate) fn decode_nd_state(bytes: &[u8]) -> Option<NdReproState> {
    let rest = bytes.strip_prefix(&ND_STATE_MAGIC)?;
    let (&version, rest) = rest.split_first()?;
    if version != ND_STATE_VERSION {
        return None;
    }
    let (entropy_bytes, rest) = rest.split_first_chunk::<8>()?;
    let entropy = u64::from_le_bytes(*entropy_bytes);
    let (extension_bytes, rest) = rest.split_first_chunk::<4>()?;
    let extension = u32::from_le_bytes(*extension_bytes);
    let (count_bytes, mut rest) = rest.split_first_chunk::<4>()?;
    let count = u32::from_le_bytes(*count_bytes);
    if count == 0 || count > ND_STATE_MAX_TIMELINES {
        return None;
    }
    let mut timelines = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (len_bytes, tail) = rest.split_first_chunk::<4>()?;
        let len = u32::from_le_bytes(*len_bytes) as usize;
        if tail.len() < len {
            return None;
        }
        let (body, tail) = tail.split_at(len);
        timelines.push(deserialize_choices(body)?);
        rest = tail;
    }
    Some(NdReproState {
        timelines,
        entropy,
        extension,
    })
}

/// Encode nondeterministic replay state into a failure blob (prefix 2/3),
/// the counterpart of [`encode_failure`] for flaky failures.
pub(crate) fn encode_nd_failure(state: &NdReproState) -> String {
    let raw = encode_nd_state(state);
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, ZLIB_LEVEL);

    let (prefix, body) = if compressed.len() < raw.len() {
        (PREFIX_ND_ZLIB, compressed)
    } else {
        (PREFIX_ND_RAW, raw)
    };

    let mut payload = Vec::with_capacity(body.len() + 1);
    payload.push(prefix);
    payload.extend_from_slice(&body);
    base64_encode(&payload)
}

/// A decoded failure blob: a plain choice sequence (prefixes 0/1) or
/// nondeterministic replay state (prefixes 2/3).
pub(crate) enum DecodedBlob {
    Choices(Vec<ChoiceValue>),
    Nd(NdReproState),
}

/// Decode any failure blob, or `None` if the blob is malformed, truncated,
/// compressed with a corrupt stream, carries an unknown prefix byte, or
/// decodes to bytes the inner decoder rejects.
pub(crate) fn decode_blob(blob: &str) -> Option<DecodedBlob> {
    let bytes = base64_decode(blob)?;
    let (&prefix, rest) = bytes.split_first()?;
    match prefix {
        PREFIX_RAW => Some(DecodedBlob::Choices(deserialize_choices(rest)?)),
        PREFIX_ZLIB => {
            let raw =
                miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(rest, MAX_DECOMPRESSED_LEN)
                    .ok()?;
            Some(DecodedBlob::Choices(deserialize_choices(&raw)?))
        }
        PREFIX_ND_RAW => Some(DecodedBlob::Nd(decode_nd_state(rest)?)),
        PREFIX_ND_ZLIB => {
            let raw =
                miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(rest, MAX_DECOMPRESSED_LEN)
                    .ok()?;
            Some(DecodedBlob::Nd(decode_nd_state(&raw)?))
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/blob_tests.rs"]
mod tests;

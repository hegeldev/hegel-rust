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
//! - `0` (`PREFIX_RAW`): `payload` is the raw [`serialize_choices`] bytes of
//!   one choice sequence.
//! - `1` (`PREFIX_ZLIB`): `payload` is the zlib compression of those bytes.
//! - `2` (`PREFIX_ND_RAW`) / `3` (`PREFIX_ND_ZLIB`): `payload` is the raw /
//!   zlib-compressed [nondeterministic replay state](NdReproState) — the
//!   counterexample graph, an entropy seed, and the length its continuation
//!   budget is sized from, so a flaky failure can be replayed until it
//!   reproduces rather than exactly once.
//!
//! [`encode_failure`] computes both and keeps whichever is shorter — for the
//! tiny choice sequences a shrunk counterexample usually has, the zlib header
//! overhead loses and the raw form wins (so most blobs carry prefix `0`); for
//! large sequences the compressed form wins, except past
//! [`MAX_DECOMPRESSED_LEN`], where the raw form is kept so [`decode_blob`]'s
//! inflation bound never rejects the encoder's own output. The inner
//! `serialize_choices` encoding (see [`crate::native::database`]) is Hegel's
//! own, so it is only guaranteed to reproduce a failure within a specific
//! version of Hegel.
//!
//! [`decode_blob`] reverses every step and returns `None` on *any*
//! malformation (bad base64, unknown prefix byte, corrupt zlib stream, or a
//! payload the inner decoder rejects). Callers treat `None` as "this blob
//! can't be replayed" and panic. Compressed payloads decode under the
//! [`MAX_DECOMPRESSED_LEN`] bound, and a stream that inflates past it counts
//! as malformed.
//!
//! Every blob the encoders return decodes: the one input they refuse
//! (returning `None`) is a sequence whose clone values nest deeper than
//! [`MAX_CLONE_DEPTH`](crate::native::core::MAX_CLONE_DEPTH), which
//! [`serialize_choices`] rejects for the same reason the decoder does. The
//! engine never produces one.
//!
//! The nondeterministic state bytes double as the version-3 **database
//! entry** format: whether an entry is nondeterministic is carried by its
//! representation, so the database needs no side table of kinds.
//! They open with a `u32::MAX` choice count no genuine [`serialize_choices`]
//! output can start with, so a pre-v2 reader's [`deserialize_choices`]
//! rejects them as corrupt instead of misreading them. Version-2 entries
//! (timeline pools) are no longer read: they decode as corrupt and are
//! deleted by the reuse phase's hygiene.

use crate::native::base64::{base64_decode, base64_encode};
use crate::native::core::ChoiceValue;
use crate::native::database::{deserialize_choices, serialize_choices};
use crate::native::graph::Graph;
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
const ND_STATE_VERSION: u8 = 3;

/// zlib compression level used by [`encode_failure`]. 6 is the zlib default.
const ZLIB_LEVEL: u8 = 6;

/// Upper bound on the decompressed size of a zlib payload, so a hostile blob
/// cannot force an arbitrarily large allocation. A choice sequence under the
/// default [`BUFFER_SIZE`](crate::native::core::BUFFER_SIZE) (2^20) bound
/// reaches about 17 MiB at [`serialize_choices`]' ~17-byte per-choice
/// sizing, and a stored graph is a shrunk failure's few runs of such draws
/// with their addresses; 64 MiB leaves headroom for content-carrying choices
/// (bytes and strings also serialize their payloads). Encoders keep the raw
/// form for payloads past this bound, so their output always decodes.
const MAX_DECOMPRESSED_LEN: usize = 64 << 20;

/// Encode a choice sequence into a failure blob (see the module docs for the
/// format). The returned string is safe to embed in source as a string
/// literal and to round-trip through [`decode_blob`]. Returns `None` only
/// for a sequence [`serialize_choices`] rejects (clone values nested deeper
/// than [`MAX_CLONE_DEPTH`](crate::native::core::MAX_CLONE_DEPTH)), which no
/// blob could represent.
pub fn encode_failure(choices: &[ChoiceValue]) -> Option<String> {
    let raw = serialize_choices(choices)?;
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, ZLIB_LEVEL);

    let (prefix, body) = if compressed.len() < raw.len() && raw.len() <= MAX_DECOMPRESSED_LEN {
        (PREFIX_ZLIB, compressed)
    } else {
        (PREFIX_RAW, raw)
    };

    let mut payload = Vec::with_capacity(body.len() + 1);
    payload.push(prefix);
    payload.extend_from_slice(&body);
    Some(base64_encode(&payload))
}

/// The replay state a nondeterministic failure persists (blob prefix 2/3,
/// or a version-3 database entry): the counterexample graph,
/// an entropy seed for a deterministic single replay, and the flattened
/// length of the longest failing run the graph holds, which sizes the
/// continuation budget of a replay.
pub(crate) struct NdReproState {
    pub(crate) graph: Graph,
    /// Seed for the fresh draws a single blob replay may need past a
    /// divergence.
    pub(crate) entropy: u64,
    /// The longest stored failing run's flattened length: a replay draws
    /// at random past the graph up to
    /// [`continuation_budget`](crate::native::nd::continuation_budget) of
    /// it.
    pub(crate) longest: u32,
}

/// Encode nondeterministic replay state (see the module docs). The output
/// is both the version-3 database entry format and the payload behind blob
/// prefixes 2/3.
pub(crate) fn encode_nd_state(state: &NdReproState) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&ND_STATE_MAGIC);
    buf.push(ND_STATE_VERSION);
    buf.extend_from_slice(&state.entropy.to_le_bytes());
    buf.extend_from_slice(&state.longest.to_le_bytes());
    buf.extend_from_slice(&state.graph.encode()?);
    Some(buf)
}

/// Decode [`encode_nd_state`] output, or `None` on any malformation —
/// wrong magic or version, truncation, or a graph body [`Graph::decode`]
/// rejects.
pub(crate) fn decode_nd_state(bytes: &[u8]) -> Option<NdReproState> {
    let rest = bytes.strip_prefix(&ND_STATE_MAGIC)?;
    let (&version, rest) = rest.split_first()?;
    if version != ND_STATE_VERSION {
        return None;
    }
    let (entropy_bytes, rest) = rest.split_first_chunk::<8>()?;
    let entropy = u64::from_le_bytes(*entropy_bytes);
    let (longest_bytes, rest) = rest.split_first_chunk::<4>()?;
    let longest = u32::from_le_bytes(*longest_bytes);
    Some(NdReproState {
        graph: Graph::decode(rest)?,
        entropy,
        longest,
    })
}

/// Encode nondeterministic replay state into a failure blob (prefix 2/3),
/// the counterpart of [`encode_failure`] for flaky failures.
pub(crate) fn encode_nd_failure(state: &NdReproState) -> Option<String> {
    let raw = encode_nd_state(state)?;
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, ZLIB_LEVEL);

    let (prefix, body) = if compressed.len() < raw.len() && raw.len() <= MAX_DECOMPRESSED_LEN {
        (PREFIX_ND_ZLIB, compressed)
    } else {
        (PREFIX_ND_RAW, raw)
    };

    let mut payload = Vec::with_capacity(body.len() + 1);
    payload.push(prefix);
    payload.extend_from_slice(&body);
    Some(base64_encode(&payload))
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

/// [`decode_blob`] narrowed to a plain choice sequence: `None` for a
/// malformed blob and for nondeterministic replay state, which
/// `hegel_test_case_from_blob` cannot replay as a single case.
#[cfg(test)]
pub(crate) fn decode_failure(blob: &str) -> Option<Vec<ChoiceValue>> {
    match decode_blob(blob)? {
        DecodedBlob::Choices(choices) => Some(choices),
        DecodedBlob::Nd(_) => None,
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/blob_tests.rs"]
mod tests;

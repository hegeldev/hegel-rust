/// guaranteed to reproduce a failure within a specific version of Hegel.
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

use super::{generate_from_schema, Generate};
use serde_json::{json, Value};

#[cfg(test)]
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[cfg(test)]
fn base64_encode(input: &[u8]) -> String {
    let mut result = String::with_capacity((input.len() + 2) / 3 * 4);

    for chunk in input.chunks(3) {
        // 3 bytes (3x8=24 bits) -> 4 base64 chars (4x6=24 bits)
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        result.push(BASE64_ALPHABET[(b0 >> 2) as usize] as char);
        result.push(BASE64_ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);

        if chunk.len() > 1 {
            result.push(BASE64_ALPHABET[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(BASE64_ALPHABET[(b2 & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

fn base64_char_value(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn base64_decode(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut result = Vec::with_capacity((bytes.len() * 3) / 4);

    for chunk in bytes.chunks(4) {
        if chunk.len() < 4 {
            break;
        }

        let a = base64_char_value(chunk[0]).unwrap_or(0);
        let b = base64_char_value(chunk[1]).unwrap_or(0);
        let c = base64_char_value(chunk[2]).unwrap_or(0);
        let d = base64_char_value(chunk[3]).unwrap_or(0);

        // 4 base64 chars (4x6=24 bits) -> 3 bytes (3x8=24 bits)
        result.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            result.push(((b & 0x0F) << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            result.push(((c & 0x03) << 6) | d);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{gen, hegel, Hegel};

    #[test]
    fn test_base64_roundtrip() {
        Hegel::new(|| {
            let input = gen::binary().generate();
            let encoded = base64_encode(&input);
            let decoded = base64_decode(&encoded);
            assert_eq!(input, decoded);
        })
        .test_cases(100)
        .run();
    }

    #[test]
    fn test_base64_explicit() {
        // RFC 4648 test vectors
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}

/// Generator for binary data (byte sequences).
pub struct BinaryGenerator {
    min_size: usize,
    max_size: Option<usize>,
}

impl BinaryGenerator {
    /// Set the minimum size in bytes.
    pub fn with_min_size(mut self, min: usize) -> Self {
        self.min_size = min;
        self
    }

    /// Set the maximum size in bytes.
    pub fn with_max_size(mut self, max: usize) -> Self {
        self.max_size = Some(max);
        self
    }
}

impl Generate<Vec<u8>> for BinaryGenerator {
    fn generate(&self) -> Vec<u8> {
        let b64: String = generate_from_schema(&self.schema().unwrap());
        base64_decode(&b64)
    }

    fn schema(&self) -> Option<Value> {
        let mut schema = json!({"type": "binary"});

        if self.min_size > 0 {
            schema["min_size"] = json!(self.min_size);
        }

        if let Some(max) = self.max_size {
            schema["max_size"] = json!(max);
        }

        Some(schema)
    }
}

/// Generate binary data (byte sequences).
///
/// # Example
///
/// ```no_run
/// use hegel::gen::{self, Generate};
///
/// // Generate any byte sequence
/// let gen = gen::binary();
///
/// // Generate 16-32 bytes
/// let gen = gen::binary().with_min_size(16).with_max_size(32);
/// ```
pub fn binary() -> BinaryGenerator {
    BinaryGenerator {
        min_size: 0,
        max_size: None,
    }
}

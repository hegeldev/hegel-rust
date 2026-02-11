use super::{BasicGenerator, Generate};
use crate::cbor_helpers::{cbor_map, map_insert};
use ciborium::Value;
use std::sync::OnceLock;

#[cfg(test)]
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[cfg(test)]
fn base64_encode(input: &[u8]) -> String {
    let mut result = String::with_capacity(input.len().div_ceil(3) * 4);

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

#[derive(Debug, PartialEq)]
enum Base64Error {
    InvalidLength(usize),
    InvalidCharacter(char),
}

fn base64_char_value(c: u8) -> Result<u8, Base64Error> {
    match c {
        b'A'..=b'Z' => Ok(c - b'A'),
        b'a'..=b'z' => Ok(c - b'a' + 26),
        b'0'..=b'9' => Ok(c - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        b'=' => Ok(0), // Padding, value doesn't matter
        _ => Err(Base64Error::InvalidCharacter(c as char)),
    }
}

fn base64_decode(input: &str) -> Result<Vec<u8>, Base64Error> {
    if input.is_empty() {
        return Ok(vec![]);
    }

    if input.len() % 4 != 0 {
        return Err(Base64Error::InvalidLength(input.len()));
    }

    let bytes = input.as_bytes();
    let mut result = Vec::with_capacity((bytes.len() * 3) / 4);

    for chunk in bytes.chunks(4) {
        let a = base64_char_value(chunk[0])?;
        let b = base64_char_value(chunk[1])?;
        let c = base64_char_value(chunk[2])?;
        let d = base64_char_value(chunk[3])?;

        // 4 base64 chars (4x6=24 bits) -> 3 bytes (3x8=24 bits)
        result.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            result.push(((b & 0x0F) << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            result.push(((c & 0x03) << 6) | d);
        }
    }

    Ok(result)
}

/// Generator for binary data (byte sequences).
pub struct BinaryGenerator {
    min_size: usize,
    max_size: Option<usize>,
    cached_basic: OnceLock<Option<BasicGenerator<Vec<u8>>>>,
}

impl BinaryGenerator {
    /// Set the minimum size in bytes.
    pub fn with_min_size(mut self, min: usize) -> Self {
        self.min_size = min;
        self.cached_basic = OnceLock::new();
        self
    }

    /// Set the maximum size in bytes.
    pub fn with_max_size(mut self, max: usize) -> Self {
        self.max_size = Some(max);
        self.cached_basic = OnceLock::new();
        self
    }
}

impl Generate<Vec<u8>> for BinaryGenerator {
    fn generate(&self) -> Vec<u8> {
        self.as_basic().unwrap().generate()
    }

    fn as_basic(&self) -> Option<BasicGenerator<Vec<u8>>> {
        self.cached_basic
            .get_or_init(|| {
                let mut schema = cbor_map! {
                    "type" => "binary",
                    "min_size" => self.min_size as u64
                };

                if let Some(max) = self.max_size {
                    map_insert(&mut schema, "max_size", Value::from(max as u64));
                }

                Some(BasicGenerator::with_transform(schema, |raw| {
                    let b64 = match raw {
                        Value::Text(s) => s,
                        _ => panic!("Expected text (base64) from binary schema, got {:?}", raw),
                    };
                    base64_decode(&b64).expect("invalid base64")
                }))
            })
            .clone()
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
        cached_basic: OnceLock::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{gen, Hegel};

    #[test]
    fn test_base64_roundtrip() {
        Hegel::new(|| {
            let input = gen::binary().generate();
            let encoded = base64_encode(&input);
            let decoded = base64_decode(&encoded).unwrap();
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

        // And decode them back
        assert_eq!(base64_decode("").unwrap(), Vec::<u8>::new());
        assert_eq!(base64_decode("Zg==").unwrap(), b"f".to_vec());
        assert_eq!(base64_decode("Zm8=").unwrap(), b"fo".to_vec());
        assert_eq!(base64_decode("Zm9v").unwrap(), b"foo".to_vec());
    }

    #[test]
    fn test_base64_decode_errors() {
        // Invalid length (not a multiple of 4)
        assert_eq!(base64_decode("Z"), Err(Base64Error::InvalidLength(1)));
        assert_eq!(base64_decode("Zm"), Err(Base64Error::InvalidLength(2)));
        assert_eq!(base64_decode("Zm9"), Err(Base64Error::InvalidLength(3)));
        assert_eq!(base64_decode("Zm9vY"), Err(Base64Error::InvalidLength(5)));

        // Invalid characters
        assert_eq!(
            base64_decode("!!!!"),
            Err(Base64Error::InvalidCharacter('!'))
        );
        assert_eq!(
            base64_decode("Zm9@"),
            Err(Base64Error::InvalidCharacter('@'))
        );
    }
}

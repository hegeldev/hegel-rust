use super::{BasicGenerator, Generate};
use crate::cbor_helpers::{cbor_map, map_insert};
use ciborium::Value;

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

impl BinaryGenerator {
    fn build_schema(&self) -> Value {
        let mut schema = cbor_map! {
            "type" => "binary",
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", Value::from(max as u64));
        }

        schema
    }
}

fn parse_binary(raw: Value) -> Vec<u8> {
    match raw {
        Value::Bytes(bytes) => bytes,
        _ => panic!(
            "Expected CBOR byte string from binary schema, got {:?}",
            raw
        ),
    }
}

impl Generate<Vec<u8>> for BinaryGenerator {
    fn generate(&self) -> Vec<u8> {
        parse_binary(super::generate_raw(&self.build_schema()))
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Vec<u8>>> {
        Some(BasicGenerator::new(self.build_schema(), parse_binary))
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

#[cfg(test)]
mod tests {
    use crate::{gen, gen::Generate, Hegel};

    #[test]
    fn test_binary_generation() {
        Hegel::new(|| {
            let data = gen::binary().with_max_size(50).generate();
            assert!(data.len() <= 50);
        })
        .test_cases(100)
        .run();
    }
}

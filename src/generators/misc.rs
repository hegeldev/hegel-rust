use super::{BasicGenerator, Generator, TestCase, generate_from_schema};
use crate::cbor_utils::{cbor_map, cbor_serialize};
use ciborium::Value;

/// Generate the unit value `()`.
// nocov start
pub fn unit() -> JustGenerator<()> {
    just(())
    // nocov end
}

/// Generator that always produces the same value. Created by [`just()`].
pub struct JustGenerator<T> {
    value: T,
}

impl<T: Clone + Send + Sync> Generator<T> for JustGenerator<T> {
    fn do_draw(&self, _tc: &TestCase) -> T {
        self.value.clone()
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        let value = self.value.clone();
        Some(BasicGenerator::new(
            cbor_map! {"type" => "constant", "value" => Value::Null},
            move |_| value.clone(),
        ))
    }
}

/// Generate a constant value.
pub fn just<T: Clone + Send + Sync>(value: T) -> JustGenerator<T> {
    JustGenerator { value }
}

/// Generator for boolean values. Created by [`booleans()`].
pub struct BoolGenerator;

impl Generator<bool> for BoolGenerator {
    fn do_draw(&self, tc: &TestCase) -> bool {
        super::generate_from_schema(tc, &cbor_map! {"type" => "boolean"})
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, bool>> {
        Some(BasicGenerator::new(
            cbor_map! {"type" => "boolean"},
            super::deserialize_value,
        ))
    }
}

/// Generate boolean values.
pub fn booleans() -> BoolGenerator {
    BoolGenerator
}

/// Generator for UUID values (as `u128`). Created by [`uuids()`].
pub struct UuidsGenerator {
    version: Option<u8>,
    allow_nil: bool,
}

impl UuidsGenerator {
    /// Restrict to UUIDs of a specific version (1–5).
    pub fn version(mut self, version: u8) -> Self {
        self.version = Some(version);
        self
    }

    /// Allow generating the nil UUID (all zeros).
    pub fn allow_nil(mut self, allow_nil: bool) -> Self {
        self.allow_nil = allow_nil;
        self
    }
}

impl Generator<u128> for UuidsGenerator {
    fn do_draw(&self, tc: &TestCase) -> u128 {
        assert!(
            !(self.allow_nil && self.version.is_some()),
            "The nil UUID is not of any version"
        );

        if self.allow_nil {
            // With 1/10 probability, return the nil UUID.
            let choice: i64 = generate_from_schema(
                tc,
                &cbor_map! {
                    "type" => "integer",
                    "min_value" => cbor_serialize(&0i64),
                    "max_value" => cbor_serialize(&9i64)
                },
            );
            if choice == 0 {
                return 0u128;
            }
        }

        // Generate a non-nil UUID: integer in [1, u128::MAX].
        let mut uuid: u128 = generate_from_schema(
            tc,
            &cbor_map! {
                "type" => "integer",
                "min_value" => cbor_serialize(&1u128),
                "max_value" => cbor_serialize(&u128::MAX)
            },
        );

        if let Some(version) = self.version {
            // Upper nibble of byte 6 (bits 79–76) is the version field.
            uuid = (uuid & !(0xF_u128 << 76)) | ((version as u128) << 76);
            // Top 2 bits of byte 8 (bits 63–62) are the variant field; set to 10 (RFC 4122).
            uuid = (uuid & !(0x3_u128 << 62)) | (0x2_u128 << 62);
        }

        uuid
    }
}

/// Generate UUID values as `u128` integers.
pub fn uuids() -> UuidsGenerator {
    UuidsGenerator {
        version: None,
        allow_nil: false,
    }
}

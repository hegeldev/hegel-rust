use super::{BasicGenerator, Generator, TestCase};
use crate::utils::cbor_utils::cbor_map;
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

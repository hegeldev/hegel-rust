use super::{BasicGenerator, Generate, TestCaseData};
use crate::cbor_helpers::cbor_map;
use ciborium::Value;

pub fn unit() -> JustGenerator<()> {
    just(())
}

pub struct JustGenerator<T> {
    value: T,
}

impl<T: Clone + Send + Sync> Generate<T> for JustGenerator<T> {
    fn do_draw(&self, _data: &TestCaseData) -> T {
        self.value.clone()
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        let value = self.value.clone();
        Some(BasicGenerator::new(
            cbor_map! {"const" => Value::Null},
            move |_| value.clone(),
        ))
    }
}

pub fn just<T: Clone + Send + Sync>(value: T) -> JustGenerator<T> {
    JustGenerator { value }
}

pub struct BoolGenerator;

impl Generate<bool> for BoolGenerator {
    fn do_draw(&self, data: &TestCaseData) -> bool {
        data.generate_from_schema(&cbor_map! {"type" => "boolean"})
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, bool>> {
        Some(BasicGenerator::new(
            cbor_map! {"type" => "boolean"},
            super::deserialize_value,
        ))
    }
}

pub fn booleans() -> BoolGenerator {
    BoolGenerator
}

use super::{BasicGenerator, Generator, TestCaseData};
use crate::cbor_utils::cbor_map;
use ciborium::Value;

pub fn unit() -> JustGenerator<()> {
    just(())
}

pub struct JustGenerator<T> {
    value: T,
}

impl<T: Clone + Send + Sync> Generator<T> for JustGenerator<T> {
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

pub struct NoneGenerator<T> {
    _phantom: std::marker::PhantomData<fn() -> T>,
}

impl<T: Send + Sync> Generator<Option<T>> for NoneGenerator<T> {
    fn do_draw(&self, _data: &TestCaseData) -> Option<T> {
        None
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Option<T>>> {
        Some(BasicGenerator::new(
            cbor_map! {"const" => Value::Null},
            |_| None,
        ))
    }
}

pub fn none<T: Send + Sync>() -> NoneGenerator<T> {
    NoneGenerator {
        _phantom: std::marker::PhantomData,
    }
}

pub struct BoolGenerator;

impl Generator<bool> for BoolGenerator {
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

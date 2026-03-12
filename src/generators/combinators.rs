use super::{integers, labels, BasicGenerator, BoxedGenerator, Generator, TestCaseData};
use crate::cbor_utils::{cbor_array, cbor_map};
use ciborium::Value;
use std::marker::PhantomData;

pub struct SampledFromGenerator<T> {
    elements: Vec<T>,
}

impl<T: Clone + Send + Sync> Generator<T> for SampledFromGenerator<T> {
    fn do_draw(&self, data: &TestCaseData) -> T {
        if let Some(basic) = self.as_basic() {
            return basic.do_draw(data);
        }

        let indices = integers::<usize>()
            .min_value(0)
            .max_value(self.elements.len() - 1);
        let index = indices.do_draw(data);
        self.elements[index].clone()
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        if self.elements.is_empty() {
            return None;
        }

        let schema = cbor_map! {
            "type" => "integer",
            "min_value" => 0u64,
            "max_value" => (self.elements.len() - 1) as u64
        };
        let elements = self.elements.clone();
        Some(BasicGenerator::new(schema, move |raw| {
            let index: usize = super::deserialize_value(raw);
            elements[index].clone()
        }))
    }
}

pub fn sampled_from<T: Clone + Send + Sync>(elements: Vec<T>) -> SampledFromGenerator<T> {
    assert!(
        !elements.is_empty(),
        "Collection passed to sampled_from cannot be empty"
    );
    SampledFromGenerator { elements }
}

pub struct OneOfGenerator<'a, T> {
    generators: Vec<BoxedGenerator<'a, T>>,
}

impl<T> Generator<T> for OneOfGenerator<'_, T> {
    fn do_draw(&self, data: &TestCaseData) -> T {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            data.start_span(labels::ONE_OF);
            let index = integers::<usize>()
                .min_value(0)
                .max_value(self.generators.len() - 1)
                .do_draw(data);
            let result = self.generators[index].do_draw(data);
            data.stop_span(false);
            result
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        let basics: Vec<BasicGenerator<'_, T>> = self
            .generators
            .iter()
            .map(|g| g.as_basic())
            .collect::<Option<Vec<_>>>()?;

        let tagged_schemas: Vec<Value> = basics
            .iter()
            .enumerate()
            .map(|(i, b)| {
                cbor_map! {
                    "type" => "tuple",
                    "elements" => cbor_array![
                        cbor_map!{"const" => Value::Integer(ciborium::value::Integer::from(i as i64))},
                        b.schema().clone()
                    ]
                }
            })
            .collect();

        let schema = cbor_map! {"one_of" => Value::Array(tagged_schemas)};

        Some(BasicGenerator::new(schema, move |raw| {
            let arr = match raw {
                Value::Array(arr) => arr,
                _ => panic!("Expected array from tagged tuple, got {:?}", raw),
            };
            let tag = match &arr[0] {
                Value::Integer(i) => {
                    let val: i128 = (*i).into();
                    val as usize
                }
                _ => panic!("Expected integer tag, got {:?}", arr[0]),
            };
            let value = arr.into_iter().nth(1).unwrap();
            basics[tag].parse_raw(value)
        }))
    }
}

/// Choose from multiple generators of the same type.
///
/// For a more convenient syntax, use the `one_of!` macro instead.
pub fn one_of<T>(generators: Vec<BoxedGenerator<'_, T>>) -> OneOfGenerator<'_, T> {
    assert!(
        !generators.is_empty(),
        "one_of requires at least one generator"
    );
    OneOfGenerator { generators }
}

/// Choose from multiple generators of the same type.
///
/// This macro automatically boxes each generator, providing a more ergonomic
/// syntax than calling [`one_of`] directly.
///
/// # Example
///
/// ```no_run
/// use hegel::generators;
///
/// #[hegel::test]
/// fn my_test() {
///     let value: i32 = hegel::draw(&hegel::one_of!(
///         generators::integers::<i32>().min_value(0).max_value(10),
///         generators::integers::<i32>().min_value(100).max_value(110),
///     ));
/// }
/// ```
#[macro_export]
macro_rules! one_of {
    ($($gen:expr),+ $(,)?) => {
        $crate::generators::one_of(vec![
            $($crate::generators::Generator::boxed($gen)),+
        ])
    };
}

pub struct OptionalGenerator<G, T> {
    inner: G,
    _phantom: PhantomData<fn(T)>,
}

impl<T, G> Generator<Option<T>> for OptionalGenerator<G, T>
where
    G: Generator<T>,
{
    fn do_draw(&self, data: &TestCaseData) -> Option<T> {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            data.start_span(labels::OPTIONAL);
            let is_some: bool = data.generate_from_schema(&cbor_map! {"type" => "boolean"});
            let result = if is_some {
                Some(self.inner.do_draw(data))
            } else {
                None
            };
            data.stop_span(false);
            result
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Option<T>>> {
        let inner_basic = self.inner.as_basic()?;
        let inner_schema = inner_basic.schema().clone();

        let null_schema = cbor_map! {
            "type" => "tuple",
            "elements" => cbor_array![
                cbor_map!{"const" => Value::Integer(0.into())},
                cbor_map!{"type" => "null"}
            ]
        };
        let value_schema = cbor_map! {
            "type" => "tuple",
            "elements" => cbor_array![
                cbor_map!{"const" => Value::Integer(1.into())},
                inner_schema
            ]
        };

        let schema = cbor_map! {"one_of" => cbor_array![null_schema, value_schema]};

        Some(BasicGenerator::new(schema, move |raw| {
            let arr = match raw {
                Value::Array(arr) => arr,
                _ => panic!("Expected array from tagged tuple, got {:?}", raw),
            };
            let tag = match &arr[0] {
                Value::Integer(i) => {
                    let val: i128 = (*i).into();
                    val as usize
                }
                _ => panic!("Expected integer tag, got {:?}", arr[0]),
            };

            if tag == 0 {
                None
            } else {
                let value = arr.into_iter().nth(1).unwrap();
                Some(inner_basic.parse_raw(value))
            }
        }))
    }
}

pub fn optional<T, G: Generator<T>>(inner: G) -> OptionalGenerator<G, T> {
    OptionalGenerator {
        inner,
        _phantom: PhantomData,
    }
}

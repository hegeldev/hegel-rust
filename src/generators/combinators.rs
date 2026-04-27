use super::{BasicGenerator, BoxedGenerator, Generator, TestCase, integers, labels};
use crate::cbor_utils::{cbor_array, cbor_map};
use ciborium::Value;
use std::borrow::Cow;
use std::marker::PhantomData;

/// Generator that picks from a fixed list of values. Created by [`sampled_from()`].
pub struct SampledFromGenerator<'a, T: Clone> {
    elements: Cow<'a, [T]>,
}

impl<'a, T: Clone + Send + Sync + 'a> Generator<T> for SampledFromGenerator<'a, T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        if let Some(basic) = self.as_basic() {
            return basic.do_draw(tc);
        }

        // nocov start
        let indices = integers::<usize>()
            .min_value(0)
            .max_value(self.elements.len() - 1);
        let index = indices.do_draw(tc);
        self.elements[index].clone()
        // nocov end
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        if self.elements.is_empty() {
            return None; // nocov
        }

        let schema = cbor_map! {
            "type" => "integer",
            "min_value" => 0u64,
            "max_value" => (self.elements.len() - 1) as u64
        };
        let elements: &[T] = &self.elements;
        Some(BasicGenerator::new(schema, move |raw| {
            let index: usize = super::deserialize_value(raw);
            elements[index].clone()
        }))
    }
}

/// Pick from a fixed list of values.
///
/// Accepts anything convertible into `Cow<[T]>`, including:
/// - `Vec<T>` (consumed without re-allocation)
/// - `&[T]` where `T: Clone` (borrowed, zero allocation)
/// - `&Vec<T>` or `&[T; N]` (via coercion to `&[T]`)
///
/// Panics if `elements` is empty.
pub fn sampled_from<'a, T, S>(elements: S) -> SampledFromGenerator<'a, T>
where
    T: Clone + Send + Sync,
    S: Into<Cow<'a, [T]>>,
{
    let elements = elements.into();
    assert!(
        !elements.is_empty(),
        "Collection passed to sampled_from cannot be empty"
    );
    SampledFromGenerator { elements }
}

/// Generator that chooses from multiple generators. Created by [`one_of()`] or [`one_of!`](crate::one_of).
pub struct OneOfGenerator<'a, T> {
    generators: Vec<BoxedGenerator<'a, T>>,
}

impl<T> Generator<T> for OneOfGenerator<'_, T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(tc)
        } else {
            tc.start_span(labels::ONE_OF);
            let index = integers::<usize>()
                .min_value(0)
                .max_value(self.generators.len() - 1)
                .do_draw(tc);
            let result = self.generators[index].do_draw(tc);
            tc.stop_span(false);
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
                        cbor_map!{"type" => "constant", "value" => Value::Integer(ciborium::value::Integer::from(i as i64))},
                        b.schema().clone()
                    ]
                }
            })
            .collect();

        let schema = cbor_map! {"type" => "one_of", "generators" => Value::Array(tagged_schemas)};

        Some(BasicGenerator::new(schema, move |raw| {
            let arr = match raw {
                Value::Array(arr) => arr,
                _ => panic!("Expected array from tagged tuple, got {:?}", raw), // nocov
            };
            let tag = match &arr[0] {
                Value::Integer(i) => {
                    let val: i128 = (*i).into();
                    val as usize
                }
                _ => panic!("Expected integer tag, got {:?}", arr[0]), // nocov
            };
            let value = arr.into_iter().nth(1).unwrap();
            basics[tag].parse_raw(value)
        }))
    }
}

/// Choose from multiple generators of the same type.
///
/// Accepts any iterable of boxed generators (e.g. `Vec<BoxedGenerator<T>>`
/// or an iterator chain). For a more convenient syntax, use the `one_of!`
/// macro instead.
pub fn one_of<'a, T, I>(generators: I) -> OneOfGenerator<'a, T>
where
    I: IntoIterator<Item = BoxedGenerator<'a, T>>,
{
    let generators: Vec<BoxedGenerator<'a, T>> = generators.into_iter().collect();
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
/// use hegel::generators as gs;
///
/// #[hegel::test]
/// fn my_test(tc: &hegel::TestCase) {
///     let value: i32 = tc.draw(hegel::one_of!(
///         gs::integers::<i32>().min_value(0).max_value(10),
///         gs::integers::<i32>().min_value(100).max_value(110),
///     ));
/// }
/// ```
#[macro_export]
macro_rules! one_of {
    ($($generator:expr),+ $(,)?) => {
        $crate::generators::one_of(vec![
            $($crate::generators::Generator::boxed($generator)),+
        ])
    };
}

/// Generator that produces `Some(value)` or `None`. Created by [`optional()`].
pub struct OptionalGenerator<G, T> {
    inner: G,
    _phantom: PhantomData<fn(T)>,
}

impl<T, G> Generator<Option<T>> for OptionalGenerator<G, T>
where
    G: Generator<T>,
{
    fn do_draw(&self, tc: &TestCase) -> Option<T> {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(tc)
        } else {
            // nocov start
            tc.start_span(labels::OPTIONAL);
            let is_some: bool = super::generate_from_schema(tc, &cbor_map! {"type" => "boolean"});
            let result = if is_some {
                Some(self.inner.do_draw(tc))
            } else {
                None
            };
            tc.stop_span(false);
            result
            // nocov end
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Option<T>>> {
        let inner_basic = self.inner.as_basic()?;
        let inner_schema = inner_basic.schema().clone();

        let null_schema = cbor_map! {
            "type" => "tuple",
            "elements" => cbor_array![
                cbor_map!{"type" => "constant", "value" => Value::Integer(0.into())},
                cbor_map!{"type" => "null"}
            ]
        };
        let value_schema = cbor_map! {
            "type" => "tuple",
            "elements" => cbor_array![
                cbor_map!{"type" => "constant", "value" => Value::Integer(1.into())},
                inner_schema
            ]
        };

        let schema =
            cbor_map! {"type" => "one_of", "generators" => cbor_array![null_schema, value_schema]};

        Some(BasicGenerator::new(schema, move |raw| {
            let arr = match raw {
                Value::Array(arr) => arr,
                _ => panic!("Expected array from tagged tuple, got {:?}", raw), // nocov
            };
            let tag = match &arr[0] {
                Value::Integer(i) => {
                    let val: i128 = (*i).into();
                    val as usize
                }
                _ => panic!("Expected integer tag, got {:?}", arr[0]), // nocov
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

/// Generate `Option<T>` values: either `Some(value)` from the inner generator, or `None`.
pub fn optional<T, G: Generator<T>>(inner: G) -> OptionalGenerator<G, T> {
    OptionalGenerator {
        inner,
        _phantom: PhantomData,
    }
}

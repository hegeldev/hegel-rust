use super::{
    discardable_group, generate_from_schema, group, integers, labels, BasicGenerator, Generate,
};
use crate::cbor_helpers::{cbor_array, cbor_map};
use ciborium::Value;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct Mapped<T, U, F, G> {
    pub(crate) source: G,
    pub(crate) f: Arc<F>,
    pub(crate) _phantom: PhantomData<fn(T) -> U>,
}

impl<T, U, F, G> Generate<U> for Mapped<T, U, F, G>
where
    G: Generate<T>,
    F: Fn(T) -> U + Send + Sync,
{
    fn generate(&self) -> U {
        if let Some(basic) = self.as_basic() {
            basic.generate()
        } else {
            group(labels::MAPPED, || (self.f)(self.source.generate()))
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, U>> {
        let source_basic = self.source.as_basic()?;
        let f = Arc::clone(&self.f);
        Some(source_basic.map(move |t| f(t)))
    }
}

pub struct FlatMapped<T, U, G2, F, G1> {
    pub(crate) source: G1,
    pub(crate) f: F,
    pub(crate) _phantom: PhantomData<fn(T) -> (U, G2)>,
}

impl<T, U, G2, F, G1> Generate<U> for FlatMapped<T, U, G2, F, G1>
where
    G1: Generate<T>,
    G2: Generate<U>,
    F: Fn(T) -> G2 + Send + Sync,
{
    fn generate(&self) -> U {
        group(labels::FLAT_MAP, || {
            let intermediate = self.source.generate();
            let next_gen = (self.f)(intermediate);
            next_gen.generate()
        })
    }
}

pub struct Filtered<T, F, G> {
    pub(crate) source: G,
    pub(crate) predicate: F,
    pub(crate) _phantom: PhantomData<fn() -> T>,
}

impl<T, F, G> Generate<T> for Filtered<T, F, G>
where
    G: Generate<T>,
    F: Fn(&T) -> bool + Send + Sync,
{
    fn generate(&self) -> T {
        for _ in 0..3 {
            if let Some(value) = discardable_group(labels::FILTER, || {
                let value = self.source.generate();
                if (self.predicate)(&value) {
                    Some(value)
                } else {
                    None
                }
            }) {
                return value;
            }
        }
        crate::assume(false);
        unreachable!()
    }
}

// ============================================================================
// BoxedGenerator - Type-erased generator
// ============================================================================

/// A type-erased generator with a lifetime parameter.
///
/// This is useful for storing generators of different concrete types
/// in collections or struct fields.
///
/// Create a `BoxedGenerator` by calling `.boxed()` on any generator.
///
/// The lifetime `'a` represents the minimum lifetime of any borrowed data
/// in the generator. Use `'static` for generators that own all their data.
/// For generators that borrow data, the lifetime will match the borrow.
pub struct BoxedGenerator<'a, T> {
    pub(crate) inner: Arc<dyn Generate<T> + Send + Sync + 'a>,
}

impl<'a, T> Clone for BoxedGenerator<'a, T> {
    fn clone(&self) -> Self {
        BoxedGenerator {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<'a, T> Generate<T> for BoxedGenerator<'a, T> {
    fn generate(&self) -> T {
        self.inner.generate()
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        self.inner.as_basic()
    }

    /// Returns self without re-wrapping.
    fn boxed<'b>(self) -> BoxedGenerator<'b, T>
    where
        Self: Sized + Send + Sync + 'b,
    {
        BoxedGenerator { inner: self.inner }
    }
}

pub struct SampledFromGenerator<T> {
    elements: Vec<T>,
}

impl<T: Clone + Send + Sync> Generate<T> for SampledFromGenerator<T> {
    fn generate(&self) -> T {
        crate::assume(!self.elements.is_empty());

        if let Some(basic) = self.as_basic() {
            return basic.generate();
        }

        // Generate index and pick
        let idx_gen = integers::<usize>()
            .with_min(0)
            .with_max(self.elements.len() - 1);
        let idx = idx_gen.generate();
        self.elements[idx].clone()
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
            let idx: usize = super::deserialize_value(raw);
            elements[idx].clone()
        }))
    }
}

pub fn sampled_from<T: Clone + Send + Sync>(elements: Vec<T>) -> SampledFromGenerator<T> {
    SampledFromGenerator { elements }
}

pub struct OneOfGenerator<'a, T> {
    generators: Vec<BoxedGenerator<'a, T>>,
}

impl<'a, T> Generate<T> for OneOfGenerator<'a, T> {
    fn generate(&self) -> T {
        crate::assume(!self.generators.is_empty());

        if let Some(basic) = self.as_basic() {
            basic.generate()
        } else {
            // Generate index and delegate
            group(labels::ONE_OF, || {
                let idx = integers::<usize>()
                    .with_min(0)
                    .with_max(self.generators.len() - 1)
                    .generate();
                self.generators[idx].generate()
            })
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
pub fn one_of<'a, T>(generators: Vec<BoxedGenerator<'a, T>>) -> OneOfGenerator<'a, T> {
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
/// use hegel::gen::{self, Generate};
///
/// let gen = hegel::one_of!(
///     gen::integers::<i32>().with_min(0).with_max(10),
///     gen::integers::<i32>().with_min(100).with_max(110),
/// );
/// let value = gen.generate();
/// ```
#[macro_export]
macro_rules! one_of {
    ($($gen:expr),+ $(,)?) => {
        $crate::gen::one_of(vec![
            $($crate::gen::Generate::boxed($gen)),+
        ])
    };
}

pub struct OptionalGenerator<G, T> {
    inner: G,
    _phantom: PhantomData<fn(T)>,
}

impl<T, G> Generate<Option<T>> for OptionalGenerator<G, T>
where
    G: Generate<T>,
{
    fn generate(&self) -> Option<T> {
        if let Some(basic) = self.as_basic() {
            basic.generate()
        } else {
            // Compositional fallback
            group(labels::OPTIONAL, || {
                let is_some: bool = generate_from_schema(&cbor_map! {"type" => "boolean"});
                if is_some {
                    Some(self.inner.generate())
                } else {
                    None
                }
            })
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

pub fn optional<T, G: Generate<T>>(inner: G) -> OptionalGenerator<G, T> {
    OptionalGenerator {
        inner,
        _phantom: PhantomData,
    }
}

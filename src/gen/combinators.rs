use super::{
    discardable_group, generate_from_schema, group, integers, labels, BasicGenerator, Generate,
};
use crate::cbor_helpers::{cbor_array, cbor_map, cbor_serialize};
use ciborium::Value;
use std::any::Any;
use std::marker::PhantomData;
use std::sync::{Arc, OnceLock};

pub struct Mapped<T, U, F, G> {
    pub(crate) source: G,
    pub(crate) f: F,
    pub(crate) _phantom: PhantomData<(T, U)>,
    pub(crate) cached_basic: OnceLock<Option<BasicGenerator<U>>>,
}

impl<T, U, F, G> Generate<U> for Mapped<T, U, F, G>
where
    T: serde::de::DeserializeOwned + 'static,
    U: 'static,
    G: Generate<T>,
    F: Fn(T) -> U + Send + Sync + Clone + 'static,
{
    fn generate(&self) -> U {
        if let Some(basic) = self.as_basic() {
            // Always has a transform (created via BasicGenerator::map), so
            // generate_transformed() works even when U isn't DeserializeOwned.
            basic.generate_transformed()
        } else {
            group(labels::MAPPED, || (self.f)(self.source.generate()))
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<U>> {
        self.cached_basic
            .get_or_init(|| {
                let source_basic = self.source.as_basic()?;
                Some(source_basic.map(self.f.clone()))
            })
            .clone()
    }
}

// Safety: Mapped is Send+Sync if its components are
unsafe impl<T, U, F, G> Send for Mapped<T, U, F, G>
where
    G: Send,
    F: Send,
{
}

unsafe impl<T, U, F, G> Sync for Mapped<T, U, F, G>
where
    G: Sync,
    F: Sync,
{
}

pub struct FlatMapped<T, U, G2, F, G1> {
    pub(crate) source: G1,
    pub(crate) f: F,
    pub(crate) _phantom: PhantomData<(T, U, G2)>,
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

unsafe impl<T, U, G2, F, G1> Send for FlatMapped<T, U, G2, F, G1>
where
    G1: Send,
    F: Send,
{
}

unsafe impl<T, U, G2, F, G1> Sync for FlatMapped<T, U, G2, F, G1>
where
    G1: Sync,
    F: Sync,
{
}

pub struct Filtered<T, F, G> {
    pub(crate) source: G,
    pub(crate) predicate: F,
    pub(crate) _phantom: PhantomData<T>,
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

unsafe impl<T, F, G> Send for Filtered<T, F, G>
where
    G: Send,
    F: Send,
{
}

unsafe impl<T, F, G> Sync for Filtered<T, F, G>
where
    G: Sync,
    F: Sync,
{
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

    fn as_basic(&self) -> Option<BasicGenerator<T>> {
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
    cached_basic: OnceLock<Option<BasicGenerator<T>>>,
}

impl<T: Clone + Send + Sync + serde::Serialize + 'static> Generate<T> for SampledFromGenerator<T> {
    fn generate(&self) -> T {
        crate::assume(!self.elements.is_empty());

        // Check if elements are primitive enough for sampled_from schema
        if let Some(basic) = self.as_basic() {
            return basic.generate_transformed();
        }

        // Generate index and pick
        let idx_gen = integers::<usize>()
            .with_min(0)
            .with_max(self.elements.len() - 1);
        let idx = idx_gen.generate();
        self.elements[idx].clone()
    }

    fn as_basic(&self) -> Option<BasicGenerator<T>> {
        self.cached_basic
            .get_or_init(|| {
                // Only use sampled_from schema for CBOR-primitive types
                let cbor_values: Vec<Value> =
                    self.elements.iter().map(|e| cbor_serialize(e)).collect();

                // Check if all values are primitives (not maps/arrays)
                let all_primitive = cbor_values.iter().all(|v| {
                    matches!(
                        v,
                        Value::Null
                            | Value::Bool(_)
                            | Value::Integer(_)
                            | Value::Float(_)
                            | Value::Text(_)
                    )
                });

                if all_primitive {
                    // Use index-based approach: generate an index, then return the element
                    let elements = self.elements.clone();
                    let schema = cbor_map! {
                        "type" => "integer",
                        "minimum" => 0u64,
                        "maximum" => (elements.len() - 1) as u64
                    };
                    Some(BasicGenerator::with_transform(schema, move |raw| {
                        let hv = super::value::HegelValue::from(raw.clone());
                        let idx: usize = super::value::from_hegel_value(hv).unwrap_or_else(|e| {
                            panic!(
                                "hegel: failed to deserialize index: {}\nValue: {:?}",
                                e, raw
                            );
                        });
                        elements[idx].clone()
                    }))
                } else {
                    None
                }
            })
            .clone()
    }
}

pub fn sampled_from<T: Clone + Send + Sync + serde::Serialize + 'static>(
    elements: Vec<T>,
) -> SampledFromGenerator<T> {
    SampledFromGenerator {
        elements,
        cached_basic: OnceLock::new(),
    }
}

pub struct SampledFromSliceGenerator<'a, T> {
    elements: &'a [T],
    cached_basic: OnceLock<Option<BasicGenerator<T>>>,
}

impl<'a, T: Clone + Send + Sync + serde::Serialize + serde::de::DeserializeOwned + 'static>
    Generate<T> for SampledFromSliceGenerator<'a, T>
{
    fn generate(&self) -> T {
        crate::assume(!self.elements.is_empty());

        if let Some(basic) = self.as_basic() {
            basic.generate()
        } else {
            // Compositional fallback
            group(labels::SAMPLED_FROM, || {
                let idx = integers::<usize>()
                    .with_min(0)
                    .with_max(self.elements.len() - 1)
                    .generate();
                self.elements[idx].clone()
            })
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<T>> {
        self.cached_basic
            .get_or_init(|| {
                let cbor_values: Vec<Value> =
                    self.elements.iter().map(|e| cbor_serialize(e)).collect();
                Some(BasicGenerator::new(
                    cbor_map! {"sampled_from" => Value::Array(cbor_values)},
                ))
            })
            .clone()
    }
}

// Safety: SampledFromSliceGenerator is Send+Sync if T is Send+Sync
unsafe impl<'a, T: Send + Sync> Send for SampledFromSliceGenerator<'a, T> {}
unsafe impl<'a, T: Send + Sync> Sync for SampledFromSliceGenerator<'a, T> {}

/// Sample uniformly from a borrowed slice.
///
/// This allows creating generators that borrow from local data,
/// enabling non-`'static` lifetimes.
///
/// # Example
///
/// ```no_run
/// use hegel::gen::{self, Generate, BoxedGenerator};
///
/// let choices = vec!["apple".to_string(), "banana".to_string(), "cherry".to_string()];
/// let gen: BoxedGenerator<'_, String> = gen::sampled_from_slice(&choices).boxed();
/// let value = gen.generate();
/// ```
pub fn sampled_from_slice<
    T: Clone + Send + Sync + serde::Serialize + serde::de::DeserializeOwned + 'static,
>(
    elements: &[T],
) -> SampledFromSliceGenerator<'_, T> {
    SampledFromSliceGenerator {
        elements,
        cached_basic: OnceLock::new(),
    }
}

pub struct OneOfGenerator<'a, T> {
    generators: Vec<BoxedGenerator<'a, T>>,
    cached_basic: OnceLock<Option<BasicGenerator<T>>>,
}

impl<'a, T: serde::de::DeserializeOwned + 'static> Generate<T> for OneOfGenerator<'a, T> {
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

    fn as_basic(&self) -> Option<BasicGenerator<T>> {
        self.cached_basic
            .get_or_init(|| {
                // Collect basic generators from all branches
                let basics: Option<Vec<BasicGenerator<T>>> =
                    self.generators.iter().map(|g| g.as_basic()).collect();
                let basics = basics?;

                // Check if all have identity transforms (simple case)
                let all_identity = basics.iter().all(|b| b.transform.is_none());

                if all_identity {
                    let schemas: Vec<Value> = basics.iter().map(|b| b.schema.clone()).collect();
                    Some(BasicGenerator::new(
                        cbor_map! {"one_of" => Value::Array(schemas)},
                    ))
                } else {
                    // Use tagged tuples: each branch becomes [const_tag, value]
                    let tagged_schemas: Vec<Value> = basics
                        .iter()
                        .enumerate()
                        .map(|(i, b)| {
                            cbor_map! {
                                "type" => "tuple",
                                "elements" => cbor_array![
                                    cbor_map!{"const" => Value::Integer(ciborium::value::Integer::from(i as i64))},
                                    b.schema.clone()
                                ]
                            }
                        })
                        .collect();

                    let schema = cbor_map! {"one_of" => Value::Array(tagged_schemas)};

                    type Transform<T> = Option<Arc<dyn Fn(Value) -> T + Send + Sync>>;
                    let transforms: Vec<Transform<T>> =
                        basics.into_iter().map(|b| b.transform).collect();

                    Some(BasicGenerator::with_transform(schema, move |raw| {
                        // raw is a tagged tuple [tag, value]
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

                        if let Some(ref transform) = transforms[tag] {
                            transform(value)
                        } else {
                            let hegel_value = super::value::HegelValue::from(value.clone());
                            super::value::from_hegel_value(hegel_value).unwrap_or_else(|e| {
                                panic!(
                                    "hegel: failed to deserialize server response: {}\nValue: {:?}",
                                    e, value
                                );
                            })
                        }
                    }))
                }
            })
            .clone()
    }
}

/// Choose from multiple generators of the same type.
///
/// For a more convenient syntax, use the `one_of!` macro instead.
pub fn one_of<'a, T>(generators: Vec<BoxedGenerator<'a, T>>) -> OneOfGenerator<'a, T> {
    OneOfGenerator {
        generators,
        cached_basic: OnceLock::new(),
    }
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

pub struct OptionalGenerator<G> {
    inner: G,
    cached_basic: OnceLock<Box<dyn Any + Send + Sync>>,
}

impl<T, G> Generate<Option<T>> for OptionalGenerator<G>
where
    G: Generate<T>,
    T: serde::de::DeserializeOwned + 'static,
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

    fn as_basic(&self) -> Option<BasicGenerator<Option<T>>> {
        self.cached_basic
            .get_or_init(|| {
                let result: Option<BasicGenerator<Option<T>>> = (|| {
                    let inner_basic = self.inner.as_basic()?;

                    if inner_basic.transform.is_none() {
                        // Simple case: no transform, compose schemas directly
                        let schema = cbor_map! {
                            "one_of" => cbor_array![
                                cbor_map!{"type" => "null"},
                                inner_basic.schema
                            ]
                        };
                        Some(BasicGenerator::new(schema))
                    } else {
                        // Inner has transform, use tagged tuples approach
                        let null_schema = cbor_map! {"type" => "tuple", "elements" => cbor_array![cbor_map!{"const" => Value::Integer(0.into())}, cbor_map!{"type" => "null"}]};
                        let value_schema = cbor_map! {"type" => "tuple", "elements" => cbor_array![cbor_map!{"const" => Value::Integer(1.into())}, inner_basic.schema]};
                        let schema = cbor_map! {"one_of" => cbor_array![null_schema, value_schema]};

                        let transform = inner_basic.transform;
                        Some(BasicGenerator::with_transform(schema, move |raw| {
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
                                if let Some(ref t) = transform {
                                    Some(t(value))
                                } else {
                                    let hegel_value =
                                        super::value::HegelValue::from(value.clone());
                                    Some(
                                        super::value::from_hegel_value(hegel_value)
                                            .unwrap_or_else(|e| {
                                                panic!(
                                                "hegel: failed to deserialize server response: {}\nValue: {:?}",
                                                e, value
                                            );
                                            }),
                                    )
                                }
                            }
                        }))
                    }
                })();
                Box::new(result) as Box<dyn Any + Send + Sync>
            })
            .downcast_ref::<Option<BasicGenerator<Option<T>>>>()
            .expect("cached_basic type mismatch")
            .clone()
    }
}

pub fn optional<T, G: Generate<T>>(inner: G) -> OptionalGenerator<G> {
    OptionalGenerator {
        inner,
        cached_basic: OnceLock::new(),
    }
}

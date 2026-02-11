use super::{group, labels, BasicGenerator, BoxedGenerator, Generate};
use crate::cbor_helpers::cbor_map;
use ciborium::Value;
use std::marker::PhantomData;
use std::sync::{Arc, OnceLock};

pub(crate) struct MappedToValue<T, G> {
    inner: G,
    _phantom: PhantomData<T>,
    cached_basic: OnceLock<Option<BasicGenerator<Value>>>,
}

impl<T: serde::Serialize + serde::de::DeserializeOwned + 'static, G: Generate<T>> Generate<Value>
    for MappedToValue<T, G>
{
    fn generate(&self) -> Value {
        crate::cbor_helpers::cbor_serialize(&self.inner.generate())
    }

    fn as_basic(&self) -> Option<BasicGenerator<Value>> {
        self.cached_basic
            .get_or_init(|| {
                let inner_basic = self.inner.as_basic()?;
                // Map the inner basic generator to produce Value instead of T
                if let Some(transform) = inner_basic.transform {
                    // Inner has a transform T -> T; we need Value -> Value
                    // We can't easily compose here since the output type differs,
                    // so just return the schema with a transform that applies the
                    // inner transform and serializes to Value
                    Some(BasicGenerator::with_transform(
                        inner_basic.schema,
                        move |raw| {
                            let t_val = transform(raw);
                            crate::cbor_helpers::cbor_serialize(&t_val)
                        },
                    ))
                } else {
                    // Identity transform on inner - schema produces T directly,
                    // which is Value-compatible since it comes from the server as Value
                    // Just pass through the raw Value
                    Some(BasicGenerator::with_transform(inner_basic.schema, |raw| {
                        raw
                    }))
                }
            })
            .clone()
    }
}

unsafe impl<T, G: Send> Send for MappedToValue<T, G> {}
unsafe impl<T, G: Sync> Sync for MappedToValue<T, G> {}

pub struct FixedDictBuilder<'a> {
    fields: Vec<(String, BoxedGenerator<'a, Value>)>,
}

impl<'a> FixedDictBuilder<'a> {
    pub fn field<T, G>(mut self, name: &str, gen: G) -> Self
    where
        G: Generate<T> + Send + Sync + 'a,
        T: serde::Serialize + serde::de::DeserializeOwned + 'static,
    {
        let boxed = BoxedGenerator {
            inner: Arc::new(MappedToValue {
                inner: gen,
                _phantom: PhantomData::<T>,
                cached_basic: OnceLock::new(),
            }),
        };
        self.fields.push((name.to_string(), boxed));
        self
    }

    pub fn build(self) -> FixedDictGenerator<'a> {
        FixedDictGenerator {
            fields: self.fields,
            cached_basic: OnceLock::new(),
        }
    }
}

pub struct FixedDictGenerator<'a> {
    fields: Vec<(String, BoxedGenerator<'a, Value>)>,
    cached_basic: OnceLock<Option<BasicGenerator<Value>>>,
}

impl<'a> Generate<Value> for FixedDictGenerator<'a> {
    fn generate(&self) -> Value {
        if let Some(basic) = self.as_basic() {
            basic.generate()
        } else {
            // Compositional fallback
            group(labels::FIXED_DICT, || {
                let entries: Vec<(Value, Value)> = self
                    .fields
                    .iter()
                    .map(|(name, gen)| (Value::Text(name.clone()), gen.generate()))
                    .collect();
                Value::Map(entries)
            })
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<Value>> {
        self.cached_basic
            .get_or_init(|| {
                // Collect basic generators for all fields
                let basics: Vec<BasicGenerator<Value>> = self
                    .fields
                    .iter()
                    .map(|(_, gen)| gen.as_basic())
                    .collect::<Option<Vec<_>>>()?;

                let has_transforms = basics.iter().any(|b| b.transform.is_some());

                let schemas: Vec<Value> = basics.iter().map(|b| b.schema.clone()).collect();
                let schema = cbor_map! {
                    "type" => "tuple",
                    "elements" => Value::Array(schemas)
                };

                if has_transforms {
                    type Transform = Option<Arc<dyn Fn(Value) -> Value + Send + Sync>>;
                    let transforms: Vec<Transform> =
                        basics.into_iter().map(|b| b.transform).collect();
                    let field_names: Vec<String> =
                        self.fields.iter().map(|(name, _)| name.clone()).collect();

                    Some(BasicGenerator::with_transform(schema, move |raw| {
                        let arr = match raw {
                            Value::Array(arr) => arr,
                            _ => panic!("Expected array from tuple schema, got {:?}", raw),
                        };

                        let entries: Vec<(Value, Value)> = field_names
                            .iter()
                            .zip(arr)
                            .zip(transforms.iter())
                            .map(|((name, val), transform)| {
                                let result = if let Some(ref t) = transform {
                                    t(val)
                                } else {
                                    val
                                };
                                (Value::Text(name.clone()), result)
                            })
                            .collect();

                        Value::Map(entries)
                    }))
                } else {
                    // All identity transforms - but we still need to convert the
                    // tuple array back to a map, so use a transform
                    let field_names: Vec<String> =
                        self.fields.iter().map(|(name, _)| name.clone()).collect();

                    Some(BasicGenerator::with_transform(schema, move |raw| {
                        let arr = match raw {
                            Value::Array(arr) => arr,
                            _ => panic!("Expected array from tuple schema, got {:?}", raw),
                        };
                        let entries: Vec<(Value, Value)> = field_names
                            .iter()
                            .zip(arr)
                            .map(|(name, val)| (Value::Text(name.clone()), val))
                            .collect();
                        Value::Map(entries)
                    }))
                }
            })
            .clone()
    }
}

/// Create a generator for dictionaries with fixed keys.
///
/// # Example
///
/// ```no_run
/// use hegel::gen::{self, Generate};
///
/// let gen = gen::fixed_dicts()
///     .field("name", gen::text())
///     .field("age", gen::integers::<u32>())
///     .build();
/// ```
pub fn fixed_dicts<'a>() -> FixedDictBuilder<'a> {
    FixedDictBuilder { fields: Vec::new() }
}

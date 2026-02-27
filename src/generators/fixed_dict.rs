use super::{labels, BasicGenerator, BoxedGenerator, Generate, TestCaseData};
use crate::cbor_helpers::cbor_map;
use ciborium::Value;
use std::marker::PhantomData;
use std::sync::Arc;

pub(crate) struct MappedToValue<T, G> {
    inner: G,
    _phantom: PhantomData<fn() -> T>,
}

impl<T: serde::Serialize, G: Generate<T>> Generate<Value> for MappedToValue<T, G> {
    fn do_draw(&self, data: &TestCaseData) -> Value {
        crate::cbor_helpers::cbor_serialize(&self.inner.do_draw(data))
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Value>> {
        let inner_basic = self.inner.as_basic()?;
        let schema = inner_basic.schema().clone();
        Some(BasicGenerator::new(schema, move |raw| {
            let t_val = inner_basic.parse_raw(raw);
            crate::cbor_helpers::cbor_serialize(&t_val)
        }))
    }
}

pub struct FixedDictBuilder<'a> {
    fields: Vec<(String, BoxedGenerator<'a, Value>)>,
}

impl<'a> FixedDictBuilder<'a> {
    pub fn field<T, G>(mut self, name: &str, gen: G) -> Self
    where
        G: Generate<T> + Send + Sync + 'a,
        T: serde::Serialize + 'a,
    {
        let boxed = BoxedGenerator {
            inner: Arc::new(MappedToValue {
                inner: gen,
                _phantom: PhantomData,
            }),
        };
        self.fields.push((name.to_string(), boxed));
        self
    }

    pub fn build(self) -> FixedDictGenerator<'a> {
        FixedDictGenerator {
            fields: self.fields,
        }
    }
}

pub struct FixedDictGenerator<'a> {
    fields: Vec<(String, BoxedGenerator<'a, Value>)>,
}

impl Generate<Value> for FixedDictGenerator<'_> {
    fn do_draw(&self, data: &TestCaseData) -> Value {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            // Compositional fallback
            data.span_group(labels::FIXED_DICT, || {
                let entries: Vec<(Value, Value)> = self
                    .fields
                    .iter()
                    .map(|(name, gen)| (Value::Text(name.clone()), gen.do_draw(data)))
                    .collect();
                Value::Map(entries)
            })
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Value>> {
        let basics: Vec<BasicGenerator<'_, Value>> = self
            .fields
            .iter()
            .map(|(_, gen)| gen.as_basic())
            .collect::<Option<Vec<_>>>()?;

        let schemas: Vec<Value> = basics.iter().map(|b| b.schema().clone()).collect();

        let schema = cbor_map! {
            "type" => "tuple",
            "elements" => Value::Array(schemas)
        };

        let field_names: Vec<String> = self.fields.iter().map(|(name, _)| name.clone()).collect();

        Some(BasicGenerator::new(schema, move |raw| {
            let arr = match raw {
                Value::Array(arr) => arr,
                _ => panic!("Expected array from tuple schema, got {:?}", raw),
            };

            let entries: Vec<(Value, Value)> = field_names
                .iter()
                .zip(basics.iter())
                .zip(arr)
                .map(|((name, basic), val)| (Value::Text(name.clone()), basic.parse_raw(val)))
                .collect();
            Value::Map(entries)
        }))
    }
}

/// Create a generator for dictionaries with fixed keys.
///
/// # Example
///
/// ```no_run
/// use hegel::generators::{self, Generate};
///
/// let gen = generators::fixed_dicts()
///     .field("name", generators::text())
///     .field("age", generators::integers::<u32>())
///     .build();
/// ```
pub fn fixed_dicts<'a>() -> FixedDictBuilder<'a> {
    FixedDictBuilder { fields: Vec::new() }
}

use super::{generate_from_schema, group, labels, BoxedGenerator, Generate};
use serde_json::{json, Value};
use std::marker::PhantomData;
use std::sync::Arc;

pub(crate) struct MappedToValue<T, G> {
    inner: G,
    _phantom: PhantomData<T>,
}

impl<T: serde::Serialize, G: Generate<T>> Generate<Value> for MappedToValue<T, G> {
    fn generate(&self) -> Value {
        json!(self.inner.generate())
    }

    fn schema(&self) -> Option<Value> {
        self.inner.schema()
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
        T: serde::Serialize + 'a,
    {
        let boxed = BoxedGenerator {
            inner: Arc::new(MappedToValue {
                inner: gen,
                _phantom: PhantomData::<T>,
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

impl<'a> Generate<Value> for FixedDictGenerator<'a> {
    fn generate(&self) -> Value {
        if let Some(schema) = self.schema() {
            generate_from_schema(&schema)
        } else {
            // Compositional fallback
            group(labels::FIXED_DICT, || {
                let mut map = serde_json::Map::new();
                for (name, gen) in &self.fields {
                    map.insert(name.clone(), gen.generate());
                }
                Value::Object(map)
            })
        }
    }

    fn schema(&self) -> Option<Value> {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for (name, gen) in &self.fields {
            let field_schema = gen.schema()?;
            properties.insert(name.clone(), field_schema);
            required.push(json!(name));
        }

        Some(json!({
            "type": "object",
            "properties": properties,
            "required": required
        }))
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

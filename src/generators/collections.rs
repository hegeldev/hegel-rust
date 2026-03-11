use super::{integers, labels, BasicGenerator, BoxedGenerator, Collection, Generator, TestCaseData};
use crate::cbor_utils::{cbor_map, map_insert};
use ciborium::Value;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct VecGenerator<G, T> {
    pub(crate) elements: G,
    pub(crate) min_size: usize,
    pub(crate) max_size: Option<usize>,
    pub(crate) unique: bool,
    pub(crate) _phantom: PhantomData<fn(T)>,
}

impl<G, T> VecGenerator<G, T> {
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
    }

    pub fn max_size(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }

    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }
}

impl<T, G> Generator<Vec<T>> for VecGenerator<G, T>
where
    G: Generator<T>,
{
    fn do_draw(&self, data: &TestCaseData) -> Vec<T> {
        if let Some(max) = self.max_size {
            assert!(self.min_size <= max, "Cannot have max_size < min_size");
        }
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            data.start_span(labels::LIST);
            let mut collection =
                Collection::new(data, "composite_list", self.min_size, self.max_size);
            let mut result = Vec::new();
            while collection.more() {
                result.push(self.elements.do_draw(data));
            }
            data.stop_span(false);
            result
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Vec<T>>> {
        if let Some(max) = self.max_size {
            assert!(self.min_size <= max, "Cannot have max_size < min_size");
        }
        let basic = self.elements.as_basic()?;

        let mut schema = cbor_map! {
            "type" => "list",
            "unique" => self.unique,
            "elements" => basic.schema().clone(),
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", max as u64);
        }

        Some(BasicGenerator::new(schema, move |raw| {
            let Value::Array(arr) = raw else {
                panic!("Expected array, got {:?}", raw)
            };
            arr.into_iter().map(|v| basic.parse_raw(v)).collect()
        }))
    }
}

pub fn vecs<T, G: Generator<T>>(elements: G) -> VecGenerator<G, T> {
    VecGenerator {
        elements,
        min_size: 0,
        max_size: None,
        unique: false,
        _phantom: PhantomData,
    }
}

pub struct HashSetGenerator<G, T> {
    elements: G,
    min_size: usize,
    max_size: Option<usize>,
    _phantom: PhantomData<fn(T)>,
}

impl<G, T> HashSetGenerator<G, T> {
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
    }

    pub fn max_size(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }
}

impl<T, G> Generator<HashSet<T>> for HashSetGenerator<G, T>
where
    G: Generator<T>,
    T: Eq + Hash,
{
    fn do_draw(&self, data: &TestCaseData) -> HashSet<T> {
        if let Some(max) = self.max_size {
            assert!(self.min_size <= max, "Cannot have max_size < min_size");
        }
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            data.start_span(labels::SET);
            let max = self.max_size.unwrap_or(100);
            let target_len = integers::<usize>()
                .min_value(self.min_size)
                .max_value(max)
                .do_draw(data);

            let mut set = HashSet::new();
            let mut attempts = 0;
            while set.len() < target_len && attempts < target_len * 10 {
                data.start_span(labels::SET_ELEMENT);
                set.insert(self.elements.do_draw(data));
                data.stop_span(false);
                attempts += 1;
            }
            data.stop_span(false);
            set
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, HashSet<T>>> {
        if let Some(max) = self.max_size {
            assert!(self.min_size <= max, "Cannot have max_size < min_size");
        }
        let basic = self.elements.as_basic()?;

        let mut schema = cbor_map! {
            "type" => "list",
            "unique" => true,
            "elements" =>  basic.schema().clone(),
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", max as u64);
        }

        Some(BasicGenerator::new(schema, move |raw| {
            let Value::Array(arr) = raw else {
                panic!("Expected array, got {:?}", raw)
            };
            arr.into_iter().map(|v| basic.parse_raw(v)).collect()
        }))
    }
}

pub fn hashsets<T, G: Generator<T>>(elements: G) -> HashSetGenerator<G, T> {
    HashSetGenerator {
        elements,
        min_size: 0,
        max_size: None,
        _phantom: PhantomData,
    }
}

pub struct HashMapGenerator<K, V, KT, VT> {
    keys: K,
    values: V,
    min_size: usize,
    max_size: Option<usize>,
    _phantom: PhantomData<fn(KT, VT)>,
}

impl<K, V, KT, VT> HashMapGenerator<K, V, KT, VT> {
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
    }

    pub fn max_size(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }
}

impl<K, V, KT, VT> Generator<HashMap<KT, VT>> for HashMapGenerator<K, V, KT, VT>
where
    K: Generator<KT>,
    V: Generator<VT>,
    KT: Eq + std::hash::Hash,
{
    fn do_draw(&self, data: &TestCaseData) -> HashMap<KT, VT> {
        if let Some(max) = self.max_size {
            assert!(self.min_size <= max, "Cannot have max_size < min_size");
        }
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            data.start_span(labels::MAP);
            let max = self.max_size.unwrap_or(100);
            let len = integers::<usize>()
                .min_value(self.min_size)
                .max_value(max)
                .do_draw(data);

            let mut map = HashMap::new();
            let max_attempts = len * 10;
            let mut attempts = 0;
            while map.len() < len && attempts < max_attempts {
                data.start_span(labels::MAP_ENTRY);
                let key = self.keys.do_draw(data);
                map.entry(key).or_insert_with(|| self.values.do_draw(data));
                data.stop_span(false);
                attempts += 1;
            }
            crate::assume(map.len() >= self.min_size);
            data.stop_span(false);
            map
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, HashMap<KT, VT>>> {
        if let Some(max) = self.max_size {
            assert!(self.min_size <= max, "Cannot have max_size < min_size");
        }
        let keys_basic = self.keys.as_basic()?;
        let values_basic = self.values.as_basic()?;

        let mut schema = cbor_map! {
            "type" => "dict",
            "keys" => keys_basic.schema().clone(),
            "values" => values_basic.schema().clone(),
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", max as u64);
        }

        Some(BasicGenerator::new(schema, move |raw| {
            // schema expects format [[key, value], ...]
            let values = match raw {
                Value::Array(arr) => arr,
                _ => panic!("Expected array, got {:?}", raw),
            };

            let mut map = HashMap::new();
            for value_raw in values {
                let value = match value_raw {
                    Value::Array(arr) => arr,
                    _ => panic!("Expected array, got {:?}", value_raw),
                };
                let mut iter = value.into_iter();
                let raw_k = iter.next().unwrap();
                let raw_v = iter.next().unwrap();

                let key = keys_basic.parse_raw(raw_k);
                let value = values_basic.parse_raw(raw_v);

                map.insert(key, value);
            }
            map
        }))
    }
}

/// Generate hash maps.
///
/// # Example
///
/// ```ignore
/// use hegel::generators::{hashmaps, integers, text};
/// use std::collections::HashMap;
///
/// let map: HashMap<i32, String> = hegel::draw(&hashmaps(integers(), text()));
/// ```
pub fn hashmaps<KT, VT, K: Generator<KT>, V: Generator<VT>>(
    keys: K,
    values: V,
) -> HashMapGenerator<K, V, KT, VT> {
    HashMapGenerator {
        keys,
        values,
        min_size: 0,
        max_size: None,
        _phantom: PhantomData,
    }
}

pub(crate) struct MappedToValue<T, G> {
    inner: G,
    _phantom: PhantomData<fn() -> T>,
}

impl<T: serde::Serialize, G: Generator<T>> Generator<Value> for MappedToValue<T, G> {
    fn do_draw(&self, data: &TestCaseData) -> Value {
        crate::cbor_utils::cbor_serialize(&self.inner.do_draw(data))
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Value>> {
        let inner_basic = self.inner.as_basic()?;
        let schema = inner_basic.schema().clone();
        Some(BasicGenerator::new(schema, move |raw| {
            let t_val = inner_basic.parse_raw(raw);
            crate::cbor_utils::cbor_serialize(&t_val)
        }))
    }
}

pub struct FixedDictBuilder {
    fields: Vec<(String, BoxedGenerator<Value>)>,
}

impl FixedDictBuilder {
    pub fn field<T, G>(mut self, name: &str, gen: G) -> Self
    where
        G: Generator<T> + Send + Sync + 'static,
        T: serde::Serialize + 'static,
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

    pub fn build(self) -> FixedDictGenerator {
        FixedDictGenerator {
            fields: self.fields,
        }
    }
}

pub struct FixedDictGenerator {
    fields: Vec<(String, BoxedGenerator<Value>)>,
}

impl Generator<Value> for FixedDictGenerator {
    fn do_draw(&self, data: &TestCaseData) -> Value {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            data.start_span(labels::FIXED_DICT);
            let entries: Vec<(Value, Value)> = self
                .fields
                .iter()
                .map(|(name, gen)| (Value::Text(name.clone()), gen.do_draw(data)))
                .collect();
            data.stop_span(false);
            Value::Map(entries)
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
/// use hegel::generators::{self, Generator};
///
/// let gen = generators::fixed_dicts()
///     .field("name", generators::text())
///     .field("age", generators::integers::<u32>())
///     .build();
/// ```
pub fn fixed_dicts() -> FixedDictBuilder {
    FixedDictBuilder { fields: Vec::new() }
}

pub struct ArrayGenerator<G, T, const N: usize> {
    element: G,
    _phantom: PhantomData<fn() -> T>,
}

impl<G, T, const N: usize> ArrayGenerator<G, T, N> {
    pub fn new(element: G) -> Self {
        ArrayGenerator {
            element,
            _phantom: PhantomData,
        }
    }
}

pub fn arrays<G: Generator<T> + Send + Sync, T, const N: usize>(
    element: G,
) -> ArrayGenerator<G, T, N> {
    ArrayGenerator::new(element)
}

impl<G: Generator<T> + Send + Sync, T, const N: usize> Generator<[T; N]> for ArrayGenerator<G, T, N> {
    fn do_draw(&self, data: &TestCaseData) -> [T; N] {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            data.start_span(labels::TUPLE);
            let result = std::array::from_fn(|_| self.element.do_draw(data));
            data.stop_span(false);
            result
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, [T; N]>> {
        let basic = self.element.as_basic()?;

        let elements = Value::Array((0..N).map(|_| basic.schema().clone()).collect());
        let schema = cbor_map! {
            "type" => "tuple",
            "elements" => elements
        };

        Some(BasicGenerator::new(schema, move |raw| {
            let arr = match raw {
                Value::Array(arr) => arr,
                _ => panic!("Expected array from tuple schema, got {:?}", raw),
            };
            assert_eq!(arr.len(), N);
            let mut iter = arr.into_iter();
            std::array::from_fn(|_| basic.parse_raw(iter.next().unwrap()))
        }))
    }
}

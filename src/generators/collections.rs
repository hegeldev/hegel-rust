use super::{integers, labels, BasicGenerator, Collection, Generate, TestCaseData};
use crate::cbor_helpers::{cbor_map, map_insert};
use ciborium::Value;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::marker::PhantomData;

/// Extract an array from a Value, handling both plain Arrays and CBOR Tag(258, Array)
/// which is the standard CBOR tag for sets.
fn extract_array(raw: Value) -> Vec<Value> {
    match raw {
        Value::Array(arr) => arr,
        Value::Tag(258, inner) => match *inner {
            Value::Array(arr) => arr,
            other => panic!("Expected array inside set tag, got {:?}", other),
        },
        other => panic!("Expected array or tagged set, got {:?}", other),
    }
}

pub struct VecGenerator<G, T> {
    pub(crate) elements: G,
    pub(crate) min_size: usize,
    pub(crate) max_size: Option<usize>,
    pub(crate) unique: bool,
    pub(crate) _phantom: PhantomData<fn(T)>,
}

impl<G, T> VecGenerator<G, T> {
    pub fn with_min_size(mut self, min: usize) -> Self {
        self.min_size = min;
        self
    }

    pub fn with_max_size(mut self, max: usize) -> Self {
        self.max_size = Some(max);
        self
    }

    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }
}

impl<T, G> Generate<Vec<T>> for VecGenerator<G, T>
where
    G: Generate<T>,
{
    fn do_draw(&self, data: &TestCaseData) -> Vec<T> {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            // Compositional fallback: use server-managed collection sizing
            data.span_group(labels::LIST, || {
                let mut collection =
                    Collection::new("composite_list", self.min_size, self.max_size);
                let mut result = Vec::new();
                while collection.more(data) {
                    result.push(self.elements.do_draw(data));
                }
                result
            })
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Vec<T>>> {
        let elem_basic = self.elements.as_basic()?;
        let elem_schema = elem_basic.schema().clone();

        let schema_type = if self.unique { "set" } else { "list" };

        let mut schema = cbor_map! {
            "type" => schema_type,
            "elements" => elem_schema,
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", Value::from(max as u64));
        }

        Some(BasicGenerator::new(schema, move |raw| {
            let arr = extract_array(raw);
            arr.into_iter().map(|v| elem_basic.parse_raw(v)).collect()
        }))
    }
}

/// Generate vectors (lists).
pub fn vecs<T, G: Generate<T>>(elements: G) -> VecGenerator<G, T> {
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
    pub fn with_min_size(mut self, min: usize) -> Self {
        self.min_size = min;
        self
    }

    pub fn with_max_size(mut self, max: usize) -> Self {
        self.max_size = Some(max);
        self
    }
}

impl<T, G> Generate<HashSet<T>> for HashSetGenerator<G, T>
where
    G: Generate<T>,
    T: Eq + Hash,
{
    fn do_draw(&self, data: &TestCaseData) -> HashSet<T> {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            // Compositional fallback
            data.span_group(labels::SET, || {
                let max = self.max_size.unwrap_or(100);
                let target_len = integers::<usize>()
                    .with_min(self.min_size)
                    .with_max(max)
                    .do_draw(data);

                let mut set = HashSet::new();
                let mut attempts = 0;
                while set.len() < target_len && attempts < target_len * 10 {
                    set.insert(
                        data.span_group(labels::SET_ELEMENT, || self.elements.do_draw(data)),
                    );
                    attempts += 1;
                }
                set
            })
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, HashSet<T>>> {
        let elem_basic = self.elements.as_basic()?;
        let elem_schema = elem_basic.schema().clone();

        let mut schema = cbor_map! {
            "type" => "set",
            "elements" => elem_schema,
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", Value::from(max as u64));
        }

        Some(BasicGenerator::new(schema, move |raw| {
            let arr = extract_array(raw);
            arr.into_iter().map(|v| elem_basic.parse_raw(v)).collect()
        }))
    }
}

pub fn hashsets<T, G: Generate<T>>(elements: G) -> HashSetGenerator<G, T> {
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
    pub fn with_min_size(mut self, min: usize) -> Self {
        self.min_size = min;
        self
    }

    pub fn with_max_size(mut self, max: usize) -> Self {
        self.max_size = Some(max);
        self
    }
}

impl<K, V, KT, VT> Generate<HashMap<KT, VT>> for HashMapGenerator<K, V, KT, VT>
where
    K: Generate<KT>,
    V: Generate<VT>,
    KT: Eq + std::hash::Hash,
{
    fn do_draw(&self, data: &TestCaseData) -> HashMap<KT, VT> {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            // Compositional fallback
            data.span_group(labels::MAP, || {
                let max = self.max_size.unwrap_or(100);
                let len = integers::<usize>()
                    .with_min(self.min_size)
                    .with_max(max)
                    .do_draw(data);

                let mut map = HashMap::new();
                let max_attempts = len * 10;
                let mut attempts = 0;
                while map.len() < len && attempts < max_attempts {
                    data.span_group(labels::MAP_ENTRY, || {
                        let key = self.keys.do_draw(data);
                        map.entry(key).or_insert_with(|| self.values.do_draw(data));
                    });
                    attempts += 1;
                }
                crate::assume(map.len() >= self.min_size);
                map
            })
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, HashMap<KT, VT>>> {
        let key_basic = self.keys.as_basic()?;
        let val_basic = self.values.as_basic()?;

        let key_schema = key_basic.schema().clone();
        let val_schema = val_basic.schema().clone();

        let mut schema = cbor_map! {
            "type" => "dict",
            "keys" => key_schema,
            "values" => val_schema,
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", Value::from(max as u64));
        }

        Some(BasicGenerator::new(schema, move |raw| {
            // Wire format: [[key, value], ...]
            let pairs = match raw {
                Value::Array(arr) => arr,
                _ => panic!("Expected array of pairs from dict schema, got {:?}", raw),
            };

            let mut map = HashMap::new();
            for pair in pairs {
                let mut pair_arr = match pair {
                    Value::Array(arr) => arr,
                    _ => panic!("Expected pair array, got {:?}", pair),
                };
                let raw_value = pair_arr.pop().unwrap();
                let raw_key = pair_arr.pop().unwrap();

                let key = key_basic.parse_raw(raw_key);
                let value = val_basic.parse_raw(raw_value);

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
/// // String keys
/// let string_keyed: HashMap<String, i32> = hegel::draw(&hashmaps(text(), integers()));
///
/// // Integer keys
/// let int_keyed: HashMap<i32, String> = hegel::draw(&hashmaps(integers(), text()));
/// ```
pub fn hashmaps<KT, VT, K: Generate<KT>, V: Generate<VT>>(
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

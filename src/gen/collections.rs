use super::{group, integers, labels, BasicGenerator, Collection, Generate};
use crate::cbor_helpers::{cbor_map, map_insert};
use ciborium::Value;
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::OnceLock;

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

pub struct VecGenerator<G> {
    pub(crate) elements: G,
    pub(crate) min_size: usize,
    pub(crate) max_size: Option<usize>,
    pub(crate) unique: bool,
    pub(crate) cached_basic: OnceLock<Box<dyn Any + Send + Sync>>,
}

impl<G> VecGenerator<G> {
    pub fn with_min_size(mut self, min: usize) -> Self {
        self.min_size = min;
        self.cached_basic = OnceLock::new();
        self
    }

    pub fn with_max_size(mut self, max: usize) -> Self {
        self.max_size = Some(max);
        self.cached_basic = OnceLock::new();
        self
    }

    pub fn unique(mut self) -> Self {
        self.unique = true;
        self.cached_basic = OnceLock::new();
        self
    }
}

impl<T, G> Generate<Vec<T>> for VecGenerator<G>
where
    G: Generate<T>,
    T: serde::de::DeserializeOwned + 'static,
{
    fn generate(&self) -> Vec<T> {
        if let Some(basic) = self.as_basic() {
            basic.generate()
        } else {
            // Compositional fallback: use server-managed collection sizing
            group(labels::LIST, || {
                let mut collection =
                    Collection::new("composite_list", self.min_size, self.max_size);
                let mut result = Vec::new();
                while collection.more() {
                    result.push(self.elements.generate());
                }
                result
            })
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<Vec<T>>> {
        self.cached_basic
            .get_or_init(|| {
                let result: Option<BasicGenerator<Vec<T>>> = (|| {
                    let element_basic = self.elements.as_basic()?;

                    let schema_type = if self.unique { "set" } else { "list" };

                    let mut schema = cbor_map! {
                        "type" => schema_type,
                        "elements" => element_basic.schema.clone(),
                        "min_size" => self.min_size as u64
                    };

                    if let Some(max) = self.max_size {
                        map_insert(&mut schema, "max_size", Value::from(max as u64));
                    }

                    if let Some(transform) = element_basic.transform {
                        // Element has a transform - apply it to each element
                        Some(BasicGenerator::with_transform(schema, move |raw| {
                            let arr = extract_array(raw);
                            arr.into_iter().map(|v| transform(v)).collect()
                        }))
                    } else if self.unique {
                        // Sets come as Tag(258, Array), need to unwrap
                        Some(BasicGenerator::with_transform(schema, |raw| {
                            let arr = extract_array(raw);
                            let hegel_values: Vec<Value> = arr;
                            hegel_values
                                .into_iter()
                                .map(|v| {
                                    let hv = super::value::HegelValue::from(v.clone());
                                    super::value::from_hegel_value(hv).unwrap_or_else(|e| {
                                        panic!(
                                            "hegel: failed to deserialize set element: {}\nValue: {:?}",
                                            e, v
                                        );
                                    })
                                })
                                .collect()
                        }))
                    } else {
                        // Identity transform for list elements - use direct deserialization
                        Some(BasicGenerator::new(schema))
                    }
                })();
                Box::new(result) as Box<dyn Any + Send + Sync>
            })
            .downcast_ref::<Option<BasicGenerator<Vec<T>>>>()
            .expect("cached_basic type mismatch")
            .clone()
    }
}

/// Generate vectors (lists).
pub fn vecs<T, G: Generate<T>>(elements: G) -> VecGenerator<G> {
    VecGenerator {
        elements,
        min_size: 0,
        max_size: None,
        unique: false,
        cached_basic: OnceLock::new(),
    }
}

pub struct HashSetGenerator<G> {
    elements: G,
    min_size: usize,
    max_size: Option<usize>,
}

impl<G> HashSetGenerator<G> {
    pub fn with_min_size(mut self, min: usize) -> Self {
        self.min_size = min;
        self
    }

    pub fn with_max_size(mut self, max: usize) -> Self {
        self.max_size = Some(max);
        self
    }
}

impl<T, G> Generate<HashSet<T>> for HashSetGenerator<G>
where
    G: Generate<T>,
    T: serde::de::DeserializeOwned + Eq + Hash + 'static,
{
    fn generate(&self) -> HashSet<T> {
        // Generate as unique vec, convert to set
        let vec_gen = VecGenerator {
            elements: &self.elements,
            min_size: self.min_size,
            max_size: self.max_size,
            unique: true,
            cached_basic: OnceLock::new(),
        };

        if let Some(basic) = vec_gen.as_basic() {
            let vec: Vec<T> = basic.generate();
            vec.into_iter().collect()
        } else {
            // Compositional fallback
            group(labels::SET, || {
                let max = self.max_size.unwrap_or(100);
                let target_len = integers::<usize>()
                    .with_min(self.min_size)
                    .with_max(max)
                    .generate();

                let mut set = HashSet::new();
                let mut attempts = 0;
                while set.len() < target_len && attempts < target_len * 10 {
                    set.insert(group(labels::SET_ELEMENT, || self.elements.generate()));
                    attempts += 1;
                }
                set
            })
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<HashSet<T>>> {
        let element_basic = self.elements.as_basic()?;

        let mut schema = cbor_map! {
            "type" => "set",
            "elements" => element_basic.schema.clone(),
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", Value::from(max as u64));
        }

        if let Some(transform) = element_basic.transform {
            Some(BasicGenerator::with_transform(schema, move |raw| {
                let arr = extract_array(raw);
                arr.into_iter().map(|v| transform(v)).collect()
            }))
        } else {
            // Generate as array then convert elements to HashSet
            Some(BasicGenerator::with_transform(schema, |raw| {
                let arr = extract_array(raw);
                arr.into_iter()
                    .map(|v| {
                        let hv = super::value::HegelValue::from(v.clone());
                        super::value::from_hegel_value(hv).unwrap_or_else(|e| {
                            panic!(
                                "hegel: failed to deserialize set element: {}\nValue: {:?}",
                                e, v
                            );
                        })
                    })
                    .collect()
            }))
        }
    }
}

pub fn hashsets<T, G: Generate<T>>(elements: G) -> HashSetGenerator<G> {
    HashSetGenerator {
        elements,
        min_size: 0,
        max_size: None,
    }
}

pub struct HashMapGenerator<K, V> {
    keys: K,
    values: V,
    min_size: usize,
    max_size: Option<usize>,
    cached_basic: OnceLock<Box<dyn Any + Send + Sync>>,
}

impl<K, V> HashMapGenerator<K, V> {
    pub fn with_min_size(mut self, min: usize) -> Self {
        self.min_size = min;
        self.cached_basic = OnceLock::new();
        self
    }

    pub fn with_max_size(mut self, max: usize) -> Self {
        self.max_size = Some(max);
        self.cached_basic = OnceLock::new();
        self
    }
}

impl<K, V, KT, VT> Generate<HashMap<KT, VT>> for HashMapGenerator<K, V>
where
    K: Generate<KT>,
    V: Generate<VT>,
    KT: serde::de::DeserializeOwned + Eq + std::hash::Hash + 'static,
    VT: serde::de::DeserializeOwned + 'static,
{
    fn generate(&self) -> HashMap<KT, VT> {
        if let Some(basic) = self.as_basic() {
            basic.generate()
        } else {
            // Compositional fallback
            group(labels::MAP, || {
                let max = self.max_size.unwrap_or(100);
                let len = integers::<usize>()
                    .with_min(self.min_size)
                    .with_max(max)
                    .generate();

                let mut map = HashMap::new();
                let max_attempts = len * 10;
                let mut attempts = 0;
                while map.len() < len && attempts < max_attempts {
                    group(labels::MAP_ENTRY, || {
                        let key = self.keys.generate();
                        map.entry(key).or_insert_with(|| self.values.generate());
                    });
                    attempts += 1;
                }
                crate::assume(map.len() >= self.min_size);
                map
            })
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<HashMap<KT, VT>>> {
        self.cached_basic
            .get_or_init(|| {
                let result: Option<BasicGenerator<HashMap<KT, VT>>> = (|| {
                    let key_basic = self.keys.as_basic()?;
                    let value_basic = self.values.as_basic()?;

                    let mut schema = cbor_map! {
                        "type" => "dict",
                        "keys" => key_basic.schema.clone(),
                        "values" => value_basic.schema.clone(),
                        "min_size" => self.min_size as u64
                    };

                    if let Some(max) = self.max_size {
                        map_insert(&mut schema, "max_size", Value::from(max as u64));
                    }

                    let key_transform = key_basic.transform;
                    let value_transform = value_basic.transform;

                    // Wire format: [[key, value], ...]
                    // We always need a transform to convert pairs to HashMap
                    Some(BasicGenerator::with_transform(schema, move |raw| {
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

                            let key = if let Some(ref kt) = key_transform {
                                kt(raw_key)
                            } else {
                                let hv = super::value::HegelValue::from(raw_key.clone());
                                super::value::from_hegel_value(hv).unwrap_or_else(|e| {
                                    panic!(
                                        "hegel: failed to deserialize key: {}\nValue: {:?}",
                                        e, raw_key
                                    );
                                })
                            };

                            let value = if let Some(ref vt) = value_transform {
                                vt(raw_value)
                            } else {
                                let hv = super::value::HegelValue::from(raw_value.clone());
                                super::value::from_hegel_value(hv).unwrap_or_else(|e| {
                                    panic!(
                                        "hegel: failed to deserialize value: {}\nValue: {:?}",
                                        e, raw_value
                                    );
                                })
                            };

                            map.insert(key, value);
                        }
                        map
                    }))
                })();
                Box::new(result) as Box<dyn Any + Send + Sync>
            })
            .downcast_ref::<Option<BasicGenerator<HashMap<KT, VT>>>>()
            .expect("cached_basic type mismatch")
            .clone()
    }
}

/// Generate hash maps.
///
/// # Example
///
/// ```ignore
/// use hegel::gen::{hashmaps, integers, text};
/// use std::collections::HashMap;
///
/// // String keys
/// let string_keyed: HashMap<String, i32> = hashmaps(text(), integers()).generate();
///
/// // Integer keys
/// let int_keyed: HashMap<i32, String> = hashmaps(integers(), text()).generate();
/// ```
pub fn hashmaps<K, V>(keys: K, values: V) -> HashMapGenerator<K, V> {
    HashMapGenerator {
        keys,
        values,
        min_size: 0,
        max_size: None,
        cached_basic: OnceLock::new(),
    }
}

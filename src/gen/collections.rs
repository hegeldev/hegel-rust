use super::{generate_from_schema, group, integers, labels, Collection, Generate};
use crate::cbor_helpers::{cbor_map, map_insert};
use ciborium::Value;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

pub struct VecGenerator<G> {
    pub(crate) elements: G,
    pub(crate) min_size: usize,
    pub(crate) max_size: Option<usize>,
    pub(crate) unique: bool,
}

impl<G> VecGenerator<G> {
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

impl<T, G> Generate<Vec<T>> for VecGenerator<G>
where
    G: Generate<T>,
    T: serde::de::DeserializeOwned,
{
    fn generate(&self) -> Vec<T> {
        if let Some(schema) = self.schema() {
            // Use composed schema for single round-trip
            generate_from_schema(&schema)
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

    fn schema(&self) -> Option<Value> {
        let element_schema = self.elements.schema()?;

        let schema_type = if self.unique { "set" } else { "list" };

        let mut schema = cbor_map! {
            "type" => schema_type,
            "elements" => element_schema,
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", Value::from(max as u64));
        }

        Some(schema)
    }
}

/// Generate vectors (lists).
pub fn vecs<T, G: Generate<T>>(elements: G) -> VecGenerator<G> {
    VecGenerator {
        elements,
        min_size: 0,
        max_size: None,
        unique: false,
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
    T: serde::de::DeserializeOwned + Eq + Hash,
{
    fn generate(&self) -> HashSet<T> {
        // Generate as unique vec, convert to set
        let vec_gen = VecGenerator {
            elements: &self.elements,
            min_size: self.min_size,
            max_size: self.max_size,
            unique: true,
        };

        if let Some(schema) = vec_gen.schema() {
            let vec: Vec<T> = generate_from_schema(&schema);
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

    fn schema(&self) -> Option<Value> {
        let element_schema = self.elements.schema()?;

        let mut schema = cbor_map! {
            "type" => "set",
            "elements" => element_schema,
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", Value::from(max as u64));
        }

        Some(schema)
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
}

impl<K, V> HashMapGenerator<K, V> {
    pub fn with_min_size(mut self, min: usize) -> Self {
        self.min_size = min;
        self
    }

    pub fn with_max_size(mut self, max: usize) -> Self {
        self.max_size = Some(max);
        self
    }
}

impl<K, V, KT, VT> Generate<HashMap<KT, VT>> for HashMapGenerator<K, V>
where
    K: Generate<KT>,
    V: Generate<VT>,
    KT: serde::de::DeserializeOwned + Eq + std::hash::Hash,
    VT: serde::de::DeserializeOwned,
{
    fn generate(&self) -> HashMap<KT, VT> {
        if let Some(schema) = self.schema() {
            // Wire format: [[key, value], ...]
            let pairs: Vec<(KT, VT)> = generate_from_schema(&schema);
            pairs.into_iter().collect()
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

    fn schema(&self) -> Option<Value> {
        let key_schema = self.keys.schema()?;
        let value_schema = self.values.schema()?;

        let mut schema = cbor_map! {
            "type" => "dict",
            "keys" => key_schema,
            "values" => value_schema,
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", Value::from(max as u64));
        }

        Some(schema)
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
    }
}

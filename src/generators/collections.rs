use super::{BasicGenerator, BoxedGenerator, Collection, Generator, TestCase, labels};
use crate::utils::cbor_utils::{cbor_map, map_insert};
use ciborium::Value;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::Arc;

/// Generator for `Vec<T>`. Created by [`vecs()`].
pub struct VecGenerator<G, T> {
    pub(crate) elements: G,
    pub(crate) min_size: usize,
    pub(crate) max_size: Option<usize>,
    pub(crate) unique_by: Option<fn(&T, &T) -> bool>,
    pub(crate) _phantom: PhantomData<fn(T)>,
}

impl<G, T> VecGenerator<G, T> {
    /// Set the minimum number of elements.
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
    }

    /// Set the maximum number of elements.
    pub fn max_size(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }
}

impl<G, T: PartialEq> VecGenerator<G, T> {
    /// Require all elements to be unique.
    pub fn unique(mut self, unique: bool) -> Self {
        self.unique_by = if unique {
            Some(<T as PartialEq>::eq)
        } else {
            None
        };
        self
    }
}

impl<T, G> Generator<Vec<T>> for VecGenerator<G, T>
where
    G: Generator<T>,
{
    fn do_draw(&self, tc: &TestCase) -> Vec<T> {
        if let Some(max) = self.max_size {
            assert!(self.min_size <= max, "Cannot have max_size < min_size");
        }
        if let Some(basic) = self.as_basic() {
            basic.do_draw(tc)
        } else {
            tc.start_span(labels::LIST);
            let mut collection = Collection::new(tc, self.min_size, self.max_size);
            let mut result = Vec::new();
            while collection.more() {
                let element = self.elements.do_draw(tc);
                if let Some(eq_fn) = &self.unique_by {
                    if result.iter().any(|existing| eq_fn(existing, &element)) {
                        collection.reject(Some("duplicate element"));
                        continue;
                    }
                }
                result.push(element);
            }
            tc.stop_span(false);
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
            "unique" => self.unique_by.is_some(),
            "elements" => basic.schema().clone(),
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", max as u64);
        }

        Some(BasicGenerator::new(schema, move |raw| {
            let Value::Array(arr) = raw else {
                panic!("Expected array, got {:?}", raw) // nocov
            };
            arr.into_iter().map(|v| basic.parse_raw(v)).collect()
        }))
    }
}

/// Generate vectors with elements from the given generator.
///
/// See [`VecGenerator`] for builder methods.
///
/// # Example
///
/// ```no_run
/// use hegel::generators as gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let v: Vec<i32> = tc.draw(gs::vecs(gs::integers())
///         .min_size(1)
///         .max_size(10));
///     assert!(!v.is_empty() && v.len() <= 10);
/// }
/// ```
pub fn vecs<T, G: Generator<T>>(elements: G) -> VecGenerator<G, T> {
    VecGenerator {
        elements,
        min_size: 0,
        max_size: None,
        unique_by: None,
        _phantom: PhantomData,
    }
}

/// Generator for `HashSet<T>`. Created by [`hashsets()`].
pub struct HashSetGenerator<G, T> {
    elements: G,
    min_size: usize,
    max_size: Option<usize>,
    _phantom: PhantomData<fn(T)>,
}

impl<G, T> HashSetGenerator<G, T> {
    /// Set the minimum number of elements.
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
    }

    /// Set the maximum number of elements.
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
    fn do_draw(&self, tc: &TestCase) -> HashSet<T> {
        if let Some(max) = self.max_size {
            assert!(self.min_size <= max, "Cannot have max_size < min_size");
        }
        if let Some(basic) = self.as_basic() {
            basic.do_draw(tc)
        } else {
            // nocov start
            tc.start_span(labels::SET);
            let mut collection = Collection::new(tc, self.min_size, self.max_size);
            let mut set = HashSet::new();
            while collection.more() {
                let element = self.elements.do_draw(tc);
                if !set.insert(element) {
                    collection.reject(Some("duplicate element"));
                    // nocov end
                }
            }
            // nocov start
            assert!(set.len() >= self.min_size);
            tc.stop_span(false);
            set
            // nocov end
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
                panic!("Expected array, got {:?}", raw) // nocov
            };
            arr.into_iter().map(|v| basic.parse_raw(v)).collect()
        }))
    }
}

/// Generate hash sets with elements from the given generator.
///
/// See [`HashSetGenerator`] for builder methods.
pub fn hashsets<T, G: Generator<T>>(elements: G) -> HashSetGenerator<G, T> {
    HashSetGenerator {
        elements,
        min_size: 0,
        max_size: None,
        _phantom: PhantomData,
    }
}

/// Generator for `HashMap<K, V>`. Created by [`hashmaps()`].
pub struct HashMapGenerator<K, V, KT, VT> {
    keys: K,
    values: V,
    min_size: usize,
    max_size: Option<usize>,
    _phantom: PhantomData<fn(KT, VT)>,
}

impl<K, V, KT, VT> HashMapGenerator<K, V, KT, VT> {
    /// Set the minimum number of entries.
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
    }

    /// Set the maximum number of entries.
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
    fn do_draw(&self, tc: &TestCase) -> HashMap<KT, VT> {
        if let Some(max) = self.max_size {
            assert!(self.min_size <= max, "Cannot have max_size < min_size");
        }
        if let Some(basic) = self.as_basic() {
            basic.do_draw(tc)
        } else {
            // nocov start
            tc.start_span(labels::MAP);
            let mut collection = Collection::new(tc, self.min_size, self.max_size);
            let mut map = HashMap::new();
            while collection.more() {
                let key = self.keys.do_draw(tc);
                match map.entry(key) {
                    std::collections::hash_map::Entry::Occupied(_) => {
                        collection.reject(Some("duplicate key"));
                    }
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        let value = self.values.do_draw(tc);
                        entry.insert(value);
                        // nocov end
                    }
                }
            }
            // nocov start
            assert!(map.len() >= self.min_size);
            tc.stop_span(false);
            map
            // nocov end
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
                _ => panic!("Expected array, got {:?}", raw), // nocov
            };

            let mut map = HashMap::new();
            for value_raw in values {
                let value = match value_raw {
                    Value::Array(arr) => arr,
                    _ => panic!("Expected array, got {:?}", value_raw), // nocov
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
/// See [`HashMapGenerator`] for builder methods.
///
/// # Example
///
/// ```ignore
/// use hegel::generators as gs;
/// use std::collections::HashMap;
///
/// let map: HashMap<i32, String> = tc.draw(gs::hashmaps(gs::integers(), gs::text()));
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
    // nocov start
    fn do_draw(&self, tc: &TestCase) -> Value {
        crate::utils::cbor_utils::cbor_serialize(&self.inner.do_draw(tc))
    }

    // nocov start
    fn as_basic(&self) -> Option<BasicGenerator<'_, Value>> {
        let inner_basic = self.inner.as_basic()?;
        let schema = inner_basic.schema().clone();
        Some(BasicGenerator::new(schema, move |raw| {
            let t_val = inner_basic.parse_raw(raw);
            crate::utils::cbor_utils::cbor_serialize(&t_val)
        }))
    }
}

/// Builder for fixed-key dictionary generators. Created by [`fixed_dicts()`].
///
/// Add fields with [`field()`](Self::field), then call [`build()`](Self::build)
/// to get the generator.
pub struct FixedDictBuilder<'a> {
    fields: Vec<(String, BoxedGenerator<'a, Value>)>,
}

impl<'a> FixedDictBuilder<'a> {
    /// Add a field with a name and generator.
    // nocov start
    pub fn field<T, G>(mut self, name: &str, generator: G) -> Self
    where
        G: Generator<T> + Send + Sync + 'a,
        T: serde::Serialize + 'a,
        // nocov end
    {
        // nocov start
        let boxed = BoxedGenerator {
            inner: Arc::new(MappedToValue {
                inner: generator,
                _phantom: PhantomData,
                // nocov end
            }),
        };
        self.fields.push((name.to_string(), boxed)); // nocov
        self // nocov
    }

    /// Build the generator.
    // nocov start
    pub fn build(self) -> FixedDictGenerator<'a> {
        FixedDictGenerator {
            fields: self.fields,
            // nocov end
        }
    }
}

/// Generator for CBOR maps with fixed keys. Created via [`FixedDictBuilder`].
pub struct FixedDictGenerator<'a> {
    fields: Vec<(String, BoxedGenerator<'a, Value>)>,
}

impl Generator<Value> for FixedDictGenerator<'_> {
    // nocov start
    fn do_draw(&self, tc: &TestCase) -> Value {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(tc)
        } else {
            tc.start_span(labels::FIXED_DICT);
            let entries: Vec<(Value, Value)> = self
                .fields
                .iter()
                .map(|(name, g)| (Value::Text(name.clone()), g.do_draw(tc)))
                .collect();
            tc.stop_span(false);
            Value::Map(entries)
            // nocov end
        }
    }

    // nocov start
    fn as_basic(&self) -> Option<BasicGenerator<'_, Value>> {
        let basics: Vec<BasicGenerator<'_, Value>> = self
            .fields
            .iter()
            .map(|(_, g)| g.as_basic())
            .collect::<Option<Vec<_>>>()?;

        let schemas: Vec<Value> = basics.iter().map(|b| b.schema().clone()).collect();

        let schema = cbor_map! {
            "type" => "tuple",
            "elements" => Value::Array(schemas)
        // nocov end
        };

        let field_names: Vec<String> = self.fields.iter().map(|(name, _)| name.clone()).collect(); // nocov

        // nocov start
        Some(BasicGenerator::new(schema, move |raw| {
            let arr = match raw {
                Value::Array(arr) => arr,
                _ => panic!("Expected array from tuple schema, got {:?}", raw),
                // nocov end
            };

            // nocov start
            let entries: Vec<(Value, Value)> = field_names
                .iter()
                .zip(basics.iter())
                .zip(arr)
                .map(|((name, basic), val)| (Value::Text(name.clone()), basic.parse_raw(val)))
                .collect();
            Value::Map(entries)
            // nocov end
        }))
    }
}

/// Create a generator for dictionaries with fixed keys.
///
/// See [`FixedDictBuilder`] for builder methods.
///
/// # Example
///
/// ```no_run
/// use hegel::generators::{self as gs, Generator};
///
/// let generator = gs::fixed_dicts()
///     .field("name", gs::text())
///     .field("age", gs::integers::<u32>())
///     .build();
/// ```
// nocov start
pub fn fixed_dicts<'a>() -> FixedDictBuilder<'a> {
    FixedDictBuilder { fields: Vec::new() }
    // nocov end
}

/// Generator for fixed-size arrays `[T; N]`. Created by [`arrays()`].
pub struct ArrayGenerator<G, T, const N: usize> {
    element: G,
    _phantom: PhantomData<fn() -> T>,
}

impl<G, T, const N: usize> ArrayGenerator<G, T, N> {
    #[doc(hidden)]
    pub fn new(element: G) -> Self {
        ArrayGenerator {
            element,
            _phantom: PhantomData,
        }
    }
}

/// Generate fixed-size arrays `[T; N]` with elements from the given generator.
pub fn arrays<G: Generator<T> + Send + Sync, T, const N: usize>(
    element: G,
) -> ArrayGenerator<G, T, N> {
    ArrayGenerator::new(element)
}

impl<G: Generator<T> + Send + Sync, T, const N: usize> Generator<[T; N]>
    for ArrayGenerator<G, T, N>
{
    fn do_draw(&self, tc: &TestCase) -> [T; N] {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(tc)
        } else {
            tc.start_span(labels::TUPLE);
            let result = std::array::from_fn(|_| self.element.do_draw(tc));
            tc.stop_span(false);
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
                _ => panic!("Expected array from tuple schema, got {:?}", raw), // nocov
            };
            assert_eq!(arr.len(), N);
            let mut iter = arr.into_iter();
            std::array::from_fn(|_| basic.parse_raw(iter.next().unwrap()))
        }))
    }
}

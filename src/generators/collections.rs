use super::{Collection, Generator, PrintableGenerator, TestCase, labels};
use crate::control::hegel_internal_assert;
use crate::pretty::{PrettyPrintable, PrettyPrinter};
use crate::test_case::invalid_argument;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::marker::PhantomData;

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
            if self.min_size > max {
                invalid_argument!("Cannot have max_size < min_size");
            }
        }
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

/// Printing a vec draw is fully compositional: each element prints through
/// the element generator inside a speculative region, so an element rejected
/// for uniqueness discards its own output (including its separator).
impl<T, G> PrintableGenerator<Vec<T>> for VecGenerator<G, T>
where
    G: PrintableGenerator<T>,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> Vec<T> {
        if let Some(max) = self.max_size {
            if self.min_size > max {
                invalid_argument!("Cannot have max_size < min_size");
            }
        }
        tc.start_span(labels::LIST);
        printer.begin_group(1, "[");
        let mut collection = Collection::new(tc, self.min_size, self.max_size);
        let mut result = Vec::new();
        while collection.more() {
            let mut speculation = printer.speculate();
            if !result.is_empty() {
                speculation.printer().text(",");
                speculation.printer().breakable(" ");
            }
            let element = self.elements.do_draw_and_print(tc, speculation.printer());
            if let Some(eq_fn) = &self.unique_by {
                if result.iter().any(|existing| eq_fn(existing, &element)) {
                    speculation.abort();
                    collection.reject(Some("duplicate element"));
                    continue;
                }
            }
            speculation.commit();
            result.push(element);
        }
        printer.end_group(1, "]");
        tc.stop_span(false);
        result
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

/// The largest enumerated value pool [`HashSetGenerator`] will draw
/// without replacement from. Mirrors the bound the engine's old
/// unique-sampled-list strategy used.
const MAX_UNIQUE_POOL: usize = 10_000;

impl<T, G> Generator<HashSet<T>> for HashSetGenerator<G, T>
where
    G: Generator<T>,
    T: Eq + Hash,
{
    fn do_draw(&self, tc: &TestCase) -> HashSet<T> {
        if let Some(max) = self.max_size {
            if self.min_size > max {
                invalid_argument!("Cannot have max_size < min_size");
            }
        }
        tc.start_span(labels::SET);
        let set = match self.enumerated_pool() {
            Some(pool) => self.draw_from_pool(tc, pool),
            None => self.draw_by_rejection(tc),
        };
        tc.stop_span(false);
        set
    }
}

impl<T, G> HashSetGenerator<G, T>
where
    G: Generator<T>,
    T: Eq + Hash,
{
    /// The distinct values of an enumerable element generator, in first
    /// occurrence order (which the shrinker treats as simplest-first), when
    /// there are few enough of them to draw without replacement.
    fn enumerated_pool(&self) -> Option<Vec<T>> {
        let values = self.elements.enumerate_values()?;
        if values.is_empty() || values.len() > MAX_UNIQUE_POOL {
            return None;
        }
        let mut by_hash: HashMap<u64, Vec<usize>> = HashMap::new();
        let mut pool: Vec<T> = Vec::new();
        for v in values {
            let bucket = by_hash.entry(fingerprint(&v)).or_default();
            if bucket.iter().any(|&i| pool[i] == v) {
                continue;
            }
            bucket.push(pool.len());
            pool.push(v);
        }
        Some(pool)
    }

    /// Draw set elements as indices into a shrinking pool of the remaining
    /// values, avoiding the coupon-collector problem when the set must
    /// contain most of a small alphabet. Port of Hypothesis's
    /// `UniqueSampledListStrategy`.
    fn draw_from_pool(&self, tc: &TestCase, mut remaining: Vec<T>) -> HashSet<T> {
        if self.min_size > remaining.len() {
            invalid_argument!(
                "Cannot generate a set: min_size {} is larger than the {} distinct values the element generator can produce",
                self.min_size,
                remaining.len()
            );
        }
        let effective_max = self
            .max_size
            .map_or(remaining.len(), |m| m.min(remaining.len()));
        let mut collection = Collection::new(tc, self.min_size, Some(effective_max));
        let mut set = HashSet::new();
        loop {
            if remaining.is_empty() || !collection.more() {
                break;
            }
            let j = tc.generate_integer_i64(0, remaining.len() as i64 - 1) as usize;
            set.insert(remaining.remove(j));
        }
        set
    }

    fn draw_by_rejection(&self, tc: &TestCase) -> HashSet<T> {
        let mut collection = Collection::new(tc, self.min_size, self.max_size);
        let mut set = HashSet::new();
        while collection.more() {
            let element = self.elements.do_draw(tc);
            if !set.insert(element) {
                collection.reject(Some("duplicate element"));
            }
        }
        hegel_internal_assert!(set.len() >= self.min_size);
        set
    }
}

/// Printing a set draw prints each element's own [`PrettyPrintable`]
/// representation after it is accepted (rather than delegating to the element
/// generator) because the without-replacement pool path produces elements by
/// index, with no element draw to delegate to; printing the same way on both
/// paths keeps the output consistent.
impl<T, G> PrintableGenerator<HashSet<T>> for HashSetGenerator<G, T>
where
    G: Generator<T>,
    T: Eq + Hash + PrettyPrintable,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> HashSet<T> {
        if let Some(max) = self.max_size {
            if self.min_size > max {
                invalid_argument!("Cannot have max_size < min_size");
            }
        }
        tc.start_span(labels::SET);
        printer.begin_group(1, "{");
        let set = match self.enumerated_pool() {
            Some(pool) => self.print_from_pool(tc, printer, pool),
            None => self.print_by_rejection(tc, printer),
        };
        printer.end_group(1, "}");
        tc.stop_span(false);
        set
    }
}

impl<T, G> HashSetGenerator<G, T>
where
    G: Generator<T>,
    T: Eq + Hash + PrettyPrintable,
{
    fn print_from_pool(
        &self,
        tc: &TestCase,
        printer: &mut PrettyPrinter,
        mut remaining: Vec<T>,
    ) -> HashSet<T> {
        if self.min_size > remaining.len() {
            invalid_argument!(
                "Cannot generate a set: min_size {} is larger than the {} distinct values the element generator can produce",
                self.min_size,
                remaining.len()
            );
        }
        let effective_max = self
            .max_size
            .map_or(remaining.len(), |m| m.min(remaining.len()));
        let mut collection = Collection::new(tc, self.min_size, Some(effective_max));
        let mut set = HashSet::new();
        loop {
            if remaining.is_empty() || !collection.more() {
                break;
            }
            let j = tc.generate_integer_i64(0, remaining.len() as i64 - 1) as usize;
            let value = remaining.remove(j);
            if !set.is_empty() {
                printer.text(",");
                printer.breakable(" ");
            }
            value.pretty_print(printer);
            set.insert(value);
        }
        set
    }

    fn print_by_rejection(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> HashSet<T> {
        let mut collection = Collection::new(tc, self.min_size, self.max_size);
        let mut set = HashSet::new();
        while collection.more() {
            let element = self.elements.do_draw(tc);
            if set.contains(&element) {
                collection.reject(Some("duplicate element"));
            } else {
                if !set.is_empty() {
                    printer.text(",");
                    printer.breakable(" ");
                }
                element.pretty_print(printer);
                set.insert(element);
            }
        }
        hegel_internal_assert!(set.len() >= self.min_size);
        set
    }
}

/// A hashable stand-in for a value that is only `Eq + Hash`, used to dedup
/// the enumerated pool.
fn fingerprint<T: Eq + Hash>(v: &T) -> u64 {
    use std::hash::{DefaultHasher, Hasher};
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
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
            if self.min_size > max {
                invalid_argument!("Cannot have max_size < min_size");
            }
        }
        tc.start_span(labels::MAP);
        let map = match self.enumerated_key_pool() {
            Some(pool) => self.draw_from_key_pool(tc, pool),
            None => self.draw_by_rejection(tc),
        };
        tc.stop_span(false);
        map
    }
}

impl<K, V, KT, VT> HashMapGenerator<K, V, KT, VT>
where
    K: Generator<KT>,
    V: Generator<VT>,
    KT: Eq + std::hash::Hash,
{
    /// The distinct values of an enumerable key generator, in first
    /// occurrence order, when there are few enough of them to draw without
    /// replacement. Mirrors [`HashSetGenerator::enumerated_pool`].
    fn enumerated_key_pool(&self) -> Option<Vec<KT>> {
        let values = self.keys.enumerate_values()?;
        if values.is_empty() || values.len() > MAX_UNIQUE_POOL {
            return None;
        }
        let mut by_hash: HashMap<u64, Vec<usize>> = HashMap::new();
        let mut pool: Vec<KT> = Vec::new();
        for v in values {
            let bucket = by_hash.entry(fingerprint(&v)).or_default();
            if bucket.iter().any(|&i| pool[i] == v) {
                continue;
            }
            bucket.push(pool.len());
            pool.push(v);
        }
        Some(pool)
    }

    /// Draw keys as indices into a shrinking pool of the remaining values,
    /// avoiding the coupon-collector problem when the map must contain most
    /// of a small key alphabet; each key's value is drawn right after it.
    fn draw_from_key_pool(&self, tc: &TestCase, mut remaining: Vec<KT>) -> HashMap<KT, VT> {
        if self.min_size > remaining.len() {
            invalid_argument!(
                "Cannot generate a map: min_size {} is larger than the {} distinct keys the key generator can produce",
                self.min_size,
                remaining.len()
            );
        }
        let effective_max = self
            .max_size
            .map_or(remaining.len(), |m| m.min(remaining.len()));
        let mut collection = Collection::new(tc, self.min_size, Some(effective_max));
        let mut map = HashMap::new();
        loop {
            if remaining.is_empty() || !collection.more() {
                break;
            }
            let j = tc.generate_integer_i64(0, remaining.len() as i64 - 1) as usize;
            let key = remaining.remove(j);
            let value = self.values.do_draw(tc);
            map.insert(key, value);
        }
        map
    }

    fn draw_by_rejection(&self, tc: &TestCase) -> HashMap<KT, VT> {
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
                }
            }
        }
        hegel_internal_assert!(map.len() >= self.min_size);
        map
    }
}

/// Printing a map draw prints each accepted key's own [`PrettyPrintable`]
/// representation (the without-replacement key-pool path produces keys by
/// index, so there is no key draw to delegate to) and each value
/// compositionally through the value generator. Duplicate keys are rejected
/// before anything is printed for the entry.
impl<K, V, KT, VT> PrintableGenerator<HashMap<KT, VT>> for HashMapGenerator<K, V, KT, VT>
where
    K: Generator<KT>,
    V: PrintableGenerator<VT>,
    KT: Eq + std::hash::Hash + PrettyPrintable,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> HashMap<KT, VT> {
        if let Some(max) = self.max_size {
            if self.min_size > max {
                invalid_argument!("Cannot have max_size < min_size");
            }
        }
        tc.start_span(labels::MAP);
        printer.begin_group(1, "{");
        let map = match self.enumerated_key_pool() {
            Some(pool) => self.print_from_key_pool(tc, printer, pool),
            None => self.print_by_rejection(tc, printer),
        };
        printer.end_group(1, "}");
        tc.stop_span(false);
        map
    }
}

impl<K, V, KT, VT> HashMapGenerator<K, V, KT, VT>
where
    K: Generator<KT>,
    V: PrintableGenerator<VT>,
    KT: Eq + std::hash::Hash + PrettyPrintable,
{
    fn print_from_key_pool(
        &self,
        tc: &TestCase,
        printer: &mut PrettyPrinter,
        mut remaining: Vec<KT>,
    ) -> HashMap<KT, VT> {
        if self.min_size > remaining.len() {
            invalid_argument!(
                "Cannot generate a map: min_size {} is larger than the {} distinct keys the key generator can produce",
                self.min_size,
                remaining.len()
            );
        }
        let effective_max = self
            .max_size
            .map_or(remaining.len(), |m| m.min(remaining.len()));
        let mut collection = Collection::new(tc, self.min_size, Some(effective_max));
        let mut map = HashMap::new();
        loop {
            if remaining.is_empty() || !collection.more() {
                break;
            }
            let j = tc.generate_integer_i64(0, remaining.len() as i64 - 1) as usize;
            let key = remaining.remove(j);
            if !map.is_empty() {
                printer.text(",");
                printer.breakable(" ");
            }
            key.pretty_print(printer);
            printer.text(": ");
            let value = self.values.do_draw_and_print(tc, printer);
            map.insert(key, value);
        }
        map
    }

    fn print_by_rejection(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> HashMap<KT, VT> {
        let mut collection = Collection::new(tc, self.min_size, self.max_size);
        let mut map = HashMap::new();
        while collection.more() {
            let key = self.keys.do_draw(tc);
            let separate = !map.is_empty();
            match map.entry(key) {
                std::collections::hash_map::Entry::Occupied(_) => {
                    collection.reject(Some("duplicate key"));
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    if separate {
                        printer.text(",");
                        printer.breakable(" ");
                    }
                    entry.key().pretty_print(printer);
                    printer.text(": ");
                    let value = self.values.do_draw_and_print(tc, printer);
                    entry.insert(value);
                }
            }
        }
        hegel_internal_assert!(map.len() >= self.min_size);
        map
    }
}

/// Generate hash maps.
///
/// See [`HashMapGenerator`] for builder methods.
///
/// # Example
///
/// ```no_run
/// use hegel::generators as gs;
/// use std::collections::HashMap;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let map: HashMap<i32, String> = tc.draw(gs::hashmaps(gs::integers(), gs::text()));
/// }
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
        tc.start_span(labels::TUPLE);
        let result = std::array::from_fn(|_| self.element.do_draw(tc));
        tc.stop_span(false);
        result
    }
}

impl<G: PrintableGenerator<T> + Send + Sync, T, const N: usize> PrintableGenerator<[T; N]>
    for ArrayGenerator<G, T, N>
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> [T; N] {
        tc.start_span(labels::TUPLE);
        printer.begin_group(1, "[");
        let result = std::array::from_fn(|i| {
            if i > 0 {
                printer.text(",");
                printer.breakable(" ");
            }
            self.element.do_draw_and_print(tc, printer)
        });
        printer.end_group(1, "]");
        tc.stop_span(false);
        result
    }
}

use super::{Collection, Generator, PrintableGenerator, TestCase, labels};
use crate::control::hegel_internal_assert;
use crate::pretty::PrettyPrinter;
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

impl<G, T> VecGenerator<G, T> {
    /// The one vec body both draw paths run: each element draws inside a
    /// speculative print region, so an element rejected for uniqueness
    /// discards its own output (including its separator). Only how each
    /// element is drawn (silently or printing) is injected.
    fn draw_vec(
        &self,
        tc: &TestCase,
        printer: &mut PrettyPrinter,
        draw: impl Fn(&G, &TestCase, &mut PrettyPrinter) -> T,
    ) -> Vec<T> {
        if let Some(max) = self.max_size {
            if self.min_size > max {
                invalid_argument!("Cannot have max_size < min_size");
            }
        }
        tc.start_span(labels::LIST);
        printer.begin_group(5, "vec![");
        let mut collection = Collection::new(tc, self.min_size, self.max_size);
        let mut result = Vec::new();
        while collection.more() {
            let mut speculation = printer.speculate();
            if !result.is_empty() {
                speculation.printer().text(",");
                speculation.printer().breakable(" ");
            }
            let element = draw(&self.elements, tc, speculation.printer());
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
        printer.end_group(5, "]");
        tc.stop_span(false);
        result
    }
}

impl<T, G> Generator<Vec<T>> for VecGenerator<G, T>
where
    G: Generator<T>,
{
    fn do_draw(&self, tc: &TestCase) -> Vec<T> {
        self.draw_vec(tc, &mut PrettyPrinter::noop(), |elements, tc, _| {
            elements.do_draw(tc)
        })
    }
}

impl<T, G> PrintableGenerator<Vec<T>> for VecGenerator<G, T>
where
    G: PrintableGenerator<T>,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> Vec<T> {
        self.draw_vec(tc, printer, |elements, tc, printer| {
            elements.draw_and_print(tc, printer)
        })
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

impl<G, T> HashSetGenerator<G, T>
where
    T: Eq + Hash,
{
    /// The one set body both draw paths run: each element draws inside a
    /// speculative print region, so an element rejected as a duplicate
    /// discards its own output (including its separator). Only how each
    /// element is drawn (silently or printing) is injected.
    fn draw_set(
        &self,
        tc: &TestCase,
        printer: &mut PrettyPrinter,
        draw: impl Fn(&G, &TestCase, &mut PrettyPrinter) -> T,
    ) -> HashSet<T> {
        if let Some(max) = self.max_size {
            if self.min_size > max {
                invalid_argument!("Cannot have max_size < min_size");
            }
        }
        tc.start_span(labels::SET);
        printer.begin_group(15, "HashSet::from([");
        let mut collection = Collection::new(tc, self.min_size, self.max_size);
        let mut set = HashSet::new();
        while collection.more() {
            let mut speculation = printer.speculate();
            if !set.is_empty() {
                speculation.printer().text(",");
                speculation.printer().breakable(" ");
            }
            let element = draw(&self.elements, tc, speculation.printer());
            if set.contains(&element) {
                speculation.abort();
                collection.reject(Some("duplicate element"));
            } else {
                speculation.commit();
                set.insert(element);
            }
        }
        hegel_internal_assert!(set.len() >= self.min_size);
        printer.end_group(15, "])");
        tc.stop_span(false);
        set
    }
}

impl<T, G> Generator<HashSet<T>> for HashSetGenerator<G, T>
where
    G: Generator<T>,
    T: Eq + Hash,
{
    fn do_draw(&self, tc: &TestCase) -> HashSet<T> {
        self.draw_set(tc, &mut PrettyPrinter::noop(), |elements, tc, _| {
            elements.do_draw(tc)
        })
    }
}

impl<T, G> PrintableGenerator<HashSet<T>> for HashSetGenerator<G, T>
where
    G: PrintableGenerator<T>,
    T: Eq + Hash,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> HashSet<T> {
        self.draw_set(tc, printer, |elements, tc, printer| {
            elements.draw_and_print(tc, printer)
        })
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
        self.draw_map(
            tc,
            &mut PrettyPrinter::noop(),
            |keys, tc, _| keys.do_draw(tc),
            |values, tc, _| values.do_draw(tc),
        )
    }
}

impl<K, V, KT, VT> HashMapGenerator<K, V, KT, VT>
where
    KT: Eq + std::hash::Hash,
{
    /// The one map body both draw paths run: each entry draws inside a
    /// speculative print region, so an entry rejected for a duplicate key
    /// discards its own output (including its separator), and the value is
    /// only drawn once the key is known to be fresh. Only how keys and
    /// values are drawn (silently or printing) is injected.
    fn draw_map(
        &self,
        tc: &TestCase,
        printer: &mut PrettyPrinter,
        draw_key: impl Fn(&K, &TestCase, &mut PrettyPrinter) -> KT,
        draw_value: impl Fn(&V, &TestCase, &mut PrettyPrinter) -> VT,
    ) -> HashMap<KT, VT> {
        if let Some(max) = self.max_size {
            if self.min_size > max {
                invalid_argument!("Cannot have max_size < min_size");
            }
        }
        tc.start_span(labels::MAP);
        printer.begin_group(15, "HashMap::from([");
        let mut collection = Collection::new(tc, self.min_size, self.max_size);
        let mut map = HashMap::new();
        while collection.more() {
            let mut speculation = printer.speculate();
            if !map.is_empty() {
                speculation.printer().text(",");
                speculation.printer().breakable(" ");
            }
            speculation.printer().text("(");
            let key = draw_key(&self.keys, tc, speculation.printer());
            match map.entry(key) {
                std::collections::hash_map::Entry::Occupied(_) => {
                    speculation.abort();
                    collection.reject(Some("duplicate key"));
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    speculation.printer().text(", ");
                    let value = draw_value(&self.values, tc, speculation.printer());
                    speculation.printer().text(")");
                    speculation.commit();
                    entry.insert(value);
                }
            }
        }
        hegel_internal_assert!(map.len() >= self.min_size);
        printer.end_group(15, "])");
        tc.stop_span(false);
        map
    }
}

impl<K, V, KT, VT> PrintableGenerator<HashMap<KT, VT>> for HashMapGenerator<K, V, KT, VT>
where
    K: PrintableGenerator<KT>,
    V: PrintableGenerator<VT>,
    KT: Eq + std::hash::Hash,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> HashMap<KT, VT> {
        self.draw_map(
            tc,
            printer,
            |keys, tc, printer| keys.draw_and_print(tc, printer),
            |values, tc, printer| values.draw_and_print(tc, printer),
        )
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

impl<G, T, const N: usize> ArrayGenerator<G, T, N> {
    /// The one array body both draw paths run; only how each element is
    /// drawn (silently or printing) is injected.
    fn draw_array(
        &self,
        tc: &TestCase,
        printer: &mut PrettyPrinter,
        draw: impl Fn(&G, &TestCase, &mut PrettyPrinter) -> T,
    ) -> [T; N] {
        tc.start_span(labels::TUPLE);
        printer.begin_group(1, "[");
        let result = std::array::from_fn(|i| {
            if i > 0 {
                printer.text(",");
                printer.breakable(" ");
            }
            draw(&self.element, tc, printer)
        });
        printer.end_group(1, "]");
        tc.stop_span(false);
        result
    }
}

impl<G: Generator<T> + Send + Sync, T, const N: usize> Generator<[T; N]>
    for ArrayGenerator<G, T, N>
{
    fn do_draw(&self, tc: &TestCase) -> [T; N] {
        self.draw_array(tc, &mut PrettyPrinter::noop(), |element, tc, _| {
            element.do_draw(tc)
        })
    }
}

impl<G: PrintableGenerator<T> + Send + Sync, T, const N: usize> PrintableGenerator<[T; N]>
    for ArrayGenerator<G, T, N>
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> [T; N] {
        self.draw_array(tc, printer, |element, tc, printer| {
            element.draw_and_print(tc, printer)
        })
    }
}

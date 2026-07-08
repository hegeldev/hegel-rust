use crate::pretty::{PrettyPrintable, PrettyPrinter};
use crate::test_case::{TestCase, invalid_argument, labels};
use std::marker::PhantomData;
use std::sync::Arc;

/// The core trait for all generators.
///
/// Generators produce values of type `T` by drawing from the engine through
/// the [`TestCase`] passed to [`do_draw`](Self::do_draw).
pub trait Generator<T> {
    /// Produce a value.
    #[doc(hidden)]
    fn do_draw(&self, tc: &TestCase) -> T;

    /// Transform generated values using a function.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators::{self as gs, Generator};
    ///
    /// // Generate even integers by doubling
    /// let evens = gs::integers::<i32>().map(|n| n * 2);
    /// ```
    fn map<U, F>(self, f: F) -> Mapped<T, U, F, Self>
    where
        Self: Sized,
        F: Fn(T) -> U + Send + Sync,
    {
        Mapped {
            source: self,
            f: Arc::new(f),
            _phantom: PhantomData,
        }
    }

    /// Generate a value, then use it to choose or configure another generator.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators::{self as gs, Generator};
    ///
    /// // Generate a length, then a vec of exactly that length
    /// let generator = gs::integers::<usize>()
    ///     .min_value(1)
    ///     .max_value(10)
    ///     .flat_map(|len| gs::vecs(gs::integers::<i32>())
    ///         .min_size(len)
    ///         .max_size(len));
    /// ```
    fn flat_map<U, G, F>(self, f: F) -> FlatMapped<T, U, G, F, Self>
    where
        Self: Sized,
        G: Generator<U>,
        F: Fn(T) -> G + Send + Sync,
    {
        FlatMapped {
            source: self,
            f,
            _phantom: PhantomData,
        }
    }

    /// Only keep generated values that satisfy the predicate.
    ///
    /// Retries up to 3 times, then calls `assume(false)` to reject the test case.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators::{self as gs, Generator};
    ///
    /// // Generate integers, then filter out the even ones
    /// let odds = gs::integers::<i32>().filter(|n| n % 2 != 0);
    /// ```
    fn filter<F>(self, predicate: F) -> Filtered<T, F, Self>
    where
        Self: Sized,
        F: Fn(&T) -> bool + Send + Sync,
    {
        Filtered {
            source: self,
            predicate,
            enumerated: std::sync::OnceLock::new(),
            _phantom: PhantomData,
        }
    }

    /// Return all possible values if this generator has a known finite value set.
    ///
    /// Used by [`Filtered`]: instead of rejection sampling, enumerate the
    /// valid elements and pick one directly. Mirrors Hypothesis's
    /// `SampledFromStrategy.do_filtered_draw` optimization.
    #[doc(hidden)]
    fn enumerate_values(&self) -> Option<Vec<T>> {
        None
    }

    /// Convert this generator into a type-erased boxed generator.
    ///
    /// This is needed when you have generators of different concrete types
    /// but the same output type and need to store them together, e.g. in a
    /// `Vec` or when passing to [`one_of()`](super::one_of).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators::{self as gs, Generator};
    ///
    /// // Different generator types producing the same output type —
    /// // boxing lets them be stored in a vec and passed to one_of
    /// let generator = vec![
    ///     gs::integers::<i32>().min_value(0).max_value(10).boxed(),
    ///     gs::integers::<i32>().map(|n| n * 100).boxed(),
    ///     gs::sampled_from(vec![1, 2, 3]).boxed(),
    /// ];
    /// ```
    fn boxed<'a>(self) -> BoxedGenerator<'a, T>
    where
        Self: Sized + Send + Sync + 'a,
    {
        BoxedGenerator {
            inner: Arc::new(self),
        }
    }

    /// Make this generator printable by describing each drawn value with `print`.
    ///
    /// This is the fine-grained control point for printing: the resulting
    /// generator satisfies [`PrintableGenerator`] for any source generator,
    /// with the drawn value's representation produced by `print` instead of
    /// the value's own [`PrettyPrintable`] implementation.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators::{self as gs, Generator};
    ///
    /// let masked = gs::text().print_with(|_, printer| printer.text("<secret>"));
    /// ```
    fn print_with<F>(self, print: F) -> PrintWith<Self, F>
    where
        Self: Sized,
        F: Fn(&T, &mut PrettyPrinter) + Send + Sync,
    {
        PrintWith {
            source: self,
            print,
        }
    }

    /// Make this generator printable by printing each drawn value's own
    /// [`PrettyPrintable`] representation.
    ///
    /// Useful when a combinator chain loses printability — e.g. a `map` to a
    /// type that does implement [`PrettyPrintable`] but whose source cannot
    /// prove it, or a hand-written [`Generator`] implementation.
    fn print_as_value(self) -> PrintAsValue<Self>
    where
        Self: Sized,
        T: PrettyPrintable,
    {
        PrintAsValue { source: self }
    }
}

/// A [`Generator`] that can print each value's representation as it draws it.
///
/// Only printable generators can be passed to [`TestCase::draw`]; a plain
/// [`Generator`] can still be drawn with [`TestCase::draw_silent`]. Most
/// generators in the library are printable — leaves unconditionally,
/// structural combinators (collections, tuples, `optional`, `one_of!`,
/// `flat_map`) whenever their component generators are, and value-transforming
/// combinators (`map`, `filter`, `just`, `sampled_from`, composites) whenever
/// the produced type implements [`PrettyPrintable`]. For everything else there
/// is [`Generator::print_as_value`] and [`Generator::print_with`].
///
/// # Contract
///
/// `do_draw_and_print` must draw **exactly** the same choices as
/// [`Generator::do_draw`] — the engine explores with the silent path and
/// replays failures with the printing path, so any divergence makes failures
/// unreplayable. Implementations should share drawing code between the two
/// paths, differing only in what they print.
pub trait PrintableGenerator<T>: Generator<T> {
    /// Produce a value, printing its representation to `printer` as it is
    /// drawn.
    #[doc(hidden)]
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T;

    /// Convert this generator into a type-erased boxed printable generator,
    /// as accepted by [`one_of!`](crate::one_of).
    fn boxed_printable<'a>(self) -> BoxedPrintableGenerator<'a, T>
    where
        Self: Sized + Send + Sync + 'a,
    {
        BoxedPrintableGenerator {
            inner: Arc::new(self),
        }
    }
}

/// Draw from `generator` silently, then print the drawn value's own
/// [`PrettyPrintable`] representation. The shared implementation for every
/// generator that prints by value.
pub(crate) fn draw_and_print_value<T: PrettyPrintable>(
    generator: &impl Generator<T>,
    tc: &TestCase,
    printer: &mut PrettyPrinter,
) -> T {
    let value = generator.do_draw(tc);
    value.pretty_print(printer);
    value
}

/// Result of [`Generator::print_with`].
pub struct PrintWith<G, F> {
    source: G,
    print: F,
}

impl<T, G, F> Generator<T> for PrintWith<G, F>
where
    G: Generator<T>,
    F: Fn(&T, &mut PrettyPrinter) + Send + Sync,
{
    fn do_draw(&self, tc: &TestCase) -> T {
        self.source.do_draw(tc)
    }
}

impl<T, G, F> PrintableGenerator<T> for PrintWith<G, F>
where
    G: Generator<T>,
    F: Fn(&T, &mut PrettyPrinter) + Send + Sync,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        let value = self.source.do_draw(tc);
        (self.print)(&value, printer);
        value
    }
}

/// Result of [`Generator::print_as_value`].
pub struct PrintAsValue<G> {
    source: G,
}

impl<T, G> Generator<T> for PrintAsValue<G>
where
    G: Generator<T>,
    T: PrettyPrintable,
{
    fn do_draw(&self, tc: &TestCase) -> T {
        self.source.do_draw(tc)
    }

    fn enumerate_values(&self) -> Option<Vec<T>> {
        self.source.enumerate_values()
    }
}

impl<T, G> PrintableGenerator<T> for PrintAsValue<G>
where
    G: Generator<T>,
    T: PrettyPrintable,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        draw_and_print_value(&self.source, tc, printer)
    }
}

impl<T, G: Generator<T>> Generator<T> for &G {
    fn do_draw(&self, tc: &TestCase) -> T {
        (*self).do_draw(tc)
    }

    fn enumerate_values(&self) -> Option<Vec<T>> {
        (*self).enumerate_values()
    }
}

impl<T, G: PrintableGenerator<T>> PrintableGenerator<T> for &G {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        (*self).do_draw_and_print(tc, printer)
    }
}

/// Result of [`Generator::map`].
pub struct Mapped<T, U, F, G> {
    source: G,
    f: Arc<F>,
    _phantom: PhantomData<fn(T) -> U>,
}

impl<T, U, F, G> Generator<U> for Mapped<T, U, F, G>
where
    G: Generator<T>,
    F: Fn(T) -> U + Send + Sync,
{
    fn do_draw(&self, tc: &TestCase) -> U {
        tc.start_span(labels::MAPPED);
        let result = (self.f)(self.source.do_draw(tc));
        tc.stop_span(false);
        result
    }

    fn enumerate_values(&self) -> Option<Vec<U>> {
        self.source
            .enumerate_values()
            .map(|vals| vals.into_iter().map(|v| (self.f)(v)).collect())
    }
}

impl<T, U, F, G> PrintableGenerator<U> for Mapped<T, U, F, G>
where
    G: Generator<T>,
    F: Fn(T) -> U + Send + Sync,
    U: PrettyPrintable,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> U {
        draw_and_print_value(self, tc, printer)
    }
}

/// Result of [`Generator::flat_map`].
pub struct FlatMapped<T, U, G2, F, G1> {
    source: G1,
    f: F,
    _phantom: PhantomData<fn(T) -> (U, G2)>,
}

impl<T, U, G2, F, G1> Generator<U> for FlatMapped<T, U, G2, F, G1>
where
    G1: Generator<T>,
    G2: Generator<U>,
    F: Fn(T) -> G2 + Send + Sync,
{
    fn do_draw(&self, tc: &TestCase) -> U {
        tc.start_span(labels::FLAT_MAP);
        let intermediate = self.source.do_draw(tc);
        let next_gen = (self.f)(intermediate);
        let result = next_gen.do_draw(tc);
        tc.stop_span(false);
        result
    }
}

impl<T, U, G2, F, G1> PrintableGenerator<U> for FlatMapped<T, U, G2, F, G1>
where
    G1: Generator<T>,
    G2: PrintableGenerator<U>,
    F: Fn(T) -> G2 + Send + Sync,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> U {
        tc.start_span(labels::FLAT_MAP);
        let intermediate = self.source.do_draw(tc);
        let next_gen = (self.f)(intermediate);
        let result = next_gen.do_draw_and_print(tc, printer);
        tc.stop_span(false);
        result
    }
}

/// Result of [`Generator::filter`].
pub struct Filtered<T, F, G> {
    source: G,
    predicate: F,
    /// The source's enumerated values with the predicate applied, computed
    /// once: for an enumerable source like `sampled_from`, re-enumerating
    /// (which clones the whole element vector) on every draw is the dominant
    /// cost of a filtered draw.
    enumerated: std::sync::OnceLock<Option<Vec<T>>>,
    _phantom: PhantomData<fn() -> T>,
}

impl<T, F, G> Filtered<T, F, G>
where
    T: Clone + Send + Sync,
    G: Generator<T>,
    F: Fn(&T) -> bool + Send + Sync,
{
    fn enumerated(&self) -> &Option<Vec<T>> {
        self.enumerated.get_or_init(|| {
            self.source
                .enumerate_values()
                .map(|vals| vals.into_iter().filter(|v| (self.predicate)(v)).collect())
        })
    }
}

impl<T, F, G> Generator<T> for Filtered<T, F, G>
where
    T: Clone + Send + Sync,
    G: Generator<T>,
    F: Fn(&T) -> bool + Send + Sync,
{
    fn do_draw(&self, tc: &TestCase) -> T {
        if let Some(valid) = self.enumerated() {
            if valid.is_empty() {
                invalid_argument!(
                    "Unsatisfiable filter: all values from the source generator \
                     are rejected by the filter predicate"
                );
            }
            let index = tc.generate_integer_i64(0, valid.len() as i64 - 1) as usize;
            return valid[index].clone();
        }
        for _ in 0..3 {
            tc.start_span(labels::FILTER);
            let value = self.source.do_draw(tc);
            if (self.predicate)(&value) {
                tc.stop_span(false);
                return value;
            }
            tc.stop_span(true);
        }
        tc.assume(false);
        unreachable!()
    }

    fn enumerate_values(&self) -> Option<Vec<T>> {
        self.enumerated().clone()
    }
}

/// Printing a filtered draw retries exactly like the silent path, but each
/// attempt prints inside a speculative region so a rejected attempt's output
/// is discarded — only the accepted value's representation survives. The
/// enumerated fast path draws only an index, so it prints the chosen value
/// directly; that is why this impl needs `T: PrettyPrintable` on top of the
/// source being printable.
impl<T, F, G> PrintableGenerator<T> for Filtered<T, F, G>
where
    T: Clone + Send + Sync + PrettyPrintable,
    G: PrintableGenerator<T>,
    F: Fn(&T) -> bool + Send + Sync,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        if let Some(valid) = self.enumerated() {
            if valid.is_empty() {
                invalid_argument!(
                    "Unsatisfiable filter: all values from the source generator \
                     are rejected by the filter predicate"
                );
            }
            let index = tc.generate_integer_i64(0, valid.len() as i64 - 1) as usize;
            let value = valid[index].clone();
            value.pretty_print(printer);
            return value;
        }
        for _ in 0..3 {
            tc.start_span(labels::FILTER);
            let mut speculation = printer.speculate();
            let value = self.source.do_draw_and_print(tc, speculation.printer());
            if (self.predicate)(&value) {
                speculation.commit();
                tc.stop_span(false);
                return value;
            }
            speculation.abort();
            tc.stop_span(true);
        }
        tc.assume(false);
        unreachable!()
    }
}

/// A type-erased generator with a lifetime parameter.
pub struct BoxedGenerator<'a, T> {
    pub(super) inner: Arc<dyn Generator<T> + Send + Sync + 'a>,
}

impl<T> Clone for BoxedGenerator<'_, T> {
    fn clone(&self) -> Self {
        BoxedGenerator {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Generator<T> for BoxedGenerator<'_, T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        self.inner.do_draw(tc)
    }

    fn enumerate_values(&self) -> Option<Vec<T>> {
        self.inner.enumerate_values()
    }

    fn boxed<'b>(self) -> BoxedGenerator<'b, T>
    where
        Self: Sized + Send + Sync + 'b,
    {
        BoxedGenerator { inner: self.inner }
    }
}

/// A type-erased printable generator with a lifetime parameter, as produced
/// by [`PrintableGenerator::boxed_printable`] and consumed by
/// [`one_of!`](crate::one_of).
pub struct BoxedPrintableGenerator<'a, T> {
    inner: Arc<dyn PrintableGenerator<T> + Send + Sync + 'a>,
}

impl<T> Clone for BoxedPrintableGenerator<'_, T> {
    fn clone(&self) -> Self {
        BoxedPrintableGenerator {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Generator<T> for BoxedPrintableGenerator<'_, T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        self.inner.do_draw(tc)
    }

    fn enumerate_values(&self) -> Option<Vec<T>> {
        self.inner.enumerate_values()
    }
}

impl<T> PrintableGenerator<T> for BoxedPrintableGenerator<'_, T> {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        self.inner.do_draw_and_print(tc, printer)
    }

    fn boxed_printable<'b>(self) -> BoxedPrintableGenerator<'b, T>
    where
        Self: Sized + Send + Sync + 'b,
    {
        BoxedPrintableGenerator { inner: self.inner }
    }
}

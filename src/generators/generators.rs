use crate::pretty::{PrettyPrintable, PrettyPrinter};
use crate::test_case::{TestCase, labels};
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
            _phantom: PhantomData,
        }
    }

    /// Convert this generator into a type-erased boxed generator.
    ///
    /// This is needed when you have generators of different concrete types
    /// but the same output type and need to store them together, e.g. in a
    /// `Vec` or when passing to [`one_of()`](super::one_of).
    ///
    /// A `BoxedGenerator` is *not* a [`PrintableGenerator`], even when the
    /// generator it erases is one — box with
    /// [`boxed_printable`](PrintableGenerator::boxed_printable) instead to
    /// keep the result usable with [`draw`](crate::TestCase::draw).
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
    fn print_with<F>(self, print: F) -> PrintedWith<Self, F>
    where
        Self: Sized,
        F: Fn(&T, &mut PrettyPrinter) + Send + Sync,
    {
        PrintedWith {
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
    fn print_as_value(self) -> PrintedAsValue<Self>
    where
        Self: Sized,
        T: PrettyPrintable,
    {
        PrintedAsValue { source: self }
    }

    /// Make this generator printable by printing each drawn value's `Debug`
    /// representation.
    ///
    /// This works for any `Debug` type, so it is the escape hatch for types
    /// the orphan rule keeps out of [`PrettyPrintable`] — standard-library
    /// and third-party types alike. Derived-`Debug` output is re-laid-out
    /// through the printer (see
    /// [`print_debug_repr`](crate::pretty::print_debug_repr)), so large
    /// values wrap like natively printed ones.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators::{self as gs, Generator};
    /// use std::path::PathBuf;
    ///
    /// let paths = gs::text().map(PathBuf::from).print_as_debug();
    /// ```
    fn print_as_debug(self) -> PrintedAsDebug<Self>
    where
        Self: Sized,
        T: std::fmt::Debug,
    {
        PrintedAsDebug { source: self }
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
/// the produced type implements [`PrettyPrintable`]. For everything else
/// there are [`Generator::print_as_value`], [`Generator::print_as_debug`],
/// and [`Generator::print_with`].
///
/// # Contract
///
/// `do_draw_and_print` must draw **exactly** the same choices as
/// [`Generator::do_draw`] — the engine explores with the silent path and
/// replays failures with the printing path, so any divergence makes failures
/// unreplayable. The reliable way to satisfy this is to write the drawing
/// logic once: implement `do_draw_and_print`, and implement
/// [`Generator::do_draw`] as
/// `self.do_draw_and_print(tc, &mut PrettyPrinter::noop())` — the no-op
/// printer discards all output, so both paths run the same body by
/// construction. Guard any work done purely for printing (formatting a
/// value, say) with [`PrettyPrinter::should_print`] to keep the silent path
/// cheap.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot print the values it draws",
    label = "`{Self}` does not implement `PrintableGenerator<{T}>`",
    note = "make it printable with `.print_as_debug()` (any `Debug` value), `.print_as_value()` (any `PrettyPrintable` value), or `.print_with(..)`",
    note = "or draw without reporting the value via `tc.draw_silent(..)`"
)]
pub trait PrintableGenerator<T>: Generator<T> {
    /// Produce a value, printing its representation to `printer` as it is
    /// drawn.
    ///
    /// A compositional implementation draws each inner generator with
    /// [`TestCase::draw_and_print`], which tracks the inner draw as one
    /// explain-annotation region; a generator that merely forwards to an
    /// inner printable generator without printing or drawing anything itself
    /// calls the inner generator's `do_draw_and_print` directly instead, so
    /// the region isn't doubled.
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
pub struct PrintedWith<G, F> {
    source: G,
    print: F,
}

impl<T, G, F> Generator<T> for PrintedWith<G, F>
where
    G: Generator<T>,
    F: Fn(&T, &mut PrettyPrinter) + Send + Sync,
{
    fn do_draw(&self, tc: &TestCase) -> T {
        self.source.do_draw(tc)
    }
}

impl<T, G, F> PrintableGenerator<T> for PrintedWith<G, F>
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
pub struct PrintedAsValue<G> {
    source: G,
}

impl<T, G> Generator<T> for PrintedAsValue<G>
where
    G: Generator<T>,
    T: PrettyPrintable,
{
    fn do_draw(&self, tc: &TestCase) -> T {
        self.source.do_draw(tc)
    }
}

impl<T, G> PrintableGenerator<T> for PrintedAsValue<G>
where
    G: Generator<T>,
    T: PrettyPrintable,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        draw_and_print_value(&self.source, tc, printer)
    }
}

/// Result of [`Generator::print_as_debug`].
pub struct PrintedAsDebug<G> {
    source: G,
}

impl<T, G> Generator<T> for PrintedAsDebug<G>
where
    G: Generator<T>,
    T: std::fmt::Debug,
{
    fn do_draw(&self, tc: &TestCase) -> T {
        self.source.do_draw(tc)
    }
}

impl<T, G> PrintableGenerator<T> for PrintedAsDebug<G>
where
    G: Generator<T>,
    T: std::fmt::Debug,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        let value = self.source.do_draw(tc);
        if printer.should_print() {
            crate::pretty::print_debug_repr(&format!("{value:?}"), printer);
        }
        value
    }
}

impl<T, G: Generator<T>> Generator<T> for &G {
    fn do_draw(&self, tc: &TestCase) -> T {
        (*self).do_draw(tc)
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

impl<T, U, G2, F, G1> FlatMapped<T, U, G2, F, G1>
where
    G1: Generator<T>,
    F: Fn(T) -> G2 + Send + Sync,
{
    /// The one flat-map body both draw paths run; only how the derived
    /// generator is drawn (silently or printing) is injected.
    fn draw_flat_mapped(&self, tc: &TestCase, draw_next: impl FnOnce(G2, &TestCase) -> U) -> U {
        tc.start_span(labels::FLAT_MAP);
        let intermediate = self.source.do_draw(tc);
        let next_gen = (self.f)(intermediate);
        let result = draw_next(next_gen, tc);
        tc.stop_span(false);
        result
    }
}

impl<T, U, G2, F, G1> Generator<U> for FlatMapped<T, U, G2, F, G1>
where
    G1: Generator<T>,
    G2: Generator<U>,
    F: Fn(T) -> G2 + Send + Sync,
{
    fn do_draw(&self, tc: &TestCase) -> U {
        self.draw_flat_mapped(tc, |next_gen, tc| next_gen.do_draw(tc))
    }
}

impl<T, U, G2, F, G1> PrintableGenerator<U> for FlatMapped<T, U, G2, F, G1>
where
    G1: Generator<T>,
    G2: PrintableGenerator<U>,
    F: Fn(T) -> G2 + Send + Sync,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> U {
        self.draw_flat_mapped(tc, |next_gen, tc| tc.draw_and_print(next_gen, printer))
    }
}

/// Result of [`Generator::filter`].
pub struct Filtered<T, F, G> {
    source: G,
    predicate: F,
    _phantom: PhantomData<fn() -> T>,
}

impl<T, F, G> Filtered<T, F, G>
where
    F: Fn(&T) -> bool + Send + Sync,
{
    /// The one filtering loop both draw paths run: each attempt draws
    /// inside a speculative print region, so a rejected attempt discards
    /// whatever the injected `draw` printed — only the accepted value's
    /// representation survives. The silent path passes the no-op printer
    /// and a print-free `draw`.
    fn draw_filtered(
        &self,
        tc: &TestCase,
        printer: &mut PrettyPrinter,
        draw: impl Fn(&G, &TestCase, &mut PrettyPrinter) -> T,
    ) -> T {
        for _ in 0..3 {
            tc.start_span(labels::FILTER);
            let mut speculation = printer.speculate();
            let value = draw(&self.source, tc, speculation.printer());
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

impl<T, F, G> Generator<T> for Filtered<T, F, G>
where
    G: Generator<T>,
    F: Fn(&T) -> bool + Send + Sync,
{
    fn do_draw(&self, tc: &TestCase) -> T {
        self.draw_filtered(tc, &mut PrettyPrinter::noop(), |source, tc, _| {
            source.do_draw(tc)
        })
    }
}

impl<T, F, G> PrintableGenerator<T> for Filtered<T, F, G>
where
    G: PrintableGenerator<T>,
    F: Fn(&T) -> bool + Send + Sync,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        self.draw_filtered(tc, printer, |source, tc, printer| {
            tc.draw_and_print(source, printer)
        })
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

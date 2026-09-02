use super::generators::draw_and_print_value;
use super::{BoxedPrintableGenerator, Generator, PrintableGenerator, TestCase, integers, labels};
use crate::pretty::{PrettyPrintable, PrettyPrinter};
use crate::test_case::invalid_argument;
use std::borrow::Cow;
use std::marker::PhantomData;

/// Generator that picks from a fixed list of values. Created by [`sampled_from()`].
pub struct SampledFromGenerator<'a, T: Clone> {
    elements: Cow<'a, [T]>,
}

impl<'a, T: Clone + Send + Sync + 'a> Generator<T> for SampledFromGenerator<'a, T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        let indices = integers::<usize>()
            .min_value(0)
            .max_value(self.elements.len() - 1);
        let index = indices.do_draw(tc);
        self.elements[index].clone()
    }
}

impl<'a, T: Clone + Send + Sync + PrettyPrintable + 'a> PrintableGenerator<T>
    for SampledFromGenerator<'a, T>
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        draw_and_print_value(self, tc, printer)
    }
}

/// Pick from a fixed list of values.
///
/// Accepts anything convertible into `Cow<[T]>`, including:
/// - `Vec<T>` (consumed without re-allocation)
/// - `&[T]` where `T: Clone` (borrowed, zero allocation)
/// - `&Vec<T>` or `&[T; N]` (via coercion to `&[T]`)
///
/// Panics if `elements` is empty.
pub fn sampled_from<'a, T, S>(elements: S) -> SampledFromGenerator<'a, T>
where
    T: Clone + Send + Sync,
    S: Into<Cow<'a, [T]>>,
{
    let elements = elements.into();
    if elements.is_empty() {
        invalid_argument!("Collection passed to sampled_from cannot be empty");
    }
    SampledFromGenerator { elements }
}

/// Generator that chooses between alternatives of the same type. Created by
/// the [`one_of!`](crate::one_of) macro (which keeps its components unboxed)
/// and the [`one_of()`] function (which stores boxed generators).
///
/// Generic over the alternative storage `A`: a [`OneOfCons`]/[`OneOfLast`]
/// chain from `one_of!`, or a `Vec` of boxed generators from `one_of` —
/// [`BoxedPrintableGenerator`](super::BoxedPrintableGenerator)s (the default)
/// for a printable result, plain [`BoxedGenerator`](super::BoxedGenerator)s
/// for one that can only be drawn silently.
pub struct OneOfGenerator<'a, T, A = Vec<BoxedPrintableGenerator<'a, T>>> {
    alternatives: A,
    _phantom: PhantomData<&'a fn() -> T>,
}

/// The alternatives a [`OneOfGenerator`] dispatches between: some fixed
/// number of generators of the same type, drawable by index.
pub trait Alternatives<T> {
    /// The highest valid alternative index. Alternatives are never empty, so
    /// this is the number of alternatives minus one.
    fn max_index(&self) -> usize;
    /// Draw from the alternative at `index`, which is at most
    /// [`max_index()`](Self::max_index).
    fn draw_at(&self, tc: &TestCase, index: usize) -> T;
}

/// An [`Alternatives`] whose components are all [`PrintableGenerator`]s,
/// making the containing [`OneOfGenerator`] one too.
pub trait PrintableAlternatives<T>: Alternatives<T> {
    /// Draw from the alternative at `index`, printing the drawn value.
    fn draw_at_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter, index: usize) -> T;
}

/// The choice structure every `one_of` draw shares — a ONE_OF span around a
/// uniform index draw followed by the chosen alternative — with the
/// alternative dispatch (and whether it draws silently or printing)
/// injected. Using this from both draw paths is what keeps their choice
/// streams identical.
fn draw_one_of<T>(tc: &TestCase, max_index: usize, draw_at: impl FnOnce(usize) -> T) -> T {
    tc.start_span(labels::ONE_OF);
    let index = integers::<usize>()
        .min_value(0)
        .max_value(max_index)
        .do_draw(tc);
    let result = draw_at(index);
    tc.stop_span(false);
    result
}

impl<'a, T, A: Alternatives<T>> Generator<T> for OneOfGenerator<'a, T, A> {
    fn do_draw(&self, tc: &TestCase) -> T {
        draw_one_of(tc, self.alternatives.max_index(), |index| {
            self.alternatives.draw_at(tc, index)
        })
    }
}

impl<'a, T, A: PrintableAlternatives<T>> PrintableGenerator<T> for OneOfGenerator<'a, T, A> {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        draw_one_of(tc, self.alternatives.max_index(), |index| {
            self.alternatives.draw_at_and_print(tc, printer, index)
        })
    }
}

impl<T, B: Generator<T>> Alternatives<T> for Vec<B> {
    fn max_index(&self) -> usize {
        self.len() - 1
    }
    fn draw_at(&self, tc: &TestCase, index: usize) -> T {
        self[index].do_draw(tc)
    }
}

impl<T, B: PrintableGenerator<T>> PrintableAlternatives<T> for Vec<B> {
    fn draw_at_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter, index: usize) -> T {
        tc.draw_and_print(&self[index], printer)
    }
}

/// A non-final `one_of!` alternative: one generator plus the rest of the
/// chain.
pub struct OneOfCons<G, R>(pub G, pub R);

/// The final `one_of!` alternative.
pub struct OneOfLast<G>(pub G);

impl<T, G: Generator<T>> Alternatives<T> for OneOfLast<G> {
    fn max_index(&self) -> usize {
        0
    }
    fn draw_at(&self, tc: &TestCase, _index: usize) -> T {
        self.0.do_draw(tc)
    }
}

impl<T, G: PrintableGenerator<T>> PrintableAlternatives<T> for OneOfLast<G> {
    fn draw_at_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter, _index: usize) -> T {
        tc.draw_and_print(&self.0, printer)
    }
}

impl<T, G: Generator<T>, R: Alternatives<T>> Alternatives<T> for OneOfCons<G, R> {
    fn max_index(&self) -> usize {
        1 + self.1.max_index()
    }
    fn draw_at(&self, tc: &TestCase, index: usize) -> T {
        if index == 0 {
            self.0.do_draw(tc)
        } else {
            self.1.draw_at(tc, index - 1)
        }
    }
}

impl<T, G: PrintableGenerator<T>, R: PrintableAlternatives<T>> PrintableAlternatives<T>
    for OneOfCons<G, R>
{
    fn draw_at_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter, index: usize) -> T {
        if index == 0 {
            tc.draw_and_print(&self.0, printer)
        } else {
            self.1.draw_at_and_print(tc, printer, index - 1)
        }
    }
}

/// Choose from multiple generators of the same type.
///
/// Accepts any iterable of boxed generators — `Vec<BoxedPrintableGenerator<T>>`
/// for a printable result, or `Vec<BoxedGenerator<T>>` for a silent one. For a
/// more convenient syntax, use the `one_of!` macro instead.
pub fn one_of<'a, T, B, I>(generators: I) -> OneOfGenerator<'a, T, Vec<B>>
where
    B: Generator<T>,
    I: IntoIterator<Item = B>,
{
    let generators: Vec<B> = generators.into_iter().collect();
    if generators.is_empty() {
        invalid_argument!("one_of requires at least one generator");
    }
    OneOfGenerator {
        alternatives: generators,
        _phantom: PhantomData,
    }
}

#[doc(hidden)]
pub fn one_of_from_alternatives<'a, T, A: Alternatives<T>>(
    alternatives: A,
) -> OneOfGenerator<'a, T, A> {
    OneOfGenerator {
        alternatives,
        _phantom: PhantomData,
    }
}

/// Choose from any number of generators of the same type.
///
/// The component generators keep their concrete types (no boxing), so the
/// result is a [`PrintableGenerator`] exactly when every component is one —
/// usable with [`draw`](crate::TestCase::draw) in that case, and with
/// [`draw_silent`](crate::TestCase::draw_silent) otherwise. When the number
/// of alternatives isn't known at compile time, box the generators and call
/// [`one_of`](crate::generators::one_of) instead.
///
/// # Example
///
/// ```no_run
/// use hegel::generators as gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let value: i32 = tc.draw(hegel::one_of!(
///         gs::integers::<i32>().min_value(0).max_value(10),
///         gs::integers::<i32>().min_value(100).max_value(110),
///     ));
/// }
/// ```
#[macro_export]
macro_rules! one_of {
    ($($g:expr),+ $(,)?) => {
        $crate::generators::one_of_from_alternatives($crate::__one_of_alternatives!($($g),+))
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __one_of_alternatives {
    ($g:expr) => {
        $crate::generators::OneOfLast($g)
    };
    ($g:expr, $($rest:expr),+) => {
        $crate::generators::OneOfCons($g, $crate::__one_of_alternatives!($($rest),+))
    };
}

/// Generator that produces `Some(value)` or `None`. Created by [`optional()`].
pub struct OptionalGenerator<G, T> {
    inner: G,
    _phantom: PhantomData<fn(T)>,
}

impl<T, G> OptionalGenerator<G, T> {
    /// The one optional body both draw paths run; only how the inner value
    /// is drawn (silently or printing) is injected.
    fn draw_optional(
        &self,
        tc: &TestCase,
        printer: &mut PrettyPrinter,
        draw: impl FnOnce(&G, &TestCase, &mut PrettyPrinter) -> T,
    ) -> Option<T> {
        tc.start_span(labels::OPTIONAL);
        let result = if tc.generate_boolean(0.5) {
            printer.begin_group(5, "Some(");
            let value = draw(&self.inner, tc, printer);
            printer.end_group(")");
            Some(value)
        } else {
            printer.text("None");
            None
        };
        tc.stop_span(false);
        result
    }
}

impl<T, G> Generator<Option<T>> for OptionalGenerator<G, T>
where
    G: Generator<T>,
{
    fn do_draw(&self, tc: &TestCase) -> Option<T> {
        self.draw_optional(tc, &mut PrettyPrinter::noop(), |inner, tc, _| {
            inner.do_draw(tc)
        })
    }
}

impl<T, G> PrintableGenerator<Option<T>> for OptionalGenerator<G, T>
where
    G: PrintableGenerator<T>,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> Option<T> {
        self.draw_optional(tc, printer, |inner, tc, printer| {
            tc.draw_and_print(inner, printer)
        })
    }
}

/// Generate `Option<T>` values: either `Some(value)` from the inner generator, or `None`.
pub fn optional<T, G: Generator<T>>(inner: G) -> OptionalGenerator<G, T> {
    OptionalGenerator {
        inner,
        _phantom: PhantomData,
    }
}

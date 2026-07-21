use super::generators::draw_and_print_value;
use super::{BoxedGenerator, Generator, PrintableGenerator, TestCase, integers, labels};
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

/// Generator that chooses from a runtime collection of boxed generators.
/// Created by [`one_of()`]; the [`one_of!`](crate::one_of) macro instead
/// builds an arity-specific generator that keeps its components unboxed.
///
/// Generic over the stored generator type `B`: built from
/// [`BoxedPrintableGenerator`](super::BoxedPrintableGenerator)s it is itself
/// printable; built from plain [`BoxedGenerator`](super::BoxedGenerator)s it
/// can only be drawn silently.
pub struct OneOfGenerator<'a, T, B = BoxedGenerator<'a, T>> {
    generators: Vec<B>,
    _phantom: PhantomData<fn(&'a ()) -> T>,
}

/// The choice structure every `one_of` form shares — a ONE_OF span around a
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

impl<'a, T, B: Generator<T>> Generator<T> for OneOfGenerator<'a, T, B> {
    fn do_draw(&self, tc: &TestCase) -> T {
        draw_one_of(tc, self.generators.len() - 1, |index| {
            self.generators[index].do_draw(tc)
        })
    }
}

impl<'a, T, B: PrintableGenerator<T>> PrintableGenerator<T> for OneOfGenerator<'a, T, B> {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        draw_one_of(tc, self.generators.len() - 1, |index| {
            self.generators[index].draw_and_print(tc, printer)
        })
    }
}

/// Choose from multiple generators of the same type.
///
/// Accepts any iterable of boxed generators — `Vec<BoxedPrintableGenerator<T>>`
/// for a printable result, or `Vec<BoxedGenerator<T>>` for a silent one. For a
/// more convenient syntax, use the `one_of!` macro instead.
pub fn one_of<'a, T, B, I>(generators: I) -> OneOfGenerator<'a, T, B>
where
    B: Generator<T>,
    I: IntoIterator<Item = B>,
{
    let generators: Vec<B> = generators.into_iter().collect();
    if generators.is_empty() {
        invalid_argument!("one_of requires at least one generator");
    }
    OneOfGenerator {
        generators,
        _phantom: PhantomData,
    }
}

/// Choose from 1–12 generators of the same type.
///
/// The component generators keep their concrete types (no boxing), so the
/// result is a [`PrintableGenerator`] exactly when every component is one —
/// usable with [`draw`](crate::TestCase::draw) in that case, and with
/// [`draw_silent`](crate::TestCase::draw_silent) otherwise. For more than 12
/// alternatives, or a number not known at compile time, box the generators
/// and call [`one_of`] directly.
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
    ($g1:expr $(,)?) => {
        $crate::generators::one_of1($g1)
    };
    ($g1:expr, $g2:expr $(,)?) => {
        $crate::generators::one_of2($g1, $g2)
    };
    ($g1:expr, $g2:expr, $g3:expr $(,)?) => {
        $crate::generators::one_of3($g1, $g2, $g3)
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr $(,)?) => {
        $crate::generators::one_of4($g1, $g2, $g3, $g4)
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr $(,)?) => {
        $crate::generators::one_of5($g1, $g2, $g3, $g4, $g5)
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr $(,)?) => {
        $crate::generators::one_of6($g1, $g2, $g3, $g4, $g5, $g6)
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr $(,)?) => {
        $crate::generators::one_of7($g1, $g2, $g3, $g4, $g5, $g6, $g7)
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr $(,)?) => {
        $crate::generators::one_of8($g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8)
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr $(,)?) => {
        $crate::generators::one_of9($g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9)
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr $(,)?) => {
        $crate::generators::one_of10($g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10)
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr $(,)?) => {
        $crate::generators::one_of11($g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11)
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr $(,)?) => {
        $crate::generators::one_of12(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $($rest:expr),+ $(,)?) => {
        compile_error!(
            "one_of! supports at most 12 generators; for more, box them and call \
             hegel::generators::one_of directly (e.g. \
             one_of(vec![g1.boxed_printable(), g2.boxed_printable(), ...]))"
        )
    };
}

/// Generator choosing from a single alternative. Created by
/// [`one_of!`](crate::one_of); the 2–12 alternative forms are the
/// macro-generated `OneOf2Generator` … `OneOf12Generator`.
pub struct OneOf1Generator<G1, T> {
    gen1: G1,
    _phantom: PhantomData<fn(T)>,
}

impl<T, G1> Generator<T> for OneOf1Generator<G1, T>
where
    G1: Generator<T>,
{
    fn do_draw(&self, tc: &TestCase) -> T {
        draw_one_of(tc, 0, |_| self.gen1.do_draw(tc))
    }
}

impl<T, G1> PrintableGenerator<T> for OneOf1Generator<G1, T>
where
    G1: PrintableGenerator<T>,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        draw_one_of(tc, 0, |_| self.gen1.draw_and_print(tc, printer))
    }
}

#[doc(hidden)]
pub fn one_of1<T, G1: Generator<T>>(gen1: G1) -> OneOf1Generator<G1, T> {
    OneOf1Generator {
        gen1,
        _phantom: PhantomData,
    }
}

macro_rules! impl_one_of {
    ($name:ident, $fn_name:ident, $max:expr,
     $(($idx:tt, $field:ident, $G:ident)),+ ; ($last_field:ident, $last_G:ident)) => {
        /// Generator choosing uniformly among its component generators.
        /// Created by [`one_of!`](crate::one_of); a
        /// [`PrintableGenerator`] exactly when every component is one.
        pub struct $name<$($G,)+ $last_G, T> {
            $($field: $G,)+
            $last_field: $last_G,
            _phantom: PhantomData<fn(T)>,
        }

        impl<T, $($G,)+ $last_G> Generator<T> for $name<$($G,)+ $last_G, T>
        where
            $($G: Generator<T>,)+
            $last_G: Generator<T>,
        {
            fn do_draw(&self, tc: &TestCase) -> T {
                draw_one_of(tc, $max, |index| match index {
                    $($idx => self.$field.do_draw(tc),)+
                    _ => self.$last_field.do_draw(tc),
                })
            }
        }

        impl<T, $($G,)+ $last_G> PrintableGenerator<T> for $name<$($G,)+ $last_G, T>
        where
            $($G: PrintableGenerator<T>,)+
            $last_G: PrintableGenerator<T>,
        {
            fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
                draw_one_of(tc, $max, |index| match index {
                    $($idx => self.$field.draw_and_print(tc, printer),)+
                    _ => self.$last_field.draw_and_print(tc, printer),
                })
            }
        }

        #[doc(hidden)]
        #[allow(clippy::too_many_arguments)]
        pub fn $fn_name<T, $($G: Generator<T>,)+ $last_G: Generator<T>>(
            $($field: $G,)+ $last_field: $last_G,
        ) -> $name<$($G,)+ $last_G, T> {
            $name {
                $($field,)+
                $last_field,
                _phantom: PhantomData,
            }
        }
    };
}

impl_one_of!(OneOf2Generator, one_of2, 1, (0, gen1, G1); (gen2, G2));
impl_one_of!(
    OneOf3Generator,
    one_of3,
    2,
    (0, gen1, G1),
    (1, gen2, G2);
    (gen3, G3)
);
impl_one_of!(
    OneOf4Generator,
    one_of4,
    3,
    (0, gen1, G1),
    (1, gen2, G2),
    (2, gen3, G3);
    (gen4, G4)
);
impl_one_of!(
    OneOf5Generator,
    one_of5,
    4,
    (0, gen1, G1),
    (1, gen2, G2),
    (2, gen3, G3),
    (3, gen4, G4);
    (gen5, G5)
);
impl_one_of!(
    OneOf6Generator,
    one_of6,
    5,
    (0, gen1, G1),
    (1, gen2, G2),
    (2, gen3, G3),
    (3, gen4, G4),
    (4, gen5, G5);
    (gen6, G6)
);
impl_one_of!(
    OneOf7Generator,
    one_of7,
    6,
    (0, gen1, G1),
    (1, gen2, G2),
    (2, gen3, G3),
    (3, gen4, G4),
    (4, gen5, G5),
    (5, gen6, G6);
    (gen7, G7)
);
impl_one_of!(
    OneOf8Generator,
    one_of8,
    7,
    (0, gen1, G1),
    (1, gen2, G2),
    (2, gen3, G3),
    (3, gen4, G4),
    (4, gen5, G5),
    (5, gen6, G6),
    (6, gen7, G7);
    (gen8, G8)
);
impl_one_of!(
    OneOf9Generator,
    one_of9,
    8,
    (0, gen1, G1),
    (1, gen2, G2),
    (2, gen3, G3),
    (3, gen4, G4),
    (4, gen5, G5),
    (5, gen6, G6),
    (6, gen7, G7),
    (7, gen8, G8);
    (gen9, G9)
);
impl_one_of!(
    OneOf10Generator,
    one_of10,
    9,
    (0, gen1, G1),
    (1, gen2, G2),
    (2, gen3, G3),
    (3, gen4, G4),
    (4, gen5, G5),
    (5, gen6, G6),
    (6, gen7, G7),
    (7, gen8, G8),
    (8, gen9, G9);
    (gen10, G10)
);
impl_one_of!(
    OneOf11Generator,
    one_of11,
    10,
    (0, gen1, G1),
    (1, gen2, G2),
    (2, gen3, G3),
    (3, gen4, G4),
    (4, gen5, G5),
    (5, gen6, G6),
    (6, gen7, G7),
    (7, gen8, G8),
    (8, gen9, G9),
    (9, gen10, G10);
    (gen11, G11)
);
impl_one_of!(
    OneOf12Generator,
    one_of12,
    11,
    (0, gen1, G1),
    (1, gen2, G2),
    (2, gen3, G3),
    (3, gen4, G4),
    (4, gen5, G5),
    (5, gen6, G6),
    (6, gen7, G7),
    (7, gen8, G8),
    (8, gen9, G9),
    (9, gen10, G10),
    (10, gen11, G11);
    (gen12, G12)
);

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
            printer.end_group(5, ")");
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
            inner.draw_and_print(tc, printer)
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

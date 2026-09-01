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

/// Generator that chooses from a runtime collection of boxed generators.
/// Created by [`one_of()`]; the [`one_of!`](crate::one_of) macro instead
/// builds an arity-specific generator that keeps its components unboxed.
///
/// Generic over the stored generator type `B`: built from
/// [`BoxedPrintableGenerator`](super::BoxedPrintableGenerator)s (the
/// default) it is itself printable; built from plain
/// [`BoxedGenerator`](super::BoxedGenerator)s it can only be drawn
/// silently.
pub struct OneOfGenerator<'a, T, B = BoxedPrintableGenerator<'a, T>> {
    generators: Vec<B>,
    _phantom: PhantomData<&'a fn() -> T>,
}

/// The choice structure every `one_of` form shares — a ONE_OF span around a
/// uniform index draw followed by the chosen alternative — with the
/// alternative dispatch (and whether it draws silently or printing)
/// injected. Using this from both draw paths is what keeps their choice
/// streams identical.
///
/// Public so that generators defined with [`impl_one_of!`](crate::impl_one_of)
/// (and any hand-written `one_of`-shaped generator) share it.
pub fn draw_one_of<T>(tc: &TestCase, max_index: usize, draw_at: impl FnOnce(usize) -> T) -> T {
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
            tc.draw_and_print(&self.generators[index], printer)
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

/// Choose from 1–30 generators of the same type.
///
/// The component generators keep their concrete types (no boxing), so the
/// result is a [`PrintableGenerator`] exactly when every component is one —
/// usable with [`draw`](crate::TestCase::draw) in that case, and with
/// [`draw_silent`](crate::TestCase::draw_silent) otherwise. For more than 30
/// alternatives, or a number not known at compile time, box the generators
/// and call [`one_of`] directly, or define a higher-arity form with
/// [`impl_one_of!`](crate::impl_one_of).
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
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr $(,)?) => {
        $crate::generators::one_of13(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr $(,)?) => {
        $crate::generators::one_of14(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr $(,)?) => {
        $crate::generators::one_of15(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr $(,)?) => {
        $crate::generators::one_of16(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr $(,)?) => {
        $crate::generators::one_of17(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr $(,)?) => {
        $crate::generators::one_of18(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17, $g18,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr, $g19:expr $(,)?) => {
        $crate::generators::one_of19(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17, $g18, $g19,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr, $g19:expr, $g20:expr $(,)?) => {
        $crate::generators::one_of20(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17, $g18, $g19, $g20,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr, $g19:expr, $g20:expr, $g21:expr $(,)?) => {
        $crate::generators::one_of21(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17, $g18, $g19, $g20, $g21,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr, $g19:expr, $g20:expr, $g21:expr, $g22:expr $(,)?) => {
        $crate::generators::one_of22(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17, $g18, $g19, $g20, $g21, $g22,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr, $g19:expr, $g20:expr, $g21:expr, $g22:expr, $g23:expr $(,)?) => {
        $crate::generators::one_of23(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17, $g18, $g19, $g20, $g21, $g22, $g23,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr, $g19:expr, $g20:expr, $g21:expr, $g22:expr, $g23:expr, $g24:expr $(,)?) => {
        $crate::generators::one_of24(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17, $g18, $g19, $g20, $g21, $g22, $g23, $g24,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr, $g19:expr, $g20:expr, $g21:expr, $g22:expr, $g23:expr, $g24:expr, $g25:expr $(,)?) => {
        $crate::generators::one_of25(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17, $g18, $g19, $g20, $g21, $g22, $g23, $g24, $g25,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr, $g19:expr, $g20:expr, $g21:expr, $g22:expr, $g23:expr, $g24:expr, $g25:expr, $g26:expr $(,)?) => {
        $crate::generators::one_of26(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17, $g18, $g19, $g20, $g21, $g22, $g23, $g24, $g25, $g26,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr, $g19:expr, $g20:expr, $g21:expr, $g22:expr, $g23:expr, $g24:expr, $g25:expr, $g26:expr, $g27:expr $(,)?) => {
        $crate::generators::one_of27(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17, $g18, $g19, $g20, $g21, $g22, $g23, $g24, $g25, $g26, $g27,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr, $g19:expr, $g20:expr, $g21:expr, $g22:expr, $g23:expr, $g24:expr, $g25:expr, $g26:expr, $g27:expr, $g28:expr $(,)?) => {
        $crate::generators::one_of28(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17, $g18, $g19, $g20, $g21, $g22, $g23, $g24, $g25, $g26, $g27, $g28,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr, $g19:expr, $g20:expr, $g21:expr, $g22:expr, $g23:expr, $g24:expr, $g25:expr, $g26:expr, $g27:expr, $g28:expr, $g29:expr $(,)?) => {
        $crate::generators::one_of29(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17, $g18, $g19, $g20, $g21, $g22, $g23, $g24, $g25, $g26, $g27, $g28, $g29,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr, $g19:expr, $g20:expr, $g21:expr, $g22:expr, $g23:expr, $g24:expr, $g25:expr, $g26:expr, $g27:expr, $g28:expr, $g29:expr, $g30:expr $(,)?) => {
        $crate::generators::one_of30(
            $g1, $g2, $g3, $g4, $g5, $g6, $g7, $g8, $g9, $g10, $g11, $g12, $g13, $g14, $g15, $g16,
            $g17, $g18, $g19, $g20, $g21, $g22, $g23, $g24, $g25, $g26, $g27, $g28, $g29, $g30,
        )
    };
    ($g1:expr, $g2:expr, $g3:expr, $g4:expr, $g5:expr, $g6:expr, $g7:expr, $g8:expr, $g9:expr, $g10:expr, $g11:expr, $g12:expr, $g13:expr, $g14:expr, $g15:expr, $g16:expr, $g17:expr, $g18:expr, $g19:expr, $g20:expr, $g21:expr, $g22:expr, $g23:expr, $g24:expr, $g25:expr, $g26:expr, $g27:expr, $g28:expr, $g29:expr, $g30:expr, $($rest:expr),+ $(,)?) => {
        compile_error!(
            "one_of! supports at most 30 generators; for more, box them and call \
             hegel::generators::one_of directly (e.g. \
             one_of(vec![g1.boxed_printable(), g2.boxed_printable(), ...])), or define a \
             higher-arity form with hegel::impl_one_of!"
        )
    };
}

/// Define a fixed-arity `one_of` generator.
///
/// [`one_of!`](crate::one_of) dispatches to generators defined by this macro,
/// covering arities 1–30. Invoke it yourself for a grammar with more
/// alternatives (or box the components and use
/// [`one_of`](crate::generators::one_of) instead).
///
/// The input is a struct name, a constructor function name, the arity, one
/// `(index, field, TypeParam)` triple per alternative except the last, then
/// `;` and a `(field, TypeParam)` pair for the last alternative, which backs
/// the dispatch's fallback arm:
///
/// ```no_run
/// use hegel::generators as gs;
///
/// hegel::impl_one_of!(OneOf2Custom, one_of2_custom, 2, (0, gen1, G1); (gen2, G2));
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let value: i32 = tc.draw(one_of2_custom(gs::just(1), gs::just(2)));
/// }
/// ```
#[macro_export]
macro_rules! impl_one_of {
    ($name:ident, $fn_name:ident, $arity:literal,
     $(($idx:tt, $field:ident, $G:ident)),* ; ($last_field:ident, $last_G:ident)) => {
        #[doc = concat!(
            "The ", $arity, "-alternative `one_of` generator, created by [`", stringify!($fn_name),
            "`]; a `PrintableGenerator` exactly when every component is one."
        )]
        pub struct $name<$($G,)* $last_G, T> {
            $($field: $G,)*
            $last_field: $last_G,
            _phantom: ::core::marker::PhantomData<fn(T)>,
        }

        impl<T, $($G,)* $last_G> $crate::generators::Generator<T> for $name<$($G,)* $last_G, T>
        where
            $($G: $crate::generators::Generator<T>,)*
            $last_G: $crate::generators::Generator<T>,
        {
            fn do_draw(&self, tc: &$crate::TestCase) -> T {
                $crate::generators::draw_one_of(tc, $arity - 1, |index| match index {
                    $($idx => $crate::generators::Generator::do_draw(&self.$field, tc),)*
                    _ => $crate::generators::Generator::do_draw(&self.$last_field, tc),
                })
            }
        }

        impl<T, $($G,)* $last_G> $crate::generators::PrintableGenerator<T>
            for $name<$($G,)* $last_G, T>
        where
            $($G: $crate::generators::PrintableGenerator<T>,)*
            $last_G: $crate::generators::PrintableGenerator<T>,
        {
            fn do_draw_and_print(
                &self,
                tc: &$crate::TestCase,
                printer: &mut $crate::PrettyPrinter,
            ) -> T {
                $crate::generators::draw_one_of(tc, $arity - 1, |index| match index {
                    $($idx => tc.draw_and_print(&self.$field, printer),)*
                    _ => tc.draw_and_print(&self.$last_field, printer),
                })
            }
        }

        #[doc = concat!(
            "Create the ", $arity, "-alternative `one_of` generator [`", stringify!($name), "`]."
        )]
        #[allow(clippy::too_many_arguments)]
        pub fn $fn_name<
            T,
            $($G: $crate::generators::Generator<T>,)*
            $last_G: $crate::generators::Generator<T>,
        >(
            $($field: $G,)* $last_field: $last_G,
        ) -> $name<$($G,)* $last_G, T> {
            $name {
                $($field,)*
                $last_field,
                _phantom: ::core::marker::PhantomData,
            }
        }
    };
}

impl_one_of!(OneOf1Generator, one_of1, 1, ; (gen1, G1));
impl_one_of!(OneOf2Generator, one_of2, 2, (0, gen1, G1); (gen2, G2));
impl_one_of!(
    OneOf3Generator,
    one_of3,
    3,
    (0, gen1, G1),
    (1, gen2, G2);
    (gen3, G3)
);
impl_one_of!(
    OneOf4Generator,
    one_of4,
    4,
    (0, gen1, G1),
    (1, gen2, G2),
    (2, gen3, G3);
    (gen4, G4)
);
impl_one_of!(
    OneOf5Generator,
    one_of5,
    5,
    (0, gen1, G1),
    (1, gen2, G2),
    (2, gen3, G3),
    (3, gen4, G4);
    (gen5, G5)
);
impl_one_of!(
    OneOf6Generator,
    one_of6,
    6,
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
    7,
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
    8,
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
    9,
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
    10,
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
    (9, gen10, G10);
    (gen11, G11)
);
impl_one_of!(
    OneOf12Generator,
    one_of12,
    12,
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
impl_one_of!(
    OneOf13Generator,
    one_of13,
    13,
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
    (10, gen11, G11),
    (11, gen12, G12);
    (gen13, G13)
);
impl_one_of!(
    OneOf14Generator,
    one_of14,
    14,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13);
    (gen14, G14)
);
impl_one_of!(
    OneOf15Generator,
    one_of15,
    15,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14);
    (gen15, G15)
);
impl_one_of!(
    OneOf16Generator,
    one_of16,
    16,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15);
    (gen16, G16)
);
impl_one_of!(
    OneOf17Generator,
    one_of17,
    17,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16);
    (gen17, G17)
);
impl_one_of!(
    OneOf18Generator,
    one_of18,
    18,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16),
    (16, gen17, G17);
    (gen18, G18)
);
impl_one_of!(
    OneOf19Generator,
    one_of19,
    19,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16),
    (16, gen17, G17),
    (17, gen18, G18);
    (gen19, G19)
);
impl_one_of!(
    OneOf20Generator,
    one_of20,
    20,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16),
    (16, gen17, G17),
    (17, gen18, G18),
    (18, gen19, G19);
    (gen20, G20)
);
impl_one_of!(
    OneOf21Generator,
    one_of21,
    21,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16),
    (16, gen17, G17),
    (17, gen18, G18),
    (18, gen19, G19),
    (19, gen20, G20);
    (gen21, G21)
);
impl_one_of!(
    OneOf22Generator,
    one_of22,
    22,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16),
    (16, gen17, G17),
    (17, gen18, G18),
    (18, gen19, G19),
    (19, gen20, G20),
    (20, gen21, G21);
    (gen22, G22)
);
impl_one_of!(
    OneOf23Generator,
    one_of23,
    23,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16),
    (16, gen17, G17),
    (17, gen18, G18),
    (18, gen19, G19),
    (19, gen20, G20),
    (20, gen21, G21),
    (21, gen22, G22);
    (gen23, G23)
);
impl_one_of!(
    OneOf24Generator,
    one_of24,
    24,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16),
    (16, gen17, G17),
    (17, gen18, G18),
    (18, gen19, G19),
    (19, gen20, G20),
    (20, gen21, G21),
    (21, gen22, G22),
    (22, gen23, G23);
    (gen24, G24)
);
impl_one_of!(
    OneOf25Generator,
    one_of25,
    25,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16),
    (16, gen17, G17),
    (17, gen18, G18),
    (18, gen19, G19),
    (19, gen20, G20),
    (20, gen21, G21),
    (21, gen22, G22),
    (22, gen23, G23),
    (23, gen24, G24);
    (gen25, G25)
);
impl_one_of!(
    OneOf26Generator,
    one_of26,
    26,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16),
    (16, gen17, G17),
    (17, gen18, G18),
    (18, gen19, G19),
    (19, gen20, G20),
    (20, gen21, G21),
    (21, gen22, G22),
    (22, gen23, G23),
    (23, gen24, G24),
    (24, gen25, G25);
    (gen26, G26)
);
impl_one_of!(
    OneOf27Generator,
    one_of27,
    27,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16),
    (16, gen17, G17),
    (17, gen18, G18),
    (18, gen19, G19),
    (19, gen20, G20),
    (20, gen21, G21),
    (21, gen22, G22),
    (22, gen23, G23),
    (23, gen24, G24),
    (24, gen25, G25),
    (25, gen26, G26);
    (gen27, G27)
);
impl_one_of!(
    OneOf28Generator,
    one_of28,
    28,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16),
    (16, gen17, G17),
    (17, gen18, G18),
    (18, gen19, G19),
    (19, gen20, G20),
    (20, gen21, G21),
    (21, gen22, G22),
    (22, gen23, G23),
    (23, gen24, G24),
    (24, gen25, G25),
    (25, gen26, G26),
    (26, gen27, G27);
    (gen28, G28)
);
impl_one_of!(
    OneOf29Generator,
    one_of29,
    29,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16),
    (16, gen17, G17),
    (17, gen18, G18),
    (18, gen19, G19),
    (19, gen20, G20),
    (20, gen21, G21),
    (21, gen22, G22),
    (22, gen23, G23),
    (23, gen24, G24),
    (24, gen25, G25),
    (25, gen26, G26),
    (26, gen27, G27),
    (27, gen28, G28);
    (gen29, G29)
);
impl_one_of!(
    OneOf30Generator,
    one_of30,
    30,
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
    (10, gen11, G11),
    (11, gen12, G12),
    (12, gen13, G13),
    (13, gen14, G14),
    (14, gen15, G15),
    (15, gen16, G16),
    (16, gen17, G17),
    (17, gen18, G18),
    (18, gen19, G19),
    (19, gen20, G20),
    (20, gen21, G21),
    (21, gen22, G22),
    (22, gen23, G23),
    (23, gen24, G24),
    (24, gen25, G25),
    (25, gen26, G26),
    (26, gen27, G27),
    (27, gen28, G28),
    (28, gen29, G29);
    (gen30, G30)
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

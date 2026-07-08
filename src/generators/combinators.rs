use super::{BoxedGenerator, Generator, TestCase, integers, labels};
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

    fn enumerate_values(&self) -> Option<Vec<T>> {
        Some(self.elements.to_vec())
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

/// Generator that chooses from multiple generators. Created by [`one_of()`] or [`one_of!`](crate::one_of).
pub struct OneOfGenerator<'a, T> {
    generators: Vec<BoxedGenerator<'a, T>>,
}

impl<T> Generator<T> for OneOfGenerator<'_, T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        tc.start_span(labels::ONE_OF);
        let index = integers::<usize>()
            .min_value(0)
            .max_value(self.generators.len() - 1)
            .do_draw(tc);
        let result = self.generators[index].do_draw(tc);
        tc.stop_span(false);
        result
    }

    fn enumerate_values(&self) -> Option<Vec<T>> {
        let mut all = Vec::new();
        for g in &self.generators {
            all.extend(g.enumerate_values()?);
        }
        Some(all)
    }
}

/// Choose from multiple generators of the same type.
///
/// Accepts any iterable of boxed generators (e.g. `Vec<BoxedGenerator<T>>`
/// or an iterator chain). For a more convenient syntax, use the `one_of!`
/// macro instead.
pub fn one_of<'a, T, I>(generators: I) -> OneOfGenerator<'a, T>
where
    I: IntoIterator<Item = BoxedGenerator<'a, T>>,
{
    let generators: Vec<BoxedGenerator<'a, T>> = generators.into_iter().collect();
    if generators.is_empty() {
        invalid_argument!("one_of requires at least one generator");
    }
    OneOfGenerator { generators }
}

/// Choose from multiple generators of the same type.
///
/// This macro automatically boxes each generator, providing a more ergonomic
/// syntax than calling [`one_of`] directly.
///
/// # Example
///
/// ```no_run
/// use hegel::generators as gs;
///
/// #[hegel::test]
/// fn my_test(tc: &hegel::TestCase) {
///     let value: i32 = tc.draw(hegel::one_of!(
///         gs::integers::<i32>().min_value(0).max_value(10),
///         gs::integers::<i32>().min_value(100).max_value(110),
///     ));
/// }
/// ```
#[macro_export]
macro_rules! one_of {
    ($($generator:expr),+ $(,)?) => {
        $crate::generators::one_of(vec![
            $($crate::generators::Generator::boxed($generator)),+
        ])
    };
}

/// Generator that produces `Some(value)` or `None`. Created by [`optional()`].
pub struct OptionalGenerator<G, T> {
    inner: G,
    _phantom: PhantomData<fn(T)>,
}

impl<T, G> Generator<Option<T>> for OptionalGenerator<G, T>
where
    G: Generator<T>,
{
    fn do_draw(&self, tc: &TestCase) -> Option<T> {
        tc.start_span(labels::OPTIONAL);
        let result = if tc.generate_boolean(0.5) {
            Some(self.inner.do_draw(tc))
        } else {
            None
        };
        tc.stop_span(false);
        result
    }
}

/// Generate `Option<T>` values: either `Some(value)` from the inner generator, or `None`.
pub fn optional<T, G: Generator<T>>(inner: G) -> OptionalGenerator<G, T> {
    OptionalGenerator {
        inner,
        _phantom: PhantomData,
    }
}

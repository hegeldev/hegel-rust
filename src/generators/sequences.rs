use super::generators::draw_and_print_value;
use super::{Collection, Generator, PrintableGenerator, TestCase, fnv1a_hash};
use crate::pretty::{PrettyPrintable, PrettyPrinter};
use crate::test_case::invalid_argument;
use std::borrow::Cow;

const SUBSEQUENCE_LABEL: u64 = fnv1a_hash(b"hegel:subsequence");
const PERMUTATION_LABEL: u64 = fnv1a_hash(b"hegel:permutation");
const SAMPLE_LABEL: u64 = fnv1a_hash(b"hegel:sample");

/// Draw between `min_size` and `max_size` distinct indices in `0..n`, without
/// replacement, in draw order. Shrinks towards fewer indices and towards the
/// earliest not-yet-taken index, so the minimal sample is `0..min_size` in
/// increasing order.
fn draw_index_sample(tc: &TestCase, n: usize, min_size: usize, max_size: usize) -> Vec<usize> {
    let mut remaining: Vec<usize> = (0..n).collect();
    let mut chosen = Vec::new();
    let mut collection = Collection::new(tc, min_size, Some(max_size));
    while !remaining.is_empty() && collection.more() {
        let j = tc.generate_integer_i64(0, remaining.len() as i64 - 1) as usize;
        chosen.push(remaining.remove(j));
    }
    chosen
}

/// Generator that picks a subsequence of a fixed list of values: each drawn
/// `Vec` contains a subset of the elements, in their original order.
/// Created by [`subsequences()`].
pub struct SubsequenceGenerator<'a, T: Clone> {
    elements: Cow<'a, [T]>,
    min_size: usize,
    max_size: Option<usize>,
}

impl<'a, T: Clone> SubsequenceGenerator<'a, T> {
    /// Set the minimum number of elements to include.
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
    }

    /// Set the maximum number of elements to include.
    pub fn max_size(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }
}

impl<'a, T: Clone + Send + Sync + 'a> Generator<Vec<T>> for SubsequenceGenerator<'a, T> {
    fn do_draw(&self, tc: &TestCase) -> Vec<T> {
        let n = self.elements.len();
        if let Some(max) = self.max_size {
            if self.min_size > max {
                invalid_argument!("Cannot have max_size < min_size");
            }
        }
        if self.min_size > n {
            invalid_argument!(
                "Cannot generate a subsequence: min_size {} is larger than the {} elements in the sequence",
                self.min_size,
                n
            );
        }
        let max_size = self.max_size.map_or(n, |m| m.min(n));
        tc.start_span(SUBSEQUENCE_LABEL);
        let mut indices = draw_index_sample(tc, n, self.min_size, max_size);
        tc.stop_span(false);
        indices.sort_unstable();
        indices
            .into_iter()
            .map(|i| self.elements[i].clone())
            .collect()
    }
}

impl<'a, T: Clone + Send + Sync + PrettyPrintable + 'a> PrintableGenerator<Vec<T>>
    for SubsequenceGenerator<'a, T>
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> Vec<T> {
        draw_and_print_value(self, tc, printer)
    }
}

/// Generate subsequences of a fixed list of values.
///
/// Each drawn `Vec` contains a subset of the elements — anywhere from none of
/// them to all of them, unless constrained with
/// [`min_size`](SubsequenceGenerator::min_size) and
/// [`max_size`](SubsequenceGenerator::max_size) — in their original order.
/// Duplicates in the input are treated as distinct elements, so they can
/// appear in the output up to as many times as they appear in the input.
/// Shrinks towards fewer elements, taken from earlier in the list.
///
/// Accepts anything convertible into `Cow<[T]>`, including:
/// - `Vec<T>` (consumed without re-allocation)
/// - `&[T]` where `T: Clone` (borrowed, zero allocation)
/// - `&Vec<T>` or `&[T; N]` (via coercion to `&[T]`)
///
/// # Example
///
/// ```no_run
/// use hegel::generators as gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let sub: Vec<i32> = tc.draw(gs::subsequences(vec![1, 2, 3, 4, 5]));
///     assert!(sub.len() <= 5);
/// }
/// ```
pub fn subsequences<'a, T, S>(elements: S) -> SubsequenceGenerator<'a, T>
where
    T: Clone + Send + Sync,
    S: Into<Cow<'a, [T]>>,
{
    SubsequenceGenerator {
        elements: elements.into(),
        min_size: 0,
        max_size: None,
    }
}

/// Generator that reorders a fixed list of values. Created by
/// [`permutations()`].
pub struct PermutationGenerator<'a, T: Clone> {
    elements: Cow<'a, [T]>,
}

impl<'a, T: Clone + Send + Sync + 'a> Generator<Vec<T>> for PermutationGenerator<'a, T> {
    fn do_draw(&self, tc: &TestCase) -> Vec<T> {
        let n = self.elements.len();
        tc.start_span(PERMUTATION_LABEL);
        let indices = draw_index_sample(tc, n, n, n);
        tc.stop_span(false);
        indices
            .into_iter()
            .map(|i| self.elements[i].clone())
            .collect()
    }
}

impl<'a, T: Clone + Send + Sync + PrettyPrintable + 'a> PrintableGenerator<Vec<T>>
    for PermutationGenerator<'a, T>
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> Vec<T> {
        draw_and_print_value(self, tc, printer)
    }
}

/// Generate permutations of a fixed list of values.
///
/// Each drawn `Vec` contains all of the elements in a randomly chosen order,
/// shrinking towards the original order. To draw a reordered subset of the
/// elements instead, use [`samples()`] with
/// [`without_replacement`](SampleGenerator::without_replacement).
///
/// Accepts anything convertible into `Cow<[T]>`, including:
/// - `Vec<T>` (consumed without re-allocation)
/// - `&[T]` where `T: Clone` (borrowed, zero allocation)
/// - `&Vec<T>` or `&[T; N]` (via coercion to `&[T]`)
///
/// # Example
///
/// ```no_run
/// use hegel::generators as gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let perm: Vec<i32> = tc.draw(gs::permutations(vec![1, 2, 3, 4, 5]));
///     assert_eq!(perm.len(), 5);
/// }
/// ```
pub fn permutations<'a, T, S>(elements: S) -> PermutationGenerator<'a, T>
where
    T: Clone + Send + Sync,
    S: Into<Cow<'a, [T]>>,
{
    PermutationGenerator {
        elements: elements.into(),
    }
}

/// Generator that samples from a fixed list of values. Created by
/// [`samples()`].
pub struct SampleGenerator<'a, T: Clone> {
    elements: Cow<'a, [T]>,
    min_size: usize,
    max_size: Option<usize>,
    replacement: bool,
}

impl<'a, T: Clone> SampleGenerator<'a, T> {
    /// Set the minimum number of elements to include.
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
    }

    /// Set the maximum number of elements to include. When sampling without
    /// replacement this is additionally capped at the number of elements in
    /// the list.
    pub fn max_size(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }

    /// Sample with replacement: each element of the result is chosen
    /// independently from the full list, so the same value can appear any
    /// number of times. This is the default.
    pub fn with_replacement(mut self) -> Self {
        self.replacement = true;
        self
    }

    /// Sample without replacement: each element of the list is used at most
    /// once, so the result is a reordered subset of the list. Duplicates in
    /// the input are treated as distinct elements, so they can appear in the
    /// output up to as many times as they appear in the input.
    pub fn without_replacement(mut self) -> Self {
        self.replacement = false;
        self
    }
}

impl<'a, T: Clone + Send + Sync + 'a> Generator<Vec<T>> for SampleGenerator<'a, T> {
    fn do_draw(&self, tc: &TestCase) -> Vec<T> {
        let n = self.elements.len();
        if let Some(max) = self.max_size {
            if self.min_size > max {
                invalid_argument!("Cannot have max_size < min_size");
            }
        }
        if self.replacement {
            if n == 0 && self.min_size > 0 {
                invalid_argument!("Cannot generate a non-empty sample from an empty sequence");
            }
            let max_size = if n == 0 { Some(0) } else { self.max_size };
            tc.start_span(SAMPLE_LABEL);
            let mut collection = Collection::new(tc, self.min_size, max_size);
            let mut result = Vec::new();
            while collection.more() {
                let i = tc.generate_integer_i64(0, n as i64 - 1) as usize;
                result.push(self.elements[i].clone());
            }
            tc.stop_span(false);
            result
        } else {
            if self.min_size > n {
                invalid_argument!(
                    "Cannot generate a sample without replacement: min_size {} is larger than the {} elements in the sequence",
                    self.min_size,
                    n
                );
            }
            let max_size = self.max_size.map_or(n, |m| m.min(n));
            tc.start_span(SAMPLE_LABEL);
            let indices = draw_index_sample(tc, n, self.min_size, max_size);
            tc.stop_span(false);
            indices
                .into_iter()
                .map(|i| self.elements[i].clone())
                .collect()
        }
    }
}

impl<'a, T: Clone + Send + Sync + PrettyPrintable + 'a> PrintableGenerator<Vec<T>>
    for SampleGenerator<'a, T>
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> Vec<T> {
        draw_and_print_value(self, tc, printer)
    }
}

/// Generate samples from a fixed list of values.
///
/// By default each drawn `Vec` is a sample **with replacement**: every
/// element is chosen independently from the full list, so the same value can
/// appear any number of times, and the size is unbounded unless constrained
/// with [`min_size`](SampleGenerator::min_size) and
/// [`max_size`](SampleGenerator::max_size). Calling
/// [`without_replacement`](SampleGenerator::without_replacement) switches to
/// sampling **without replacement**: each element of the list is used at most
/// once, so the result is a reordered subset of the list, with at most as
/// many elements as the list itself. Samples shrink towards fewer elements,
/// taken from earlier in the list.
///
/// To draw a single element, use [`sampled_from()`](super::sampled_from); to
/// reorder the whole list, use [`permutations()`]; to pick a subset while
/// preserving the original order, use [`subsequences()`].
///
/// Accepts anything convertible into `Cow<[T]>`, including:
/// - `Vec<T>` (consumed without re-allocation)
/// - `&[T]` where `T: Clone` (borrowed, zero allocation)
/// - `&Vec<T>` or `&[T; N]` (via coercion to `&[T]`)
///
/// # Example
///
/// ```no_run
/// use hegel::generators as gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let with: Vec<i32> = tc.draw(gs::samples(vec![1, 2, 3]).max_size(10));
///     let without: Vec<i32> = tc.draw(gs::samples(vec![1, 2, 3]).without_replacement());
///     assert!(without.len() <= 3);
/// }
/// ```
pub fn samples<'a, T, S>(elements: S) -> SampleGenerator<'a, T>
where
    T: Clone + Send + Sync,
    S: Into<Cow<'a, [T]>>,
{
    SampleGenerator {
        elements: elements.into(),
        min_size: 0,
        max_size: None,
        replacement: true,
    }
}

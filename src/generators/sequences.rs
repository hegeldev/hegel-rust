use super::{Collection, Generator, TestCase, fnv1a_hash};
use crate::test_case::invalid_argument;
use std::borrow::Cow;

const SUBSEQUENCE_LABEL: u64 = fnv1a_hash(b"hegel:subsequence");
const PERMUTATION_LABEL: u64 = fnv1a_hash(b"hegel:permutation");

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
    min_size: Option<usize>,
    max_size: Option<usize>,
}

impl<'a, T: Clone> PermutationGenerator<'a, T> {
    /// Set the minimum number of elements to include. Setting either size
    /// bound makes the generator draw a permutation of a subset of the
    /// elements instead of all of them; the unset bound defaults to 0
    /// (for `min_size`) or the full length (for `max_size`).
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = Some(min_size);
        self
    }

    /// Set the maximum number of elements to include. See
    /// [`min_size`](Self::min_size) for how size bounds change what is
    /// generated.
    pub fn max_size(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }
}

impl<'a, T: Clone + Send + Sync + 'a> Generator<Vec<T>> for PermutationGenerator<'a, T> {
    fn do_draw(&self, tc: &TestCase) -> Vec<T> {
        let n = self.elements.len();
        if let (Some(min), Some(max)) = (self.min_size, self.max_size) {
            if min > max {
                invalid_argument!("Cannot have max_size < min_size");
            }
        }
        let min_size = if self.min_size.is_none() && self.max_size.is_none() {
            n
        } else {
            self.min_size.unwrap_or(0)
        };
        if min_size > n {
            invalid_argument!(
                "Cannot generate a permutation: min_size {} is larger than the {} elements in the sequence",
                min_size,
                n
            );
        }
        let max_size = self.max_size.map_or(n, |m| m.min(n));
        tc.start_span(PERMUTATION_LABEL);
        let indices = draw_index_sample(tc, n, min_size, max_size);
        tc.stop_span(false);
        indices
            .into_iter()
            .map(|i| self.elements[i].clone())
            .collect()
    }
}

/// Generate permutations of a fixed list of values.
///
/// By default each drawn `Vec` contains all of the elements in a randomly
/// chosen order, shrinking towards the original order. Setting
/// [`min_size`](PermutationGenerator::min_size) or
/// [`max_size`](PermutationGenerator::max_size) instead draws an ordered
/// sample without replacement: a permutation of a subset of the elements,
/// with a size in the given bounds, shrinking towards fewer elements, taken
/// from earlier in the list, in their original order.
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
        min_size: None,
        max_size: None,
    }
}

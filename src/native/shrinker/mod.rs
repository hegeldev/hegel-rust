// Shrinker for the native backend.
//
// Ported from pbtkit core.py. Reduces failing test cases to minimal
// counterexamples by systematically simplifying the choice sequence.
//
// Split into submodules:
//   deletion   — delete_chunks, bind_deletion, try_replace_with_deletion
//   integers   — zero_choices, swap_integer_sign, binary_search_integer_towards_zero,
//                redistribute_integers, shrink_duplicates
//   sequence   — sort_values, swap_adjacent_blocks
//   floats     — shrink_floats
//   bytes      — shrink_bytes
//   strings    — shrink_strings

mod bytes;
mod deletion;
mod floats;
mod integers;
mod sequence;
mod strings;
pub mod value_shrinkers;

use std::collections::HashMap;

use crate::native::core::{ChoiceNode, ChoiceValue, MAX_SHRINK_ITERATIONS, NodeSortKey, sort_key};

/// A callback that runs a test case from a choice sequence.
/// Returns `(is_interesting, actual_nodes_consumed)`.
/// `actual_nodes_consumed` is how many ChoiceNodes were produced
/// during the run (may be less than candidate length for flatmap bindings).
pub type TestFn<'a> = dyn FnMut(&[ChoiceNode]) -> (bool, usize) + 'a;

pub struct Shrinker<'a> {
    test_fn: Box<TestFn<'a>>,
    pub current_nodes: Vec<ChoiceNode>,
}

impl<'a> Shrinker<'a> {
    pub fn new(test_fn: Box<TestFn<'a>>, initial_nodes: Vec<ChoiceNode>) -> Self {
        Shrinker {
            test_fn,
            current_nodes: initial_nodes,
        }
    }

    /// Try a candidate choice sequence. If interesting and smaller than
    /// the current best, update current_nodes. Returns whether interesting.
    pub fn consider(&mut self, nodes: &[ChoiceNode]) -> bool {
        if sort_key(nodes) == sort_key(&self.current_nodes) {
            return true;
        }
        let (is_interesting, _) = (self.test_fn)(nodes);
        if is_interesting && sort_key(nodes) < sort_key(&self.current_nodes) {
            self.current_nodes = nodes.to_vec();
        }
        is_interesting
    }

    /// Try replacing values at specific indices.
    pub fn replace(&mut self, values: &HashMap<usize, ChoiceValue>) -> bool {
        let mut attempt: Vec<ChoiceNode> = self.current_nodes.clone();
        for (&i, v) in values {
            assert!(i < attempt.len());
            attempt[i] = attempt[i].with_value(v.clone());
        }
        self.consider(&attempt)
    }

    /// Run all shrink passes repeatedly until no more progress or iteration cap.
    pub fn shrink(&mut self) {
        let mut prev: Vec<NodeSortKey> = Vec::new();
        let mut iterations = 0;

        loop {
            let current_key: Vec<NodeSortKey> =
                self.current_nodes.iter().map(|n| n.sort_key()).collect();
            if current_key == prev || iterations >= MAX_SHRINK_ITERATIONS {
                break;
            }
            prev = current_key;
            iterations += 1;

            self.delete_chunks();
            self.zero_choices();
            self.swap_integer_sign();
            self.binary_search_integer_towards_zero();
            self.bind_deletion();
            self.redistribute_integers();
            self.shrink_duplicates();
            self.sort_values();
            self.swap_adjacent_blocks();
            self.shrink_floats();
            self.shrink_bytes();
            self.shrink_strings();
            self.redistribute_string_pairs();
        }
    }
}

/// Binary search for the smallest value in [lo, hi] where f returns true.
///
/// Assumes f(hi) is true (not checked). Returns lo if f(lo) is true,
/// otherwise finds a locally minimal true value.
pub(super) fn bin_search_down(lo: i128, hi: i128, f: &mut impl FnMut(i128) -> bool) -> i128 {
    if f(lo) {
        return lo;
    }
    let mut lo = lo;
    let mut hi = hi;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if f(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_tests.rs"]
mod tests;

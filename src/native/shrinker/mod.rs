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
mod index_passes;
mod integers;
mod mutation;
mod sequence;
mod strings;
pub mod value_shrinkers;

use std::collections::HashMap;

use crate::native::core::{ChoiceNode, ChoiceValue, MAX_SHRINK_ITERATIONS, NodeSortKey, sort_key};

/// Request passed to the shrinker's test function.
///
/// [`Full`] replays a full node sequence with punning (the shape used by
/// most shrink passes). [`Probe`] replays a prefix of choice values and
/// then draws randomly beyond it — needed by `mutate_and_shrink` (port of
/// pbtkit's `shrinking/mutation.py`).
pub enum ShrinkRun<'a> {
    Full(&'a [ChoiceNode]),
    Probe {
        prefix: &'a [ChoiceValue],
        seed: u64,
        max_size: usize,
    },
}

/// A callback that runs a test case for the shrinker.
/// Returns `(is_interesting, actual_nodes)`.
/// `actual_nodes` is the sequence of ChoiceNodes produced during the run.
/// For [`ShrinkRun::Full`], it may be shorter than the candidate length
/// (for early exit / flatmap bindings), or have different values where the
/// candidate was punned because the kind changed at that position.
pub type TestFn<'a> = dyn FnMut(ShrinkRun) -> (bool, Vec<ChoiceNode>) + 'a;

pub struct Shrinker<'a> {
    test_fn: Box<TestFn<'a>>,
    pub current_nodes: Vec<ChoiceNode>,
}

impl<'a> Shrinker<'a> {
    /// Construct a Shrinker from a `&[ChoiceNode]`-taking closure. Passes
    /// that issue [`ShrinkRun::Probe`] requests (currently only
    /// `mutate_and_shrink`) are no-ops with this constructor: the wrapped
    /// closure ignores the probe and returns `(false, vec![])`. Use
    /// [`Shrinker::with_probe`] when probe support is needed.
    #[cfg(test)]
    pub fn new<F>(mut test_fn: Box<F>, initial_nodes: Vec<ChoiceNode>) -> Self
    where
        F: FnMut(&[ChoiceNode]) -> (bool, Vec<ChoiceNode>) + ?Sized + 'a,
    {
        Shrinker {
            test_fn: Box::new(move |req: ShrinkRun| match req {
                ShrinkRun::Full(nodes) => test_fn(nodes),
                ShrinkRun::Probe { .. } => (false, Vec::new()),
            }),
            current_nodes: initial_nodes,
        }
    }

    /// Construct a Shrinker from a closure that handles both [`ShrinkRun::Full`]
    /// and [`ShrinkRun::Probe`] requests. Required for `mutate_and_shrink` to
    /// actually explore random continuations.
    pub fn with_probe(test_fn: Box<TestFn<'a>>, initial_nodes: Vec<ChoiceNode>) -> Self {
        Shrinker {
            test_fn,
            current_nodes: initial_nodes,
        }
    }

    /// Try a candidate choice sequence. If interesting and smaller than
    /// the current best, update current_nodes. Returns whether interesting.
    ///
    /// The stored nodes are the actual sequence produced by the test
    /// function, not the candidate passed in. This matters when the test
    /// exits early (actual is shorter than candidate) or when value
    /// punning replaces values that no longer fit the kind at that
    /// position after a one_of branch switch.
    pub fn consider(&mut self, nodes: &[ChoiceNode]) -> bool {
        if sort_key(nodes) == sort_key(&self.current_nodes) {
            return true;
        }
        let (is_interesting, actual_nodes) = (self.test_fn)(ShrinkRun::Full(nodes));
        if is_interesting && sort_key(&actual_nodes) < sort_key(&self.current_nodes) {
            self.current_nodes = actual_nodes;
        }
        is_interesting
    }

    /// Run a probe: replay `prefix` then continue with random draws from a
    /// deterministic RNG seeded by `seed`, capped at `max_size` choices. If
    /// the resulting run is interesting and shortlex-smaller than
    /// `current_nodes`, update `current_nodes`.
    ///
    /// Port of pbtkit's `shrinker.test_function(TestCase(prefix=..., random=...))`.
    pub(super) fn probe(&mut self, prefix: &[ChoiceValue], seed: u64, max_size: usize) {
        let (is_interesting, actual_nodes) = (self.test_fn)(ShrinkRun::Probe {
            prefix,
            seed,
            max_size,
        });
        if is_interesting && sort_key(&actual_nodes) < sort_key(&self.current_nodes) {
            self.current_nodes = actual_nodes;
        }
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
            self.lower_and_bump();
            self.try_shortening_via_increment();
            self.mutate_and_shrink();
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

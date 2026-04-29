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

use std::collections::{HashMap, HashSet};

use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, MAX_SHRINK_ITERATIONS, NodeSortKey, sort_key};

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
    /// Count of times `current_nodes` was replaced by a strictly smaller candidate.
    /// Mirrors `engine.py::ConjectureRunner.shrinks` increments inside `test_function`.
    pub improvements: usize,
    /// The choice sequences that were displaced each time `current_nodes` improved.
    /// Used by `shrink_interesting_examples` to downgrade each predecessor to the
    /// secondary key, mirroring `engine.py::downgrade_choices`.
    pub downgraded: Vec<Vec<ChoiceValue>>,
    /// Optional cap on `improvements`.  When `improvements >= max_improvements`,
    /// `consider` and `probe` return `false` without running the test function,
    /// causing the shrinker to stall and exit naturally.
    pub max_improvements: Option<usize>,
    /// Indices of choice nodes that have changed since `clear_change_tracking`.
    /// Used by `lower_common_node_offset` to find candidate nodes to jointly lower.
    /// Mirrors `Shrinker.__all_changed_nodes` / `Shrinker.mark_changed` in Hypothesis.
    changed_nodes: HashSet<usize>,
}

impl<'a> Shrinker<'a> {
    /// Construct a Shrinker from a `&[ChoiceNode]`-taking closure. Passes
    /// that issue [`ShrinkRun::Probe`] requests (currently only
    /// `mutate_and_shrink`) are no-ops with this constructor: the wrapped
    /// closure ignores the probe and returns `(false, vec![])`. Use
    /// [`Shrinker::with_probe`] when probe support is needed.
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
            improvements: 0,
            downgraded: Vec::new(),
            max_improvements: None,
            changed_nodes: HashSet::new(),
        }
    }

    /// Construct a Shrinker from a closure that handles both [`ShrinkRun::Full`]
    /// and [`ShrinkRun::Probe`] requests. Required for `mutate_and_shrink` to
    /// actually explore random continuations.
    pub fn with_probe(test_fn: Box<TestFn<'a>>, initial_nodes: Vec<ChoiceNode>) -> Self {
        Shrinker {
            test_fn,
            current_nodes: initial_nodes,
            improvements: 0,
            downgraded: Vec::new(),
            max_improvements: None,
            changed_nodes: HashSet::new(),
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
        if let Some(max) = self.max_improvements {
            if self.improvements >= max {
                return false;
            }
        }
        let (is_interesting, actual_nodes) = (self.test_fn)(ShrinkRun::Full(nodes));
        if is_interesting && sort_key(&actual_nodes) < sort_key(&self.current_nodes) {
            let old: Vec<ChoiceValue> =
                self.current_nodes.iter().map(|n| n.value.clone()).collect();
            self.downgraded.push(old);
            self.improvements += 1;
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
        if let Some(max) = self.max_improvements {
            if self.improvements >= max {
                return;
            }
        }
        let (is_interesting, actual_nodes) = (self.test_fn)(ShrinkRun::Probe {
            prefix,
            seed,
            max_size,
        });
        if is_interesting && sort_key(&actual_nodes) < sort_key(&self.current_nodes) {
            let old: Vec<ChoiceValue> =
                self.current_nodes.iter().map(|n| n.value.clone()).collect();
            self.downgraded.push(old);
            self.improvements += 1;
            self.current_nodes = actual_nodes;
        }
    }

    /// Try replacing values at specific indices.
    ///
    /// Returns `false` (replacement impossible) if any index is past the end
    /// of `current_nodes`, or if a proposed value's variant doesn't match the
    /// kind variant at that index. Many callers loop across passes that
    /// successively shrink `current_nodes` and pun kinds at fixed positions —
    /// e.g. `bind_deletion` runs `bin_search_down` with a callback that
    /// passes the same captured `i` to `replace` on each probe; the first
    /// probe can shorten the sequence past `i`, or change the kind at `j` so
    /// an Integer value no longer fits the (now Boolean) node. Treating both
    /// as a failed replacement (rather than panicking later in `sort_key`)
    /// matches the semantic invariant: a value that doesn't fit the node's
    /// schema can't be assigned to it.
    pub fn replace(&mut self, values: &HashMap<usize, ChoiceValue>) -> bool {
        let mut attempt: Vec<ChoiceNode> = self.current_nodes.clone();
        for (&i, v) in values {
            if i >= attempt.len() {
                return false;
            }
            if !attempt[i].kind.validate(v) {
                return false;
            }
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
            self.lower_integers_together();
            self.shrink_duplicates();
            self.sort_values();
            self.swap_adjacent_blocks();
            self.shrink_floats();
            self.shrink_bytes();
            self.redistribute_bytes_pairs();
            self.shrink_strings();
            self.redistribute_string_pairs();
            self.lower_and_bump();
            self.try_shortening_via_increment();
            self.mutate_and_shrink();
        }
    }

    /// Mark node index `i` as changed.  Mirrors `Shrinker.mark_changed(i)`.
    /// Used before calling `lower_common_node_offset` to specify which nodes
    /// the caller has already observed changing.
    pub fn mark_changed(&mut self, i: usize) {
        self.changed_nodes.insert(i);
    }

    /// Clear the set of changed nodes.  Mirrors `Shrinker.clear_change_tracking`.
    pub fn clear_change_tracking(&mut self) {
        self.changed_nodes.clear();
    }

    /// Run one named shrink pass.  Used by `NativeShrinker::fixate_shrink_passes`.
    /// Panics on unrecognised pass names.
    pub fn run_named_pass(&mut self, name: &str) {
        match name {
            "minimize_individual_choices" => {
                self.zero_choices();
                self.binary_search_integer_towards_zero();
            }
            "remove_discarded" => {
                // no-op at Shrinker level (span data needed);
                // NativeShrinker.remove_discarded handles the actual logic.
            }
            _ => panic!("unknown shrink pass: {name:?}"),
        }
    }

    /// Jointly lower a common offset across changed integer nodes.
    /// Mirrors `Shrinker.lower_common_node_offset` in Hypothesis.
    ///
    /// Considers every index in `changed_nodes` that holds a non-trivial
    /// non-zero integer, finds the minimum absolute value, and binary-searches
    /// the largest delta `k` in `[0, min_abs]` such that replacing each node's
    /// value with `value - k` (or `value + k`) still satisfies the test.
    pub fn lower_common_node_offset(&mut self) {
        if self.changed_nodes.len() <= 1 {
            return;
        }

        // Collect non-trivial integer nodes from changed indices.
        let mut changed: Vec<(usize, i128)> = Vec::new();
        for &i in &self.changed_nodes.clone() {
            if i >= self.current_nodes.len() {
                continue;
            }
            let node = &self.current_nodes[i];
            match (&node.kind, &node.value) {
                (ChoiceKind::Integer(_), ChoiceValue::Integer(v)) if *v != 0 => {
                    changed.push((i, *v));
                }
                _ => {}
            }
        }

        if changed.is_empty() {
            return;
        }

        let offset = changed.iter().map(|(_, v)| v.unsigned_abs()).min().unwrap();
        if offset == 0 {
            return;
        }

        // Find the maximum k in [0, offset] such that lowering each value by k
        // is still interesting. `find_integer` returns the largest k where f(k)=true.
        let changed_clone = changed.clone();
        find_integer(|k| {
            if k == 0 {
                return true;
            }
            let k = k as i128;
            let replacements: HashMap<usize, ChoiceValue> = changed_clone
                .iter()
                .map(|(i, v)| {
                    let new_val = if *v > 0 { v - k } else { v + k };
                    (*i, ChoiceValue::Integer(new_val))
                })
                .collect();
            self.replace(&replacements)
        });

        self.changed_nodes.clear();
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
    // `lo + 1` overflows when `lo == i128::MAX`. The float shrinker can
    // reach that bound by saturating-casting `f64::MAX as i128` from a
    // generator with `min_value(f64::MAX)`. The search range is
    // degenerate in that case (since `hi >= lo`, both must equal
    // `i128::MAX`), so bail with `hi`.
    while lo.checked_add(1).is_some_and(|n| n < hi) {
        let mid = lo + (hi - lo) / 2;
        if f(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

/// Finds a (hopefully large) integer `n >= 0` such that `f(n)` is true and
/// `f(n+1)` is false. `f(0)` is assumed to be true and is not checked.
///
/// Port of Hypothesis's `junkdrawer.find_integer`. Used by shrink passes that
/// want to maximise a step size — e.g. "lower both nodes by k" needs the
/// largest k for which the joint replacement is still interesting.
///
/// Uses `checked_mul` on the exponential probe and `lo + (hi - lo) / 2` on
/// the binary-search midpoint: in Python this is arbitrary-precision, but in
/// Rust a predicate that accepts an unbounded range (e.g. a `lower_integers_together`
/// pass over full-range `i128` nodes) would otherwise walk `hi` off the end
/// of `usize`.
pub(super) fn find_integer(mut f: impl FnMut(usize) -> bool) -> usize {
    for i in 1..5 {
        if !f(i) {
            return i - 1;
        }
    }
    let mut lo = 4;
    let mut hi = 5;
    while f(hi) {
        lo = hi;
        let Some(next) = hi.checked_mul(2) else {
            return lo;
        };
        hi = next;
    }
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if f(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_tests.rs"]
mod tests;

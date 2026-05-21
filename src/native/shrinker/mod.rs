// Shrinker for the native backend.
//
// Ported from Hypothesis. Reduces failing test cases to minimal
// counterexamples by systematically simplifying the choice sequence.
//
// Split into submodules:
//   deletion   — delete_chunks, bind_deletion, try_replace_with_deletion
//   integers   — zero_choices, swap_integer_sign, binary_search_integer_towards_zero,
//                redistribute_integers, shrink_duplicates
//   sequence   — sort_values, swap_adjacent_blocks
//   floats     — shrink_floats, redistribute_numeric_pairs
//   bytes      — shrink_bytes, redistribute_bytes_pairs
//   strings    — shrink_strings

mod bytes;
mod coarse;
mod deletion;
mod floats;
mod index_passes;
mod integers;
mod mutation;
mod ordering;
mod scheduling;
mod sequence;
mod spans;
mod strings;

pub use scheduling::ShrinkPass;

use std::collections::{HashMap, HashSet, VecDeque};

use crate::native::core::{ChoiceNode, ChoiceValue, MAX_SHRINKS, NodeSortKey, Spans, sort_key};

/// Request passed to the shrinker's test function.
///
/// [`ShrinkRun::Full`] replays a full node sequence with punning (the shape used by
/// most shrink passes). [`ShrinkRun::Probe`] replays a prefix of choice values and
/// then draws randomly beyond it — needed by `mutate_and_shrink` (port of
/// Hypothesis's `shrinking/mutation.py`).
pub enum ShrinkRun<'a> {
    Full(&'a [ChoiceNode]),
    Probe {
        prefix: &'a [ChoiceValue],
        seed: u64,
        max_size: usize,
    },
}

/// A callback that runs a test case for the shrinker.
/// Returns `(is_interesting, actual_nodes, actual_spans)`.
/// `actual_nodes` is the sequence of ChoiceNodes produced during the run.
/// For [`ShrinkRun::Full`], it may be shorter than the candidate length
/// (for early exit / flatmap bindings), or have different values where the
/// candidate was punned because the kind changed at that position.
/// `actual_spans` is the span tree recorded by the same run.
pub type TestFn<'a> = dyn FnMut(ShrinkRun) -> (bool, Vec<ChoiceNode>, Spans) + 'a;

pub struct Shrinker<'a> {
    test_fn: Box<TestFn<'a>>,
    pub current_nodes: Vec<ChoiceNode>,
    /// Spans recorded by the run that produced `current_nodes`.  Updated whenever
    /// `consider` accepts a smaller candidate so span-aware passes (try_trivial_spans,
    /// pass_to_descendant, reorder_spans, remove_discarded) can interrogate the
    /// current shrink target's structure.
    pub current_spans: Spans,
    /// Count of times `current_nodes` was replaced by a strictly smaller candidate.
    /// Mirrors `engine.py::ConjectureRunner.shrinks` increments inside `test_function`.
    pub improvements: usize,
    /// The choice sequences that were displaced each time `current_nodes` improved.
    /// Used by `shrink_interesting_examples` to downgrade each predecessor to the
    /// secondary key, mirroring `engine.py::downgrade_choices`.
    pub downgraded: Vec<Vec<ChoiceValue>>,
    /// Cap on `improvements`.  Once `improvements >= max_improvements`,
    /// `consider` and `probe` short-circuit so the runner doesn't get
    /// stuck chasing diminishing returns.  Defaults to [`MAX_SHRINKS`]
    /// (mirrors Hypothesis's hard cap in
    /// `internal/conjecture/engine.py`); tests can lower it for
    /// controlled-budget assertions.
    pub max_improvements: usize,
    /// Total number of times the test closure has been invoked through
    /// `consider` or `probe`.  Used together with `calls_at_last_shrink`
    /// + `max_stall` to detect runaway shrink searches.
    pub calls: usize,
    /// Value of `calls` at the moment of the most recent
    /// `accept_improvement`.  See `max_stall`.
    pub calls_at_last_shrink: usize,
    /// Once `calls - calls_at_last_shrink >= max_stall`, further
    /// `consider` / `probe` invocations short-circuit.  Mirrors
    /// Hypothesis's `shrinker.py:333-340, 387, 1139-1141`: starts at
    /// 200, grows on every successful shrink by `max(max_stall,
    /// (calls - calls_at_last_shrink) * 2)` so a long shrink search
    /// where each step is expensive doesn't get cut off prematurely.
    pub max_stall: usize,
    /// Snapshot of `current_nodes` at the last call to
    /// [`Shrinker::clear_change_tracking`] (or construction).  Each `consider`
    /// improvement diffs against this baseline so [`Shrinker::changed_nodes`]
    /// reports node indices whose `(kind, value)` differs.
    last_checkpoint_nodes: Vec<ChoiceNode>,
    /// Set of indices that changed (under structural identity) since the last
    /// checkpoint.  `lower_common_node_offset` reads this to find correlated
    /// integer nodes that keep shrinking together (`shrinker.py:1097-1131`).
    all_changed_nodes: HashSet<usize>,
    /// Bounded cache of *uninteresting* candidate sort_keys.  Mirrors
    /// Hypothesis's `cached_test_function` (`shrinker.py:390-412`).
    /// When two passes propose the same candidate (or one with the
    /// same sort_key shape), the cached negative result lets
    /// `consider` short-circuit without re-running the closure.
    ///
    /// Positive (interesting) results are *not* cached because
    /// `accept_improvement`'s `sort_key(actual) < sort_key(current)`
    /// check is relative to the live shrink target — a positive that
    /// didn't improve last time might improve now.
    ///
    /// `consider_cache_set` is the membership index; `consider_cache_order`
    /// records insertion order so eviction is FIFO rather than the
    /// undefined order of `HashSet::iter`.
    consider_cache_set: HashSet<Vec<NodeSortKey>>,
    consider_cache_order: VecDeque<Vec<NodeSortKey>>,
}

impl<'a> Shrinker<'a> {
    /// Construct a Shrinker from a closure that handles both [`ShrinkRun::Full`]
    /// and [`ShrinkRun::Probe`] requests. Required for `mutate_and_shrink` to
    /// actually explore random continuations.
    pub fn with_probe(
        test_fn: Box<TestFn<'a>>,
        initial_nodes: Vec<ChoiceNode>,
        initial_spans: Spans,
    ) -> Self {
        Shrinker {
            test_fn,
            last_checkpoint_nodes: initial_nodes.clone(),
            current_nodes: initial_nodes,
            current_spans: initial_spans,
            improvements: 0,
            downgraded: Vec::new(),
            max_improvements: MAX_SHRINKS,
            calls: 0,
            calls_at_last_shrink: 0,
            max_stall: 200,
            all_changed_nodes: HashSet::new(),
            consider_cache_set: HashSet::new(),
            consider_cache_order: VecDeque::new(),
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
        let candidate_key = sort_key(nodes);
        if candidate_key == sort_key(&self.current_nodes) {
            return true;
        }
        // Forced-node guard: a candidate may not differ from the
        // current shrink target at any index marked `was_forced`.
        // Mirrors Hypothesis's invariant that forced choices stay put
        // through shrinking.  `replace` enforces this on its own
        // single-position path; consider covers callers that build
        // candidate sequences directly (try_shortening_via_increment,
        // delete_chunks, span passes, …).
        for (i, candidate) in nodes
            .iter()
            .enumerate()
            .take(nodes.len().min(self.current_nodes.len()))
        {
            if self.current_nodes[i].was_forced && candidate.value != self.current_nodes[i].value {
                return false;
            }
        }
        if self.improvements >= self.max_improvements {
            return false;
        }
        if self.calls.saturating_sub(self.calls_at_last_shrink) >= self.max_stall {
            return false;
        }
        // Negative-result cache: if we already asked the closure
        // about a candidate with this sort_key and it was
        // uninteresting, short-circuit.  Mirrors
        // `cached_test_function` (`shrinker.py:390-412`), restricted
        // to the negative case — see the field docstring for why
        // positive results aren't cached.
        let cache_key: Vec<NodeSortKey> = candidate_key.1.clone();
        if self.consider_cache_set.contains(&cache_key) {
            return false;
        }

        self.calls += 1;
        let (is_interesting, actual_nodes, actual_spans) = (self.test_fn)(ShrinkRun::Full(nodes));
        // Bounded cache with FIFO eviction: drop the oldest entry once
        // we exceed 4096.  Insertion-order is recorded explicitly in
        // `consider_cache_order` — `HashSet::iter` makes no order
        // guarantee, so the previous version was effectively random.
        if !is_interesting && self.consider_cache_set.insert(cache_key.clone()) {
            self.consider_cache_order.push_back(cache_key);
            if self.consider_cache_set.len() > 4096 {
                if let Some(oldest) = self.consider_cache_order.pop_front() {
                    self.consider_cache_set.remove(&oldest);
                }
            }
        }
        if is_interesting && sort_key(&actual_nodes) < sort_key(&self.current_nodes) {
            self.accept_improvement(actual_nodes, actual_spans);
        }
        is_interesting
    }

    /// Run a probe: replay `prefix` then continue with random draws from a
    /// deterministic RNG seeded by `seed`, capped at `max_size` choices. If
    /// the resulting run is interesting and shortlex-smaller than
    /// `current_nodes`, update `current_nodes`.
    ///
    /// Port of Hypothesis's `shrinker.test_function(TestCase(prefix=..., random=...))`.
    pub(super) fn probe(&mut self, prefix: &[ChoiceValue], seed: u64, max_size: usize) {
        if self.improvements >= self.max_improvements {
            return;
        }
        if self.calls.saturating_sub(self.calls_at_last_shrink) >= self.max_stall {
            return;
        }
        self.calls += 1;
        let (is_interesting, actual_nodes, actual_spans) = (self.test_fn)(ShrinkRun::Probe {
            prefix,
            seed,
            max_size,
        });
        if is_interesting && sort_key(&actual_nodes) < sort_key(&self.current_nodes) {
            self.accept_improvement(actual_nodes, actual_spans);
        }
    }

    /// Common bookkeeping when a candidate becomes the new shrink target:
    /// record the displaced sequence, bump `improvements`, fold the diff
    /// into `all_changed_nodes`, and refresh `current_nodes` / `current_spans`.
    fn accept_improvement(&mut self, new_nodes: Vec<ChoiceNode>, new_spans: Spans) {
        let old: Vec<ChoiceValue> = self.current_nodes.iter().map(|n| n.value.clone()).collect();
        self.downgraded.push(old);
        self.improvements += 1;
        // Grow max_stall so a long shrink search doesn't get cut off
        // prematurely.  Mirrors `shrinker.py:1139-1141`.
        let span = self.calls.saturating_sub(self.calls_at_last_shrink);
        let grown = span.saturating_mul(2);
        if grown > self.max_stall {
            self.max_stall = grown;
        }
        self.calls_at_last_shrink = self.calls;
        Self::update_change_tracking(
            &self.last_checkpoint_nodes,
            &new_nodes,
            &mut self.all_changed_nodes,
        );
        self.current_nodes = new_nodes;
        self.current_spans = new_spans;
    }

    /// Update `changed` to reflect a diff between `prev` and `new`.
    ///
    /// Mirrors `shrinker.py:1097-1131`: when shape (length, kinds) is preserved
    /// across the improvement, indices whose value changed are unioned into
    /// `changed`.  When shape changes the set is cleared — there's no stable
    /// identity between old and new node positions.
    fn update_change_tracking(
        prev: &[ChoiceNode],
        new: &[ChoiceNode],
        changed: &mut HashSet<usize>,
    ) {
        let shape_preserved = prev.len() == new.len()
            && prev
                .iter()
                .zip(new.iter())
                .all(|(a, b)| std::mem::discriminant(&a.kind) == std::mem::discriminant(&b.kind));
        if !shape_preserved {
            changed.clear();
            return;
        }
        for (i, (a, b)) in prev.iter().zip(new.iter()).enumerate() {
            if a.value != b.value {
                changed.insert(i);
            }
        }
    }

    /// Indices that changed between `last_checkpoint_nodes` and `current_nodes`.
    /// Consumed by `lower_common_node_offset`.
    pub fn changed_nodes(&self) -> &HashSet<usize> {
        &self.all_changed_nodes
    }

    /// Reset the change-tracking set and rebaseline at `current_nodes`.
    pub fn clear_change_tracking(&mut self) {
        self.all_changed_nodes.clear();
        self.last_checkpoint_nodes = self.current_nodes.clone();
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
                return false; // nocov — index range guarded by callers
            }
            if attempt[i].was_forced {
                // Forced choices stay put — mirrors Hypothesis's
                // `n.was_forced` check throughout the shrinker.
                return false;
            }
            if !attempt[i].kind.validate(v) {
                return false; // nocov — kind/value mismatch after one_of branch switch
            }
            attempt[i] = attempt[i].with_value(v.clone());
        }
        self.consider(&attempt)
    }

    /// Run all shrink passes repeatedly until no more progress or iteration cap.
    ///
    /// The pass order interleaves Hypothesis-ported passes with the
    /// native-only extras: span-aware structural passes first (cheap
    /// when they apply), then deletion / zeroing, then the value-level
    /// minimization passes, finishing with the index-generic and
    /// entropy-based passes.
    pub fn shrink(&mut self) {
        // Build the pass list and hand it to the scheduler.  Each pass
        // is wrapped in a `ShrinkPass` so `fixate_shrink_passes` can
        // track per-pass stats and re-order them by recent success
        // between outer iterations.
        let mut passes: Vec<ShrinkPass> = vec![
            // Span-aware passes — catch shapes the per-type passes
            // can't see, run ahead of them.
            ShrinkPass::new(
                "remove_discarded",
                Box::new(|sh| {
                    sh.remove_discarded();
                }),
            ),
            ShrinkPass::new("try_trivial_spans", Box::new(|sh| sh.try_trivial_spans())),
            ShrinkPass::new("pass_to_descendant", Box::new(|sh| sh.pass_to_descendant())),
            ShrinkPass::new("reorder_spans", Box::new(|sh| sh.reorder_spans())),
            // Node-program adaptive deletion (Hypothesis's
            // `node_program("X" * n)` family).
            ShrinkPass::new("node_program_5", Box::new(|sh| sh.node_program(5))),
            ShrinkPass::new("node_program_4", Box::new(|sh| sh.node_program(4))),
            ShrinkPass::new("node_program_3", Box::new(|sh| sh.node_program(3))),
            ShrinkPass::new("node_program_2", Box::new(|sh| sh.node_program(2))),
            ShrinkPass::new("node_program_1", Box::new(|sh| sh.node_program(1))),
            ShrinkPass::new("delete_chunks", Box::new(|sh| sh.delete_chunks())),
            ShrinkPass::new("zero_choices", Box::new(|sh| sh.zero_choices())),
            ShrinkPass::new("swap_integer_sign", Box::new(|sh| sh.swap_integer_sign())),
            ShrinkPass::new(
                "binary_search_integer_towards_zero",
                Box::new(|sh| sh.binary_search_integer_towards_zero()),
            ),
            ShrinkPass::new("bind_deletion", Box::new(|sh| sh.bind_deletion())),
            ShrinkPass::new(
                "minimize_individual_choices",
                Box::new(|sh| sh.minimize_individual_choices()),
            ),
            ShrinkPass::new(
                "lower_common_node_offset",
                Box::new(|sh| sh.lower_common_node_offset()),
            ),
            ShrinkPass::new(
                "redistribute_integers",
                Box::new(|sh| sh.redistribute_integers()),
            ),
            ShrinkPass::new(
                "lower_integers_together",
                Box::new(|sh| sh.lower_integers_together()),
            ),
            ShrinkPass::new("shrink_duplicates", Box::new(|sh| sh.shrink_duplicates())),
            ShrinkPass::new("sort_values", Box::new(|sh| sh.sort_values())),
            ShrinkPass::new(
                "swap_adjacent_blocks",
                Box::new(|sh| sh.swap_adjacent_blocks()),
            ),
            ShrinkPass::new("shrink_floats", Box::new(|sh| sh.shrink_floats())),
            ShrinkPass::new(
                "redistribute_numeric_pairs",
                Box::new(|sh| sh.redistribute_numeric_pairs()),
            ),
            ShrinkPass::new("shrink_bytes", Box::new(|sh| sh.shrink_bytes())),
            ShrinkPass::new(
                "redistribute_bytes_pairs",
                Box::new(|sh| sh.redistribute_bytes_pairs()),
            ),
            ShrinkPass::new("shrink_strings", Box::new(|sh| sh.shrink_strings())),
            ShrinkPass::new(
                "lower_duplicated_characters",
                Box::new(|sh| sh.lower_duplicated_characters()),
            ),
            ShrinkPass::new(
                "normalize_unicode_chars",
                Box::new(|sh| sh.normalize_unicode_chars()),
            ),
            ShrinkPass::new(
                "redistribute_string_pairs",
                Box::new(|sh| sh.redistribute_string_pairs()),
            ),
            ShrinkPass::new("lower_and_bump", Box::new(|sh| sh.lower_and_bump())),
            ShrinkPass::new(
                "try_shortening_via_increment",
                Box::new(|sh| sh.try_shortening_via_increment()),
            ),
            ShrinkPass::new("mutate_and_shrink", Box::new(|sh| sh.mutate_and_shrink())),
        ];
        self.fixate_shrink_passes(&mut passes);
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
pub(crate) fn find_integer(mut f: impl FnMut(usize) -> bool) -> usize {
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
            return lo; // nocov — usize overflow guard; tests never reach usize::MAX/2
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
#[path = "../../../tests/embedded/native/shrinker_spans_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_forced_node_tests.rs"]
mod forced_node_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_cache_tests.rs"]
mod cache_tests;

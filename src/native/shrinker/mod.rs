// Shrinker for the native backend. Reduces failing test cases to minimal
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

use std::collections::{HashMap, HashSet};

use crate::native::bignum::BigInt;
use crate::native::core::{
    ChoiceKind, ChoiceNode, ChoiceValue, MAX_SHRINKS, NodeSortKey, Spans, sort_key,
};

/// Request passed to the shrinker's test function.
///
/// [`ShrinkRun::Full`] replays a full node sequence with punning (the shape used by
/// most shrink passes). [`ShrinkRun::Probe`] replays a prefix of choice values and
/// then draws randomly beyond it — needed by `mutate_and_shrink`.
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

/// A callback for shrinker debug output (per-pass-step lines and the
/// end-of-shrink profiling report).  Wired only at `Verbosity::Debug`.
pub type DebugFn<'a> = dyn FnMut(&str) + 'a;

pub struct Shrinker<'a> {
    test_fn: Box<TestFn<'a>>,
    pub current_nodes: Vec<ChoiceNode>,
    /// Spans recorded by the run that produced `current_nodes`.  Updated whenever
    /// `consider` accepts a smaller candidate so span-aware passes (try_trivial_spans,
    /// pass_to_descendant, reorder_spans, remove_discarded) can interrogate the
    /// current shrink target's structure.
    pub current_spans: Spans,
    /// Count of times `current_nodes` was replaced by a strictly smaller candidate.
    pub improvements: usize,
    /// The choice sequences that were displaced each time `current_nodes` improved.
    /// Used by `shrink_interesting_examples` to downgrade each predecessor to the
    /// secondary key.
    pub downgraded: Vec<Vec<ChoiceValue>>,
    /// Cap on `improvements`. Once `improvements >= max_improvements`,
    /// `consider` and `probe` short-circuit so the runner doesn't get
    /// stuck chasing diminishing returns. Defaults to [`MAX_SHRINKS`];
    /// tests can lower it for controlled-budget assertions.
    pub max_improvements: usize,
    /// Total number of times the test closure has been invoked through
    /// `consider` or `probe`.  Used together with `calls_at_last_shrink`
    /// + `max_stall` to detect runaway shrink searches.
    pub calls: usize,
    /// Value of `calls` at the moment of the most recent
    /// `accept_improvement`.  See `max_stall`.
    pub calls_at_last_shrink: usize,
    /// Once `calls - calls_at_last_shrink >= max_stall`, further
    /// `consider` / `probe` invocations short-circuit. Grows on every
    /// successful shrink by
    /// `max(max_stall, (calls - calls_at_last_shrink) * 2)` so a long
    /// shrink search where each step is expensive doesn't get cut off
    /// prematurely.
    ///
    /// Default is [`MAX_SHRINKS`] = 500. `calls` is shrinker-local and
    /// starts at zero, so a tighter threshold lands mid-pass for
    /// predicates that need many cold calls between the first few
    /// shrinks and stalls on a sub-minimal target.
    pub max_stall: usize,
    /// Snapshot of `current_nodes` at the last call to
    /// [`Shrinker::clear_change_tracking`] (or construction).  Each `consider`
    /// improvement diffs against this baseline so [`Shrinker::changed_nodes`]
    /// reports node indices whose `(kind, value)` differs.
    last_checkpoint_nodes: Vec<ChoiceNode>,
    /// Set of indices that changed (under structural identity) since the last
    /// checkpoint. `lower_common_node_offset` reads this to find correlated
    /// integer nodes that keep shrinking together.
    all_changed_nodes: HashSet<usize>,
    /// Negative-result cache of candidate sort_keys. When two passes
    /// propose the same candidate (or one with the same sort_key shape),
    /// the cached negative result lets `consider` short-circuit without
    /// re-running the closure.
    ///
    /// Positive (interesting) results are *not* cached because
    /// `accept_improvement`'s `sort_key(actual) < sort_key(current)`
    /// check is relative to the live shrink target — a positive that
    /// didn't improve last time might improve now.
    ///
    /// Unbounded. Test cases produce a few hundred to a few thousand
    /// distinct candidates; an unbounded cache caps memory at a few MB
    /// on the largest seen runs. Bounding the cache with FIFO / LRU
    /// eviction introduced seed-dependent shrink trajectories that
    /// converged on neighbouring minima (cache eviction interacts with
    /// the order in which redistribute candidates are revisited), so
    /// eviction is simply dropped.
    consider_cache: HashSet<Vec<(u8, NodeSortKey)>>,
    /// Optional debug callback. When set, the shrinker emits
    /// per-pass-step "Trying shrink pass: <name>" lines and an
    /// end-of-shrink "Shrink pass profiling" report. Wired by the test
    /// runner at `Verbosity::Debug`; unused otherwise.
    pub(super) debug: Option<Box<DebugFn<'a>>>,
}

/// One-byte tag identifying a `ChoiceKind` variant — used by
/// `consider_cache` to keep entries for kind-punned candidates
/// separate.  Must agree across `Shrinker::consider` calls; only the
/// shrinker reads it.
fn kind_tag(kind: &crate::native::core::ChoiceKind) -> u8 {
    use crate::native::core::ChoiceKind::*;
    match kind {
        Boolean(_) => 0,
        Integer(_) => 1,
        Float(_) => 2,
        Bytes(_) => 3,
        String(_) => 4,
    }
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
            max_stall: MAX_SHRINKS,
            all_changed_nodes: HashSet::new(),
            consider_cache: HashSet::new(),
            debug: None,
        }
    }

    /// Install a debug callback.  Each emitted message corresponds to
    /// either the start of a pass step (`"Trying shrink pass: <name>"`)
    /// or one line of the end-of-shrink profiling report.  Wired by the
    /// test runner at `Verbosity::Debug`.
    pub fn set_debug<F: FnMut(&str) + 'a>(&mut self, f: F) {
        self.debug = Some(Box::new(f));
    }

    pub(super) fn debug_msg(&mut self, msg: &str) {
        if let Some(d) = self.debug.as_mut() {
            d(msg);
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
        // Forced-node guard: a candidate may not differ from the
        // current shrink target at any index marked `was_forced`.
        // Forced choices stay put through shrinking. `replace` enforces
        // this on its own single-position path; consider covers callers
        // that build candidate sequences directly
        // (try_shortening_via_increment, delete_chunks, span passes, …).
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
        // Only enforce the stall guard once we've found at least one
        // improvement.  Without warmup, predicates that need many calls
        // to find the first shrink — e.g. searching a large
        // redistribute space — trip the guard before making any
        // progress and stall on a sub-minimal target.
        if self.improvements > 0
            && self.calls.saturating_sub(self.calls_at_last_shrink) >= self.max_stall
        {
            return false;
        }
        // Negative-result cache: if we already asked the closure about a
        // candidate with this sort_key and it was uninteresting,
        // short-circuit. See the field docstring for why positive results
        // aren't cached.
        // Cache key bundles the kind discriminant with the per-node
        // sort key: `NodeSortKey::Scalar(0, false)` is produced both
        // by `Boolean(false)` and `Integer(0)`, and a cache shared on
        // sort_key alone would falsely short-circuit kind-punned
        // candidates that the test_fn would in fact accept.
        let cache_key: Vec<(u8, NodeSortKey)> = nodes
            .iter()
            .map(|node| (kind_tag(&node.kind), node.sort_key()))
            .collect();
        if self.consider_cache.contains(&cache_key) {
            return false;
        }

        self.calls += 1;
        let (is_interesting, actual_nodes, actual_spans) = (self.test_fn)(ShrinkRun::Full(nodes));
        // Bounded cache with FIFO eviction: drop the oldest entry once
        // we exceed 4096.  Insertion-order is recorded explicitly in
        // `consider_cache_order` — `HashSet::iter` makes no order
        // guarantee, so the previous version was effectively random.
        if !is_interesting {
            self.consider_cache.insert(cache_key);
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
        // prematurely.
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
    /// When shape (length, kinds) is preserved across the improvement,
    /// indices whose value changed are unioned into `changed`. When shape
    /// changes the set is cleared — there's no stable identity between
    /// old and new node positions.
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
                return false;
            }
            if attempt[i].was_forced {
                // Forced choices stay put.
                return false;
            }
            if !attempt[i].kind.validate(v) {
                return false;
            }
            // Integer values must be expressed in the target node's width — a
            // pass may move a value between integer nodes of different widths
            // (e.g. `sort_values`). Coerce to the node's width so the stored
            // node stays width-consistent. The `validate` check above already
            // guarantees the value lies in `[min, max] ⊆ width`, so the
            // conversion cannot fail.
            let coerced = match (&attempt[i].kind, v) {
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(av)) => ChoiceValue::Integer(
                    ic.value_from_bigint(&av.to_bigint())
                        .unwrap_or_else(|| unreachable!("validated integer fits the node's width")),
                ),
                _ => v.clone(),
            };
            attempt[i] = attempt[i].with_value(coerced);
        }
        self.consider(&attempt)
    }

    /// Format an end-of-shrink profile report and feed it line-by-line to
    /// the debug callback. Passes with zero calls are filtered out, the
    /// remainder are split into useful (`shrinks > 0`) and useless
    /// buckets, each bucket sorted by `(-calls, deletions, shrinks)`.
    fn emit_profile_report(
        &mut self,
        passes: &[ShrinkPass<'a>],
        initial_size: usize,
        initial_calls: usize,
    ) {
        if self.debug.is_none() {
            return;
        }
        fn s(n: usize) -> &'static str {
            if n == 1 { "" } else { "s" }
        }
        let stats = self.pass_stats(passes);
        let total_calls = self.calls.saturating_sub(initial_calls);
        let total_deleted = initial_size.saturating_sub(self.current_nodes.len());
        let shrinks = self.improvements;
        self.debug_msg("---------------------");
        self.debug_msg("Shrink pass profiling");
        self.debug_msg("---------------------");
        self.debug_msg("");
        self.debug_msg(&format!(
            "Shrinking made a total of {total_calls} call{} of which {shrinks} shrank. \
             This deleted {total_deleted} choice{} out of {initial_size}.",
            s(total_calls),
            s(total_deleted),
        ));
        for useful in [true, false] {
            self.debug_msg("");
            self.debug_msg(if useful {
                "Useful passes:"
            } else {
                "Useless passes:"
            });
            self.debug_msg("");
            let mut buckets: Vec<&(&'static str, usize, usize, usize)> = stats
                .iter()
                .filter(|(_, calls, shrinks, _)| *calls > 0 && ((*shrinks > 0) == useful))
                .collect();
            buckets.sort_by_key(|(_, calls, shrinks, deletions)| {
                (std::cmp::Reverse(*calls), *deletions, *shrinks)
            });
            for (name, calls, shrinks, deletions) in buckets {
                self.debug_msg(&format!(
                    "  * {name} made {calls} call{} of which {shrinks} shrank, \
                     deleting {deletions} choice{}.",
                    s(*calls),
                    s(*deletions),
                ));
            }
        }
        self.debug_msg("");
    }

    /// Run all shrink passes repeatedly until no more progress or iteration cap.
    ///
    /// The pass order runs span-aware structural passes first (cheap when
    /// they apply), then deletion / zeroing, then the value-level
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
            // Node-program adaptive deletion (the `node_program("X" * n)`
            // family).
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
        let initial_size = self.current_nodes.len();
        let initial_calls = self.calls;
        self.fixate_shrink_passes(&mut passes);
        self.emit_profile_report(&passes, initial_size, initial_calls);
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

/// [`BigInt`] counterpart of [`bin_search_down`], used by the integer shrink
/// passes which now carry values as arbitrary-precision integers. Same
/// contract: assumes `f(hi)` is true, returns the smallest locally-true value
/// in `[lo, hi]`.
pub(super) fn bin_search_down_big(
    lo: BigInt,
    hi: BigInt,
    f: &mut impl FnMut(&BigInt) -> bool,
) -> BigInt {
    if f(&lo) {
        return lo;
    }
    let mut lo = lo;
    let mut hi = hi;
    while &lo + 1 < hi {
        let mid = &lo + (&hi - &lo) / 2;
        if f(&mid) {
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
/// Used by shrink passes that want to maximise a step size — e.g. "lower
/// both nodes by k" needs the largest k for which the joint replacement
/// is still interesting.
///
/// Uses `checked_mul` on the exponential probe and `lo + (hi - lo) / 2` on
/// the binary-search midpoint: a predicate that accepts an unbounded range
/// (e.g. a `lower_integers_together` pass over full-range `i128` nodes)
/// would otherwise walk `hi` off the end of `usize`.
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
#[path = "../../../tests/embedded/native/shrinker_spans_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_forced_node_tests.rs"]
mod forced_node_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_cache_tests.rs"]
mod cache_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_defensive_branch_tests.rs"]
mod defensive_branch_tests;

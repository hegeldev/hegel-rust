// Targeted property-based search for the production native runner.
//
// Plumbs `tc.target()` / `tc.target_labelled()` observations into a
// hill-climber that perturbs integer choices in the best-scoring sequence
// for each target label. Mirrors `Optimiser.hill_climb` in Hypothesis's
// `internal/conjecture/optimiser.py` and the `optimise_targets` driver in
// `engine.py`.

use std::collections::{HashMap, HashSet};

use crate::native::conjecture_runner::{is_climbable, step_choice};
use crate::native::core::{ChoiceNode, ChoiceValue, NativeTestCase, Status, sort_key};
use crate::native::tree::NativeRunner;

/// Per-label best score and the choice sequence that produced it.
pub(crate) struct TargetingState {
    pub best_observed_targets: HashMap<String, f64>,
    pub best_choices_for_target: HashMap<String, Vec<ChoiceValue>>,
}

impl TargetingState {
    pub fn new() -> Self {
        Self {
            best_observed_targets: HashMap::new(),
            best_choices_for_target: HashMap::new(),
        }
    }

    /// Record the observations from a Valid run. The first observation for
    /// each label always populates both maps; subsequent observations only
    /// overwrite when the score strictly improves. The two maps therefore
    /// share the same key set (relied on by [`hill_climb`]).
    pub fn record(&mut self, choices: &[ChoiceValue], observations: &HashMap<String, f64>) {
        for (label, &score) in observations {
            let should_record = match self.best_observed_targets.get(label) {
                None => true,
                Some(&best) => score > best,
            };
            if should_record {
                self.best_observed_targets.insert(label.clone(), score);
                self.best_choices_for_target
                    .insert(label.clone(), choices.to_vec());
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.best_observed_targets.is_empty()
    }
}

/// All the targeted-PBT state and triggers needed by `native_run`. Holds
/// the per-run [`TargetingState`] plus the one-shot `optimise_at` gate
/// (mirroring Hypothesis's `engine.py::generate_new_examples` line 1317
/// behaviour: fire `optimise_targets` exactly once after enough valid
/// examples have been observed). Wrapping all of this in a single struct
/// lets the runner's hot loop call into the targeting module via two
/// short method calls instead of inlining the whole protocol.
pub(crate) struct TargetingDriver {
    targeting: TargetingState,
    optimise_at: u64,
    ran_optimisations: bool,
}

impl TargetingDriver {
    pub fn new(max_examples: u64) -> Self {
        let small_example_cap = (max_examples / 10).min(50);
        let optimise_at = (max_examples / 2).max(small_example_cap + 1).max(10);
        Self {
            targeting: TargetingState::new(),
            optimise_at,
            ran_optimisations: false,
        }
    }

    /// Record `observations` against the choice sequence in `nodes`. No-op
    /// when the test made no `tc.target()` calls.
    pub fn record(&mut self, nodes: &[ChoiceNode], observations: &HashMap<String, f64>) {
        if observations.is_empty() {
            return;
        }
        let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
        self.targeting.record(&choices, observations);
    }

    /// Fire `optimise_targets` if we've crossed the `optimise_at` threshold,
    /// haven't already run this pass, and have at least one observed target
    /// to climb. No-op once an interesting example has been recorded — at
    /// that point the runner will move on to shrinking.
    pub fn maybe_optimise(
        &mut self,
        runner: &mut dyn NativeRunner,
        result: &mut Option<Vec<ChoiceNode>>,
        calls: &mut u64,
        valid_test_cases: &mut u64,
        max_examples: u64,
    ) {
        if self.ran_optimisations
            || *valid_test_cases < self.optimise_at
            || self.targeting.is_empty()
            || result.is_some()
        {
            return;
        }
        self.ran_optimisations = true;
        let mut ctx = OptimiseCtx {
            result,
            calls,
            valid_test_cases,
            max_valid: max_examples,
            max_calls: max_examples * 10,
        };
        optimise_targets(runner, &mut self.targeting, &mut ctx);
    }
}

/// Mutable state threaded through the hill-climber.
pub(crate) struct OptimiseCtx<'a> {
    pub result: &'a mut Option<Vec<ChoiceNode>>,
    pub calls: &'a mut u64,
    pub valid_test_cases: &'a mut u64,
    pub max_valid: u64,
    pub max_calls: u64,
}

impl OptimiseCtx<'_> {
    fn budget_exhausted(&self) -> bool {
        self.result.is_some()
            || *self.valid_test_cases >= self.max_valid
            || *self.calls >= self.max_calls
    }
}

/// Run a single trial and update bookkeeping. Returns `Some((status, nodes,
/// observations))` if the trial completed (regardless of status); returns
/// `None` if the budget was exhausted before this call.
fn run_trial(
    runner: &mut dyn NativeRunner,
    targeting: &mut TargetingState,
    ctx: &mut OptimiseCtx<'_>,
    choices: &[ChoiceValue],
) -> Option<(Status, Vec<ChoiceNode>, HashMap<String, f64>)> {
    if ctx.budget_exhausted() {
        return None;
    }
    let ntc = NativeTestCase::for_choices(choices, None, None);
    let run = runner.run(ntc);
    *ctx.calls += 1;
    if run.status >= Status::Valid {
        *ctx.valid_test_cases += 1;
        let actual_choices: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value.clone()).collect();
        targeting.record(&actual_choices, &run.target_observations);
    }
    if run.status == Status::Interesting
        && (ctx.result.is_none() || sort_key(&run.nodes) < sort_key(ctx.result.as_ref().unwrap()))
    {
        *ctx.result = Some(run.nodes.clone());
    }
    Some((run.status, run.nodes, run.target_observations))
}

/// Hill-climb every target until no further improvements are found or the
/// budget is exhausted. Mirrors `engine.py::optimise_targets`.
///
/// Budget is enforced by [`run_trial`], which is the only place a test is
/// actually invoked; once it short-circuits, every downstream `try_replace`
/// returns false and the surrounding loops fall out via their natural
/// no-improvement exits.
pub(crate) fn optimise_targets(
    runner: &mut dyn NativeRunner,
    targeting: &mut TargetingState,
    ctx: &mut OptimiseCtx<'_>,
) {
    let targets: Vec<String> = targeting.best_observed_targets.keys().cloned().collect();
    let mut max_improvements: usize = 10;
    loop {
        let prev_calls = *ctx.calls;
        let mut any_improvements = false;
        for target in &targets {
            let imps = hill_climb(runner, targeting, ctx, target, max_improvements);
            if imps > 0 {
                any_improvements = true;
            }
        }
        max_improvements = max_improvements.saturating_mul(2);
        if !any_improvements || prev_calls == *ctx.calls {
            return;
        }
    }
}

/// Walk the integer choices in `best_choices_for_target[target]` from the
/// end backwards, hill-climbing each one in both directions. Mirrors
/// `Optimiser._optimise_target`.
fn hill_climb(
    runner: &mut dyn NativeRunner,
    targeting: &mut TargetingState,
    ctx: &mut OptimiseCtx<'_>,
    target: &str,
    max_improvements: usize,
) -> usize {
    // `record` keeps `best_choices_for_target` in sync with `best_observed_targets`,
    // so any label our caller iterates from `best_observed_targets` must have a
    // matching choice sequence here.
    let start_choices = targeting
        .best_choices_for_target
        .get(target)
        .cloned()
        .unwrap_or_else(|| {
            unreachable!("best_choices_for_target out of sync with best_observed_targets")
        });
    let trial = match run_trial(runner, targeting, ctx, &start_choices) {
        Some(t) => t,
        None => return 0,
    };
    let (status, nodes, observations) = trial;
    if status < Status::Valid {
        return 0;
    }
    let mut current_choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
    let mut current_nodes = nodes;
    let mut current_score = *observations.get(target).unwrap_or(&f64::NEG_INFINITY);
    let mut improvements: usize = 0;

    let mut nodes_examined: HashSet<usize> = HashSet::new();
    let mut i: isize = current_nodes.len() as isize - 1;
    let mut prev_len = current_nodes.len();
    while i >= 0 && improvements <= max_improvements {
        // E3 (mirrors `optimiser.py:95-97` and the parallel
        // `optimiser.rs::hill_climb`): when `find_integer` lengthens or
        // shortens `current_nodes`, the existing `i` no longer indexes
        // the same logical position. Reset to the new tail and start
        // afresh; clear `nodes_examined` so positions that were valid
        // before but now correspond to different node identities can
        // be retried.
        if current_nodes.len() != prev_len {
            i = current_nodes.len() as isize - 1;
            prev_len = current_nodes.len();
            nodes_examined.clear();
            continue;
        }
        let idx = i as usize;
        if idx >= current_nodes.len() || !nodes_examined.insert(idx) {
            // `idx` was either resized out of range during a re-entry
            // or already visited; either way, advance.
            i -= 1;
            continue;
        }
        let node = &current_nodes[idx];
        if !node.was_forced && is_climbable(&node.value, &node.kind) {
            let len_before = current_nodes.len();
            improvements += find_integer(
                runner,
                targeting,
                ctx,
                target,
                &mut current_choices,
                &mut current_nodes,
                &mut current_score,
                max_improvements.saturating_sub(improvements),
                idx,
                1,
            );
            // If the +1 direction grew current_nodes, idx no longer points
            // at the same logical position; trying -1 there almost always
            // shrinks the sequence back below the new score, so skip.
            // Mirrors the same guard in Hypothesis's `Optimiser.hill_climb`.
            if idx < current_nodes.len() && current_nodes.len() == len_before {
                improvements += find_integer(
                    runner,
                    targeting,
                    ctx,
                    target,
                    &mut current_choices,
                    &mut current_nodes,
                    &mut current_score,
                    max_improvements.saturating_sub(improvements),
                    idx,
                    -1,
                );
            }
        }
        i -= 1;
    }
    improvements
}

/// Port of `junkdrawer.find_integer`: linear scan over deltas 1..5, then
/// exponential probing 5, 10, 20, ..., then a binary search between the
/// last accepted delta and the first rejected one.
#[allow(clippy::too_many_arguments)]
fn find_integer(
    runner: &mut dyn NativeRunner,
    targeting: &mut TargetingState,
    ctx: &mut OptimiseCtx<'_>,
    target: &str,
    current_choices: &mut Vec<ChoiceValue>,
    current_nodes: &mut Vec<ChoiceNode>,
    current_score: &mut f64,
    max_improvements: usize,
    idx: usize,
    sign: i128,
) -> usize {
    let mut improvements: usize = 0;

    for k in 1..5i128 {
        if improvements >= max_improvements {
            return improvements;
        }
        if !try_replace(
            runner,
            targeting,
            ctx,
            target,
            current_choices,
            current_nodes,
            current_score,
            &mut improvements,
            idx,
            sign * k,
        ) {
            return improvements;
        }
    }

    let mut lo: i128 = 4;
    let mut hi: i128 = 5;
    loop {
        if improvements >= max_improvements {
            return improvements;
        }
        if !try_replace(
            runner,
            targeting,
            ctx,
            target,
            current_choices,
            current_nodes,
            current_score,
            &mut improvements,
            idx,
            sign * hi,
        ) {
            break;
        }
        lo = hi;
        hi = hi.saturating_mul(2);
        if hi > (1 << 20) {
            return improvements;
        }
    }

    while lo + 1 < hi {
        if improvements >= max_improvements {
            return improvements;
        }
        let mid = lo + (hi - lo) / 2;
        if try_replace(
            runner,
            targeting,
            ctx,
            target,
            current_choices,
            current_nodes,
            current_score,
            &mut improvements,
            idx,
            sign * mid,
        ) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    improvements
}

/// Replace `current_choices[idx]` by stepping it `delta` units in the
/// direction of the climb. Mirrors `optimiser.py::Optimiser.attempt_replace`
/// (lines 112-156) for the per-kind stepping (integer/float: addition;
/// boolean: ±1 toggles; bytes: big-endian addition with non-negative clamp)
/// and `consider_new_data` (lines 65-82) for score acceptance: a strict
/// score improvement commits the new state and bumps `improvements`; a tie
/// commits iff the new node count doesn't grow but does *not* count as an
/// improvement (lateral moves are the principal mechanism for escaping
/// local maxima, but they shouldn't keep the climber spinning forever).
/// Returns `true` iff the trial was committed.
#[allow(clippy::too_many_arguments)]
fn try_replace(
    runner: &mut dyn NativeRunner,
    targeting: &mut TargetingState,
    ctx: &mut OptimiseCtx<'_>,
    target: &str,
    current_choices: &mut Vec<ChoiceValue>,
    current_nodes: &mut Vec<ChoiceNode>,
    current_score: &mut f64,
    improvements: &mut usize,
    idx: usize,
    delta: i128,
) -> bool {
    let new_val = match step_choice(&current_nodes[idx], delta) {
        Some(v) => v,
        None => return false,
    };
    let mut trial_choices = current_choices.clone();
    trial_choices[idx] = new_val;
    let trial = match run_trial(runner, targeting, ctx, &trial_choices) {
        Some(t) => t,
        None => return false,
    };
    let (status, nodes, observations) = trial;
    if status < Status::Valid {
        return false;
    }
    let new_score = *observations.get(target).unwrap_or(&f64::NEG_INFINITY);
    if new_score < *current_score {
        return false;
    }
    let strict = new_score > *current_score;
    if !strict && nodes.len() > current_nodes.len() {
        return false;
    }
    *current_score = new_score;
    *current_choices = nodes.iter().map(|n| n.value.clone()).collect();
    *current_nodes = nodes;
    if strict {
        *improvements += 1;
    }
    true
}

#[cfg(test)]
#[path = "../../tests/embedded/native/targeting_tests.rs"]
mod tests;

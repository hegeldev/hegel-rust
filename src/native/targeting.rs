// Targeted property-based search for the production native runner.
//
// Plumbs `tc.target()` / `tc.target_labelled()` observations into a
// hill-climber that perturbs integer choices in the best-scoring sequence
// for each target label. Mirrors `Optimiser.hill_climb` in Hypothesis's
// `internal/conjecture/optimiser.py` and the `optimise_targets` driver in
// `engine.py`.

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, NativeTestCase, Status, sort_key};
use crate::native::tree::CachedTestFunction;
use crate::test_case::TestCase;

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

    /// Record the observations from a Valid run. For each label, if the
    /// score beats the current best, remember the new score and the choice
    /// sequence that produced it.
    pub fn record(&mut self, choices: &[ChoiceValue], observations: &HashMap<String, f64>) {
        for (label, &score) in observations {
            let entry = self
                .best_observed_targets
                .entry(label.clone())
                .or_insert(f64::NEG_INFINITY);
            if score > *entry {
                *entry = score;
                self.best_choices_for_target
                    .insert(label.clone(), choices.to_vec());
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.best_observed_targets.is_empty()
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
fn run_trial<F: FnMut(TestCase)>(
    ctf: &mut CachedTestFunction<F>,
    targeting: &mut TargetingState,
    ctx: &mut OptimiseCtx<'_>,
    choices: &[ChoiceValue],
) -> Option<(Status, Vec<ChoiceNode>, HashMap<String, f64>)> {
    if ctx.budget_exhausted() {
        return None;
    }
    let ntc = NativeTestCase::for_choices(choices, None, None);
    let run = ctf.run(ntc);
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
pub(crate) fn optimise_targets<F: FnMut(TestCase)>(
    ctf: &mut CachedTestFunction<F>,
    targeting: &mut TargetingState,
    ctx: &mut OptimiseCtx<'_>,
) {
    let targets: Vec<String> = targeting.best_observed_targets.keys().cloned().collect();
    if targets.is_empty() {
        return;
    }
    let mut max_improvements: usize = 10;
    loop {
        if ctx.budget_exhausted() {
            return;
        }
        let prev_calls = *ctx.calls;
        let mut any_improvements = false;
        for target in &targets {
            if ctx.budget_exhausted() {
                return;
            }
            let imps = hill_climb(ctf, targeting, ctx, target, max_improvements);
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
fn hill_climb<F: FnMut(TestCase)>(
    ctf: &mut CachedTestFunction<F>,
    targeting: &mut TargetingState,
    ctx: &mut OptimiseCtx<'_>,
    target: &str,
    max_improvements: usize,
) -> usize {
    let start_choices = match targeting.best_choices_for_target.get(target).cloned() {
        Some(c) => c,
        None => return 0,
    };
    let trial = match run_trial(ctf, targeting, ctx, &start_choices) {
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

    let mut i: isize = current_nodes.len() as isize - 1;
    while i >= 0 && improvements <= max_improvements {
        if ctx.budget_exhausted() {
            return improvements;
        }
        let idx = i as usize;
        let node = &current_nodes[idx];
        if !node.was_forced {
            if let (ChoiceValue::Integer(_), ChoiceKind::Integer(_)) = (&node.value, &node.kind) {
                let len_before = current_nodes.len();
                improvements += find_integer(
                    ctf,
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
                        ctf,
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
        }
        i -= 1;
    }
    improvements
}

/// Port of `junkdrawer.find_integer`: linear scan over deltas 1..5, then
/// exponential probing 5, 10, 20, ..., then a binary search between the
/// last accepted delta and the first rejected one.
#[allow(clippy::too_many_arguments)]
fn find_integer<F: FnMut(TestCase)>(
    ctf: &mut CachedTestFunction<F>,
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
        if improvements >= max_improvements || ctx.budget_exhausted() {
            return improvements;
        }
        if !try_replace(
            ctf,
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
        if improvements >= max_improvements || ctx.budget_exhausted() {
            return improvements;
        }
        if !try_replace(
            ctf,
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
        if improvements >= max_improvements || ctx.budget_exhausted() {
            return improvements;
        }
        let mid = lo + (hi - lo) / 2;
        if try_replace(
            ctf,
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

/// Replace `current_choices[idx]` with `current + delta`, bounds-check, and
/// run the trial. On a strict score improvement, commit the new state and
/// bump `improvements`. Returns `true` iff the trial improved the score.
#[allow(clippy::too_many_arguments)]
fn try_replace<F: FnMut(TestCase)>(
    ctf: &mut CachedTestFunction<F>,
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
    let current_val = match &current_choices[idx] {
        ChoiceValue::Integer(n) => *n,
        _ => unreachable!("try_replace called on non-integer node"),
    };
    let kind = match &current_nodes[idx].kind {
        ChoiceKind::Integer(ic) => ic,
        _ => unreachable!("try_replace called on non-integer node"),
    };
    let new_val = current_val.saturating_add(delta);
    if !kind.validate(new_val) {
        return false;
    }
    let mut trial_choices = current_choices.clone();
    trial_choices[idx] = ChoiceValue::Integer(new_val);
    let trial = match run_trial(ctf, targeting, ctx, &trial_choices) {
        Some(t) => t,
        None => return false,
    };
    let (status, nodes, observations) = trial;
    if status < Status::Valid {
        return false;
    }
    let new_score = *observations.get(target).unwrap_or(&f64::NEG_INFINITY);
    if new_score <= *current_score {
        return false;
    }
    *current_score = new_score;
    *current_choices = nodes.iter().map(|n| n.value.clone()).collect();
    *current_nodes = nodes;
    *improvements += 1;
    true
}

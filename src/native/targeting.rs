// Targeted property-based search for the native runner.
//
// Records `tc.target()` observations during the generation phase, keeps
// the best-scoring choice sequence seen for each label, and — when
// `Phase::Target` is enabled and no bug has been found yet — hill-climbs
// each label by perturbing climbable choices (integers, floats, booleans,
// bytes) in the best-scoring sequence.
//
// Mirrors Hypothesis's `internal/conjecture/optimiser.py::Optimiser.hill_climb`
// and the `optimise_targets` driver in `internal/conjecture/engine.py`. The
// integer-search step uses the same linear/exponential/binary pattern as
// `junkdrawer.find_integer`.

use std::collections::{HashMap, HashSet};

use crate::native::core::{
    BUFFER_SIZE, ChoiceKind, ChoiceNode, ChoiceValue, NativeTestCase, Span, Status,
};
use crate::native::shrinker::find_integer;
use crate::native::test_runner::{Engine, RunResult};

/// Per-label best score and the choice sequence that produced it.
pub(crate) struct TargetingState {
    best_observed_targets: HashMap<String, f64>,
    best_choices_for_target: HashMap<String, Vec<ChoiceValue>>,
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

    #[cfg(test)]
    pub(crate) fn best_score(&self, label: &str) -> Option<f64> {
        self.best_observed_targets.get(label).copied()
    }

    /// Best observed score per label, for the statistics report.
    pub(crate) fn best_targets(&self) -> &HashMap<String, f64> {
        &self.best_observed_targets
    }
}

/// Schedule for firing `optimise_targets` during the generation loop.
///
/// Mirrors the threshold logic in
/// `internal/conjecture/engine.py::generate_new_examples`: a first pass
/// after ~10% of the budget (capped at 50 valid test cases), then another
/// pass every ~50% of the original budget thereafter. Re-entry lets the
/// climber explore from fresh starting points if later random draws raise
/// the best-observed score for some label after the first pass settled
/// on a different starting sequence.
pub(crate) struct TargetingSchedule {
    step: u64,
    next_at: u64,
}

impl TargetingSchedule {
    pub fn new(max_test_cases: u64) -> Self {
        let small_example_cap = (max_test_cases / 10).min(50);
        let step = (max_test_cases / 2)
            .max(small_example_cap.saturating_add(1))
            .max(10);
        Self {
            step,
            next_at: step,
        }
    }

    /// Returns `true` if the caller should fire `optimise_targets` now.
    /// Advances the next-firing threshold so each call to this method on
    /// the same valid count returns `true` at most once.
    pub fn should_fire(&mut self, valid_test_cases: u64) -> bool {
        if valid_test_cases < self.next_at {
            return false;
        }
        self.next_at = valid_test_cases.saturating_add(self.step);
        true
    }
}

/// The hill-climbing optimiser — Hypothesis's `Optimiser` class: a mutable
/// borrow of the engine (which owns the [`TargetingState`] the climber
/// reads, and through which every trial is executed and recorded) plus the
/// budget caps for this firing.
pub(crate) struct Optimiser<'a, 'b> {
    pub engine: &'a mut Engine<'b>,
    pub max_valid: u64,
    pub max_calls: u64,
}

impl Optimiser<'_, '_> {
    fn budget_exhausted(&self) -> bool {
        !self.engine.interesting.is_empty()
            || self.engine.valid_test_cases >= self.max_valid
            || self.engine.calls >= self.max_calls
    }

    /// Run a single trial through the engine. Returns the run result if it
    /// completed, or `None` if the budget was exhausted before this call.
    ///
    /// Uses [`NativeTestCase::for_probe`] rather than `for_choices` so that
    /// perturbations which grow the realised choice sequence (e.g. raising
    /// an integer that controls a downstream loop count) can still draw the
    /// extra values from a fresh RNG instead of overrunning the prefix.
    /// Mirrors Hypothesis's `cached_test_function(choices, extend="full")`
    /// in `optimiser.py::attempt_replace`.
    fn run_trial(&mut self, choices: &[ChoiceValue]) -> Option<RunResult> {
        if self.budget_exhausted() {
            return None;
        }
        let ntc = NativeTestCase::for_probe(choices, self.engine.rng_spawn(), BUFFER_SIZE);
        // A non-determinism mismatch here is dropped: it's a generator
        // property, so the next generation-loop recording re-detects it and
        // returns a clean failure.
        let (run, _mismatch) = self.engine.test_function(ntc);
        Some(run)
    }

    /// Hill-climb every target until no further improvements are found or
    /// the budget is exhausted. Mirrors `engine.py::optimise_targets`.
    pub(crate) fn optimise_targets(&mut self) {
        let mut targets: Vec<String> = self
            .engine
            .targeting
            .best_observed_targets
            .keys()
            .cloned()
            .collect();
        // Iterate in a deterministic order: each hill-climb consumes the
        // shared call budget, so `HashMap`'s per-process-randomised key
        // order would make a seeded multi-target run non-reproducible.
        targets.sort();
        let mut max_improvements: usize = 10;
        loop {
            let prev_calls = self.engine.calls;
            let mut any_improvements = false;
            for target in &targets {
                let imps = self.hill_climb(target, max_improvements);
                if imps > 0 {
                    any_improvements = true;
                }
            }
            max_improvements = max_improvements.saturating_mul(2);
            if !any_improvements || prev_calls == self.engine.calls {
                return;
            }
        }
    }

    /// Walk the integer choices in `best_choices_for_target[target]` from
    /// the end backwards, hill-climbing each one in both directions.
    /// Mirrors `Optimiser._optimise_target`.
    fn hill_climb(&mut self, target: &str, max_improvements: usize) -> usize {
        // `record` keeps `best_choices_for_target` in sync with
        // `best_observed_targets`, so any label our caller iterates from
        // `best_observed_targets` must have a matching choice sequence here.
        let start_choices = self
            .engine
            .targeting
            .best_choices_for_target
            .get(target)
            .cloned()
            .expect("best_choices_for_target out of sync with best_observed_targets");
        let trial = match self.run_trial(&start_choices) {
            Some(t) => t,
            None => return 0,
        };
        if trial.status < Status::Valid {
            return 0;
        }
        let mut current_choices: Vec<ChoiceValue> =
            trial.nodes.iter().map(|n| n.value.clone()).collect();
        let mut current_spans = trial.spans;
        let mut current_nodes = trial.nodes;
        let mut current_score = *trial
            .target_observations
            .get(target)
            .unwrap_or(&f64::NEG_INFINITY);
        let mut improvements: usize = 0;

        let mut nodes_examined: HashSet<usize> = HashSet::new();
        let mut i: isize = current_nodes.len() as isize - 1;
        let mut prev_len = current_nodes.len();
        while i >= 0 && improvements <= max_improvements {
            // When `find_integer` lengthens or shortens `current_nodes`, `i`
            // no longer indexes the same logical position. Reset to the new
            // tail and start afresh — but keep `nodes_examined` populated so
            // indices we already optimised in the pre-resize pass don't get
            // redone. Mirrors `optimiser.py:95-97`.
            if current_nodes.len() != prev_len {
                i = current_nodes.len() as isize - 1;
                prev_len = current_nodes.len();
                continue;
            }
            let idx = i as usize;
            if !nodes_examined.insert(idx) {
                i -= 1;
                continue;
            }
            let node = &current_nodes[idx];
            if !node.was_forced && is_climbable(&node.value, node.kind.as_ref()) {
                let len_before = current_nodes.len();
                // Hill-climb in the +1 direction. `find_integer` itself is the
                // general `junkdrawer.find_integer` from `shrinker/mod.rs`: it
                // probes deltas 1, 2, 3, 4, then 5, 10, 20, … with binary
                // bisection in between, calling the closure once per probe.
                // The closure threads through all the per-climb state; its
                // return value (the largest accepted delta) is discarded —
                // commit happens as a side effect inside `try_replace`, and
                // the `improvements` counter it bumps is what drives the
                // outer `while improvements <= max_improvements` loop.
                find_integer(|k| {
                    self.try_replace(
                        target,
                        &mut current_choices,
                        &mut current_nodes,
                        &mut current_spans,
                        &mut current_score,
                        &mut improvements,
                        idx,
                        k as i128,
                    )
                });
                // If the +1 direction grew `current_nodes`, idx no longer points
                // at the same logical position; trying -1 there almost always
                // shrinks the sequence back below the new score, so skip.
                // Mirrors the same guard in Hypothesis's `Optimiser.hill_climb`.
                if idx < current_nodes.len() && current_nodes.len() == len_before {
                    find_integer(|k| {
                        self.try_replace(
                            target,
                            &mut current_choices,
                            &mut current_nodes,
                            &mut current_spans,
                            &mut current_score,
                            &mut improvements,
                            idx,
                            -(k as i128),
                        )
                    });
                }
            }
            i -= 1;
        }
        improvements
    }

    /// Replace `current_choices[idx]` by stepping it `delta` units —
    /// `optimiser.py::attempt_replace`. The direct replacement is tried up
    /// to three times; whenever an attempt changes the realised node count
    /// without scoring, the spans containing `idx` whose choice counts
    /// changed are realigned: the attempt's realised span content is
    /// spliced in front of the *old* suffix, repairing score-gating
    /// choices that the resize would otherwise consume or redraw.
    /// Returns `true` iff a trial was committed.
    #[allow(clippy::too_many_arguments)]
    fn try_replace(
        &mut self,
        target: &str,
        current_choices: &mut Vec<ChoiceValue>,
        current_nodes: &mut Vec<ChoiceNode>,
        current_spans: &mut Vec<Span>,
        current_score: &mut f64,
        improvements: &mut usize,
        idx: usize,
        delta: i128,
    ) -> bool {
        // Cap the perturbation magnitude to avoid driving an unbounded score
        // up forever. Mirrors `optimiser.py::attempt_replace` line 122.
        if delta.saturating_abs() > (1 << 20) {
            return false;
        }
        let new_val = match step_choice(&current_nodes[idx], delta) {
            Some(v) => v,
            None => return false,
        };
        for _ in 0..3 {
            let mut trial_choices = current_choices.clone();
            trial_choices[idx] = new_val.clone();
            let snapshot_choices = trial_choices.clone();
            let trial = match self.run_trial(&trial_choices) {
                Some(t) => t,
                None => return false,
            };
            let trial_len = trial.nodes.len();
            let trial_values: Vec<ChoiceValue> =
                trial.nodes.iter().map(|n| n.value.clone()).collect();
            let trial_spans = trial.spans.clone();
            if self.consider_new_data(
                target,
                trial,
                current_choices,
                current_nodes,
                current_spans,
                current_score,
                improvements,
            ) {
                return true;
            }
            // An overrun can't be repaired by realignment; a same-length
            // attempt didn't resize anything, so there is nothing to
            // realign either.
            if trial_len == current_nodes.len() {
                return false;
            }
            // Span realignment: for each span containing `idx` whose
            // choice count changed, splice the attempt's realised span
            // content in front of the preserved old suffix.
            for j in 0..current_spans.len() {
                let (ex_start, ex_end) = (current_spans[j].start, current_spans[j].end);
                if ex_start > idx {
                    break;
                }
                if ex_end <= idx || ex_end > current_choices.len() {
                    continue;
                }
                let Some(ex_attempt) = trial_spans.get(j) else {
                    continue;
                };
                let (at_start, at_end) = (ex_attempt.start, ex_attempt.end);
                if at_start > at_end || at_end > trial_values.len() {
                    continue;
                }
                if ex_end - ex_start == at_end - at_start {
                    continue;
                }
                let mut candidate = snapshot_choices[..idx].to_vec();
                candidate.extend_from_slice(&trial_values[at_start..at_end]);
                candidate.extend_from_slice(&current_choices[ex_end..]);
                let realigned = match self.run_trial(&candidate) {
                    Some(t) => t,
                    None => return false,
                };
                if self.consider_new_data(
                    target,
                    realigned,
                    current_choices,
                    current_nodes,
                    current_spans,
                    current_score,
                    improvements,
                ) {
                    return true;
                }
            }
        }
        false
    }

    /// Score acceptance for a hill-climb trial, mirroring
    /// `optimiser.py::consider_new_data` (lines 62-82): a strict score
    /// improvement commits the new state and bumps `improvements`; a tie
    /// commits iff the new node count doesn't grow but does *not* count as
    /// an improvement (lateral moves are the principal mechanism for
    /// escaping local maxima, but they shouldn't keep the climber spinning
    /// forever). Returns `true` iff the trial was committed.
    #[allow(clippy::too_many_arguments)]
    fn consider_new_data(
        &mut self,
        target: &str,
        trial: RunResult,
        current_choices: &mut Vec<ChoiceValue>,
        current_nodes: &mut Vec<ChoiceNode>,
        current_spans: &mut Vec<Span>,
        current_score: &mut f64,
        improvements: &mut usize,
    ) -> bool {
        if trial.status < Status::Valid {
            return false;
        }
        let new_score = *trial
            .target_observations
            .get(target)
            .unwrap_or(&f64::NEG_INFINITY);
        if new_score < *current_score {
            return false;
        }
        let strict = new_score > *current_score;
        if !strict && trial.nodes.len() > current_nodes.len() {
            return false;
        }
        *current_score = new_score;
        *current_choices = trial.nodes.iter().map(|n| n.value.clone()).collect();
        *current_nodes = trial.nodes;
        *current_spans = trial.spans;
        if strict {
            *improvements += 1;
        }
        true
    }
}

/// Returns `true` iff `(value, kind)` is a node kind the hill-climber can
/// step. Mirrors `optimiser.py:109`, which admits integer / float / bytes /
/// boolean and skips strings (no sensible "larger" step).
pub(crate) fn is_climbable(value: &ChoiceValue, kind: &ChoiceKind) -> bool {
    matches!(
        (value, kind),
        (ChoiceValue::Integer(_), ChoiceKind::Integer(_))
            | (ChoiceValue::Float(_), ChoiceKind::Float(_))
            | (ChoiceValue::Boolean(_), ChoiceKind::Boolean(_))
            | (ChoiceValue::Bytes(_), ChoiceKind::Bytes(_))
    )
}

/// Step a choice node by `delta` and return the resulting value if it's
/// representable and validates against the node's kind constraints, or
/// `None` to signal "this trial isn't worth running." Mirrors
/// `optimiser.py::Optimiser.attempt_replace` (lines 130-156) plus the
/// `choice_permitted(new_choice, node.constraints)` post-check.
pub(crate) fn step_choice(node: &ChoiceNode, delta: i128) -> Option<ChoiceValue> {
    match (&node.value, node.kind.as_ref()) {
        (ChoiceValue::Integer(v), ChoiceKind::Integer(kind)) => {
            let new = v + crate::native::bignum::BigInt::from(delta);
            Some(ChoiceValue::Integer(kind.value_from_bigint(&new)?))
        }
        (ChoiceValue::Float(v), ChoiceKind::Float(kind)) => {
            let new = v + delta as f64;
            if !kind.validate(new) {
                return None;
            }
            Some(ChoiceValue::Float(new))
        }
        (ChoiceValue::Boolean(b), ChoiceKind::Boolean(_)) => {
            if delta.saturating_abs() > 1 {
                return None;
            }
            let new = if delta == -1 {
                false
            } else if delta == 1 {
                true
            } else {
                *b
            };
            Some(ChoiceValue::Boolean(new))
        }
        (ChoiceValue::Bytes(b), ChoiceKind::Bytes(kind)) => {
            let mut v: i128 = 0;
            for &byte in b {
                v = (v << 8) | byte as i128;
            }
            let new_v = v.saturating_add(delta);
            if new_v < 0 {
                return None;
            }
            let mut new_bytes = Vec::new();
            let mut x = new_v;
            if x == 0 {
                new_bytes.push(0u8);
            }
            while x > 0 {
                new_bytes.push((x & 0xff) as u8);
                x >>= 8;
            }
            new_bytes.reverse();
            // Pad up to the original length so a shorter encoding doesn't
            // collapse the byte string. Mirrors upstream's
            // `max(len(node.value), bits_to_bytes(v.bit_length()))`.
            while new_bytes.len() < b.len() {
                new_bytes.insert(0, 0);
            }
            if !kind.validate(&new_bytes) {
                return None;
            }
            Some(ChoiceValue::Bytes(new_bytes))
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/targeting_tests.rs"]
mod tests;

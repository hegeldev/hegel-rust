use crate::backend::RunError;
use crate::control::hegel_internal_unwrap;
use crate::native::{HashMap, HashSet};
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::native::core::{ChoiceData, ChoiceNode, ChoiceValue, NativeTestCase, Status};
use crate::native::shrinker::search::FindInteger;
use crate::native::test_runner::{Engine, RunResult};

/// The best score observed for one label, together with the choice
/// sequence that produced it. Keeping the pair in one entry means the
/// score and its choices can never fall out of sync.
struct BestTarget {
    score: f64,
    choices: Vec<ChoiceValue>,
}

/// One label's targeting state under ND handling: the
/// reference timeline's realized nodes (the race perturbs these and derives
/// replayable values from them), a monotone reference score estimated only
/// from fresh unselected batches, and a dead marker for labels whose
/// reference batch observed no score at all — raced never, re-measured
/// never.
pub(crate) struct NdTarget {
    pub(crate) nodes: Vec<ChoiceNode>,
    pub(crate) reference: f64,
    pub(crate) dead: bool,
}

impl NdTarget {
    pub(crate) fn timeline(&self) -> Vec<ChoiceValue> {
        self.nodes.iter().map(|n| n.value()).collect()
    }
}

/// Per-label best score and the choice sequence that produced it, plus the
/// per-label ND state. The recorded best is a running maximum of single-run
/// observations, so under ND handling it is selection-biased and serves
/// only as seed material for honest measurement.
pub(crate) struct TargetingState {
    best_targets: HashMap<String, BestTarget>,
    nd_targets: HashMap<String, NdTarget>,
    /// Bumped every time a label's recorded best changes, so the
    /// [`TargetingSchedule`] can tell whether generation has found a new
    /// best since the last optimisation pass.
    version: u64,
}

impl TargetingState {
    pub fn new() -> Self {
        Self {
            best_targets: HashMap::default(),
            nd_targets: HashMap::default(),
            version: 0,
        }
    }

    /// Record the observations from a Valid run. The first observation for
    /// each label always populates the map; subsequent observations only
    /// overwrite when the score strictly improves.
    pub fn record(&mut self, choices: &[ChoiceValue], observations: &HashMap<String, f64>) {
        for (label, &score) in observations {
            let should_record = match self.best_targets.get(label) {
                None => true,
                Some(best) => score > best.score,
            };
            if should_record {
                self.best_targets.insert(
                    label.clone(),
                    BestTarget {
                        score,
                        choices: choices.to_vec(),
                    },
                );
                self.version += 1;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.best_targets.is_empty()
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    /// The highest score observed for every label, sorted by label.
    pub fn best_scores(&self) -> Vec<(String, f64)> {
        let mut scores: Vec<_> = self
            .best_targets
            .iter()
            .map(|(label, best)| (label.clone(), best.score))
            .collect();
        scores.sort_by(|a, b| a.0.cmp(&b.0));
        scores
    }

    /// Every label's recorded best, sorted by label — the ND race's seed
    /// material.
    pub(crate) fn seeds(&self) -> Vec<(String, f64, Vec<ChoiceValue>)> {
        let mut seeds: Vec<_> = self
            .best_targets
            .iter()
            .map(|(label, best)| (label.clone(), best.score, best.choices.clone()))
            .collect();
        seeds.sort_by(|a, b| a.0.cmp(&b.0));
        seeds
    }

    pub(crate) fn nd_target(&self, label: &str) -> Option<&NdTarget> {
        self.nd_targets.get(label)
    }

    pub(crate) fn set_nd_target(&mut self, label: String, target: NdTarget) {
        self.nd_targets.insert(label, target);
    }

    /// Move `label` onto an adopted winner's fresh-batch node view. The
    /// reference only ever rises, and always from a fresh unselected batch.
    pub(crate) fn adopt_nd(&mut self, label: &str, nodes: Vec<ChoiceNode>, median: f64) {
        let target = self
            .nd_targets
            .entry(String::from(label))
            .or_insert(NdTarget {
                nodes: Vec::new(),
                reference: f64::NEG_INFINITY,
                dead: false,
            });
        target.nodes = nodes;
        target.reference = target.reference.max(median);
        target.dead = false;
    }

    #[cfg(test)]
    pub(crate) fn best_score(&self, label: &str) -> Option<f64> {
        self.best_targets.get(label).map(|best| best.score)
    }
}

/// Schedule for firing the optimiser during the generation loop.
///
/// Mirrors `engine.py`'s `_should_optimise_now` / `_run_optimise_pass`.
/// A budget below [`INTERLEAVED_TARGETING_MIN_BUDGET`] gets a single pass
/// halfway through, which experience shows is roughly optimal when there
/// is little to spend. A larger budget starts optimising after a warm-up
/// of [`TARGETING_WARM_UP`] valid cases and then alternates passes with
/// random generation on a fair-share schedule: each pass is capped at a
/// slice of the remaining targeting budget (half the run's budget in
/// total), generation then runs for as many valid cases as the pass
/// spent, and a pass also fires early whenever generation raises a best
/// score since the previous pass settled. A pass that spent no valid
/// cases (ND targeting measures instead of generating) re-fires on the
/// halfway cadence.
pub(crate) struct TargetingSchedule {
    max_test_cases: u64,
    next_at: Option<u64>,
    spent: u64,
    version_at_last_pass: Option<u64>,
}

/// Budgets this large interleave targeting with generation from early in
/// the run instead of running one pass halfway through.
pub(crate) const INTERLEAVED_TARGETING_MIN_BUDGET: u64 = 1000;

/// Valid cases of random generation before the first interleaved pass.
pub(crate) const TARGETING_WARM_UP: u64 = 200;

impl TargetingSchedule {
    pub fn new(max_test_cases: u64) -> Self {
        let next_at = if max_test_cases < INTERLEAVED_TARGETING_MIN_BUDGET {
            let small_example_cap = (max_test_cases / 10).min(50);
            (max_test_cases / 2)
                .max(small_example_cap.saturating_add(1))
                .max(10)
        } else {
            TARGETING_WARM_UP
        };
        Self {
            max_test_cases,
            next_at: Some(next_at),
            spent: 0,
            version_at_last_pass: None,
        }
    }

    fn interleaved(&self) -> bool {
        self.max_test_cases >= INTERLEAVED_TARGETING_MIN_BUDGET
    }

    fn remaining(&self) -> u64 {
        (self.max_test_cases / 2).saturating_sub(self.spent)
    }

    /// Whether to run a pass now, given the valid count and the
    /// [`TargetingState::version`]. Every `true` must be followed by
    /// [`Self::pass_finished`], which schedules the next pass.
    pub fn should_fire(&self, valid_test_cases: u64, version: u64) -> bool {
        if self.next_at.is_some_and(|at| valid_test_cases >= at) {
            return true;
        }
        self.interleaved()
            && self.spent > 0
            && self.remaining() > 0
            && self.version_at_last_pass.is_some_and(|v| v != version)
    }

    /// The valid count at which a pass starting now must stop.
    pub fn pass_cap(&self, valid_test_cases: u64) -> u64 {
        if !self.interleaved() {
            return self.max_test_cases;
        }
        let remaining = self.remaining();
        valid_test_cases
            .saturating_add((remaining / 4).max(TARGETING_WARM_UP).min(remaining))
            .min(self.max_test_cases)
    }

    /// Account for a finished pass that spent `spent` valid cases and
    /// schedule the next one.
    pub fn pass_finished(
        &mut self,
        valid_test_cases: u64,
        spent: u64,
        improved: bool,
        version: u64,
    ) {
        self.spent = self.spent.saturating_add(spent);
        self.version_at_last_pass = Some(version);
        self.next_at = if spent == 0 {
            Some(valid_test_cases.saturating_add(self.max_test_cases / 2))
        } else if !self.interleaved() {
            None
        } else if improved && self.remaining() > 0 {
            Some(valid_test_cases.saturating_add(spent))
        } else {
            None
        };
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
        self.engine.origins.any_live()
            || self.engine.nd_active
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
    async fn run_trial(&mut self, choices: &[ChoiceValue]) -> Result<Option<RunResult>, RunError> {
        if self.budget_exhausted() {
            return Ok(None);
        }
        let bound = self.engine.choice_bound();
        let ntc = NativeTestCase::for_probe(choices, self.engine.rng_spawn(), bound)?;
        let (run, mismatch) = self.engine.test_function(ntc).await?;
        if let Some(err) = mismatch {
            return Err(err);
        }
        Ok(Some(run))
    }

    /// Hill-climb every target until no further improvements are found or
    /// the budget is exhausted, returning whether any score improved.
    /// Mirrors `engine.py::optimise_targets`.
    pub(crate) async fn optimise_targets(&mut self) -> Result<bool, RunError> {
        let mut targets: Vec<String> = self.engine.targeting.best_targets.keys().cloned().collect();
        targets.sort();
        let mut max_improvements: usize = 10;
        let mut improved = false;
        loop {
            let prev_calls = self.engine.calls;
            let mut any_improvements = false;
            for target in &targets {
                let imps = self.hill_climb(target, max_improvements).await?;
                if imps > 0 {
                    any_improvements = true;
                    improved = true;
                }
            }
            max_improvements = max_improvements.saturating_mul(2);
            if !any_improvements || prev_calls == self.engine.calls {
                return Ok(improved);
            }
        }
    }

    /// Walk the integer choices in `target`'s best-known choice sequence
    /// from the end backwards, hill-climbing each one in both directions.
    /// Mirrors `Optimiser._optimise_target`.
    async fn hill_climb(
        &mut self,
        target: &str,
        max_improvements: usize,
    ) -> Result<usize, RunError> {
        let recorded = self.engine.targeting.best_targets.get(target);
        let start_choices = hegel_internal_unwrap!(
            recorded,
            "hill_climb target {target:?} has no recorded best choices"
        )
        .choices
        .clone();
        let trial = match self.run_trial(&start_choices).await? {
            Some(t) => t,
            None => return Ok(0),
        };
        if trial.status < Status::Valid {
            return Ok(0);
        }
        let mut current_choices: Vec<ChoiceValue> = trial.nodes.iter().map(|n| n.value()).collect();
        let mut current_nodes = trial.nodes;
        let mut current_score = *trial
            .target_observations
            .get(target)
            .unwrap_or(&f64::NEG_INFINITY);
        let mut improvements: usize = 0;

        let mut nodes_examined: HashSet<usize> = HashSet::default();
        let mut i: isize = current_nodes.len() as isize - 1;
        let mut prev_len = current_nodes.len();
        while i >= 0 && improvements <= max_improvements {
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
            if !node.was_forced && is_climbable(&node.data) {
                let len_before = current_nodes.len();
                let mut search = FindInteger::new();
                while let Some(k) = search.probe() {
                    let ok = self
                        .try_replace(
                            target,
                            &mut current_choices,
                            &mut current_nodes,
                            &mut current_score,
                            &mut improvements,
                            idx,
                            k as i128,
                        )
                        .await?;
                    search.record(ok);
                }
                if idx < current_nodes.len() && current_nodes.len() == len_before {
                    let mut search = FindInteger::new();
                    while let Some(k) = search.probe() {
                        let ok = self
                            .try_replace(
                                target,
                                &mut current_choices,
                                &mut current_nodes,
                                &mut current_score,
                                &mut improvements,
                                idx,
                                -(k as i128),
                            )
                            .await?;
                        search.record(ok);
                    }
                }
            }
            i -= 1;
        }
        Ok(improvements)
    }

    /// Replace `current_choices[idx]` by stepping it `delta` units. Score
    /// acceptance mirrors `optimiser.py::consider_new_data` (lines 65-82): a
    /// strict score improvement commits the new state and bumps `improvements`;
    /// a tie commits iff the new node count doesn't grow but does *not* count
    /// as an improvement (lateral moves are the principal mechanism for
    /// escaping local maxima, but they shouldn't keep the climber spinning
    /// forever). Returns `true` iff the trial was committed.
    async fn try_replace(
        &mut self,
        target: &str,
        current_choices: &mut Vec<ChoiceValue>,
        current_nodes: &mut Vec<ChoiceNode>,
        current_score: &mut f64,
        improvements: &mut usize,
        idx: usize,
        delta: i128,
    ) -> Result<bool, RunError> {
        if delta.saturating_abs() > (1 << 20) {
            return Ok(false);
        }
        let new_val = match step_choice(&current_nodes[idx], delta) {
            Some(v) => v,
            None => return Ok(false),
        };
        let mut trial_choices = current_choices.clone();
        trial_choices[idx] = new_val;
        let trial = match self.run_trial(&trial_choices).await? {
            Some(t) => t,
            None => return Ok(false),
        };
        if trial.status < Status::Valid {
            return Ok(false);
        }
        let new_score = *trial
            .target_observations
            .get(target)
            .unwrap_or(&f64::NEG_INFINITY);
        if new_score < *current_score {
            return Ok(false);
        }
        let strict = new_score > *current_score;
        if !strict && trial.nodes.len() > current_nodes.len() {
            return Ok(false);
        }
        *current_score = new_score;
        *current_choices = trial.nodes.iter().map(|n| n.value()).collect();
        *current_nodes = trial.nodes;
        if strict {
            *improvements += 1;
        }
        Ok(true)
    }
}

/// The end-of-run report of the highest score observed per label, in
/// Hypothesis's `describe_targets` layout: one line for a single label, a
/// table sorted by score for several. Empty when nothing was targeted.
pub(crate) fn describe_targets(best_scores: &[(String, f64)]) -> Vec<String> {
    match best_scores {
        [] => Vec::new(),
        [(label, score)] => vec![format!("Highest target score: {score}  (label={label:?})")],
        _ => {
            let mut sorted = best_scores.to_vec();
            sorted.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            let mut lines = vec![String::from("Highest target scores:")];
            for (label, score) in sorted {
                lines.push(format!("{score:>16}  (label={label:?})"));
            }
            lines
        }
    }
}

/// Returns `true` iff `data` is a node kind the hill-climber can step.
/// Mirrors `optimiser.py:109`, which admits integer / float / bytes /
/// boolean and skips strings (no sensible "larger" step).
pub(crate) fn is_climbable(data: &ChoiceData) -> bool {
    matches!(
        data,
        ChoiceData::Integer(..)
            | ChoiceData::Float(..)
            | ChoiceData::Boolean(..)
            | ChoiceData::Bytes(..)
    )
}

/// Step a choice node by `delta` and return the resulting value if it's
/// representable and validates against the node's kind constraints, or
/// `None` to signal "this trial isn't worth running." Mirrors
/// `optimiser.py::Optimiser.attempt_replace` (lines 130-156) plus the
/// `choice_permitted(new_choice, node.constraints)` post-check.
pub(crate) fn step_choice(node: &ChoiceNode, delta: i128) -> Option<ChoiceValue> {
    match &node.data {
        ChoiceData::Integer(kind, v) => {
            let new = v + crate::native::bignum::BigInt::from(delta);
            Some(ChoiceValue::Integer(kind.value_from_bigint(&new)?))
        }
        ChoiceData::Float(kind, v) => {
            let new = v + delta as f64;
            if !kind.validate(new) {
                return None;
            }
            Some(ChoiceValue::Float(new))
        }
        ChoiceData::Boolean(_, b) => {
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
        ChoiceData::Bytes(kind, b) => {
            use crate::native::bignum::{BigInt, Sign};
            let v = BigInt::from_bytes_be(Sign::Plus, b);
            let new_v = v + BigInt::from(delta);
            if new_v.sign() == Sign::Minus {
                return None;
            }
            let (_, mut new_bytes) = new_v.to_bytes_be();
            if new_bytes.is_empty() {
                new_bytes.push(0u8);
            }
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

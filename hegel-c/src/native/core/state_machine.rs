use crate::native::HashSet;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::choices::EngineError;
use super::state::NativeTestCase;
use crate::control::{InternalError, hegel_internal_assert};
use crate::hegel_label_t::HEGEL_LABEL_FEATURE_FLAG;
use crate::native::bignum::{BigInt, ToPrimitive};

/// Probability that the per-step stop decision in
/// [`NativeStateMachine::next_rule`] halts a stateful test case, when the
/// engine is free to choose (2^-16).
const P_STOP: f64 = 1.0 / 65536.0;

/// Multiplier on `stateful_step_count` bounding the total rules handed out
/// (including rejected ones), so a machine whose rules mostly fail their
/// assumptions cannot retry indefinitely.
const ATTEMPT_MULTIPLIER: i64 = 10;

/// Minimum attempt budget for a machine that has not completed a single
/// rule yet, so machines with very selective assumptions still get a real
/// chance to make progress under a small step count.
const MIN_ATTEMPTS_WITHOUT_SUCCESS: i64 = 1000;

/// Draw a uniform index in `[0, n)`.
fn draw_index(ntc: &mut NativeTestCase, n: usize) -> Result<usize, EngineError> {
    let i = ntc.draw_integer(BigInt::from(0), BigInt::from(n as i64 - 1))?;
    Ok(i.to_i128().unwrap() as usize)
}

/// Per-test-case feature flags over rule indices, deciding which rules are
/// enabled for the current test case.
///
/// The disabling probability is decided up front so that any subset from
/// all-enabled down to a single surviving rule is reachable (all-disabled is
/// not: see `at_least_one_of`); rules are then decided lazily as they are
/// first asked about. Decided flags are re-recorded as forced draws on later
/// queries, so deleting the original deciding draw during shrinking just
/// moves the decision to the next query point.
struct FeatureFlags {
    p_disabled: f64,
    /// Decision per rule index; `None` until first queried.
    is_disabled: Vec<Option<bool>>,
    /// Rule indices still candidates for the "at least one rule enabled"
    /// guarantee. Starts as all rules; emptied when any member is enabled.
    /// When it shrinks to a single undecided candidate, that rule is forced
    /// enabled — disabling every rule would make the test unable to
    /// progress.
    at_least_one_of: HashSet<usize>,
}

impl FeatureFlags {
    fn new(ntc: &mut NativeTestCase, num_rules: usize) -> Result<Self, EngineError> {
        let raw = ntc.draw_integer(BigInt::from(0), BigInt::from(254))?;
        Ok(FeatureFlags {
            p_disabled: raw.to_i128().unwrap() as f64 / 255.0,
            is_disabled: vec![None; num_rules],
            at_least_one_of: (0..num_rules).collect(),
        })
    }

    fn is_enabled(&mut self, ntc: &mut NativeTestCase, i: usize) -> Result<bool, EngineError> {
        ntc.start_span(HEGEL_LABEL_FEATURE_FLAG as u64);
        let forced = if self.at_least_one_of.len() == 1 && self.at_least_one_of.contains(&i) {
            Some(false)
        } else {
            self.is_disabled[i]
        };
        let is_disabled = ntc.weighted(self.p_disabled, forced)?;
        self.is_disabled[i] = Some(is_disabled);
        if !is_disabled {
            self.at_least_one_of.clear();
        }
        self.at_least_one_of.remove(&i);
        ntc.stop_span(false);
        Ok(!is_disabled)
    }
}

/// Engine-side driver for a single stateful (rule-based) test case.
///
/// The test body registers a fixed set of rules and asks the engine which
/// rule to run at each step.
#[derive(Default)]
pub struct NativeStateMachine {
    rule_names: Vec<String>,
    /// Registered for future use (e.g. per-invariant metrics); the engine does
    /// not drive invariant execution.
    #[allow(dead_code)]
    invariant_names: Vec<String>,
    flags: Option<FeatureFlags>,
    /// Number of rules handed out so far, including rejected ones.
    steps_drawn: i64,
    /// Number of handed-out rules reported back as rejected via
    /// [`Self::rule_rejected`].
    steps_rejected: i64,
    /// Whether the most recently handed-out rule has already been reported
    /// as rejected.
    current_rule_rejected: bool,
}

impl NativeStateMachine {
    pub fn new(
        rule_names: Vec<String>,
        invariant_names: Vec<String>,
    ) -> Result<Self, InternalError> {
        hegel_internal_assert!(
            !rule_names.is_empty(),
            "Stateful testing: there must be at least one rule"
        );

        Ok(NativeStateMachine {
            rule_names,
            invariant_names,
            flags: None,
            steps_drawn: 0,
            steps_rejected: 0,
            current_rule_rejected: false,
        })
    }

    /// Draw the index of the next rule to run, in `[0, num_rules)`, or
    /// `None` once the test case decides to stop.
    ///
    /// Each call first makes a per-step stop decision: a boolean draw with
    /// probability [`P_STOP`] of halting. Every test case runs at least one
    /// step and at most `stateful_step_count` successful steps — rules
    /// reported as rejected ([`Self::rule_rejected`]) do not count. Total
    /// attempts including rejected rules are bounded by
    /// [`ATTEMPT_MULTIPLIER`] times the step count, or
    /// [`MIN_ATTEMPTS_WITHOUT_SUCCESS`] while no rule has succeeded.
    /// Families marked as unbounded (single-test-case runs) never force a
    /// stop.
    pub fn next_rule(&mut self, ntc: &mut NativeTestCase) -> Result<Option<i64>, EngineError> {
        let successful_steps = self.steps_drawn - self.steps_rejected;
        let step_count = ntc.family().stateful_step_count();
        let attempt_cap = if successful_steps == 0 {
            step_count
                .saturating_mul(ATTEMPT_MULTIPLIER)
                .max(MIN_ATTEMPTS_WITHOUT_SUCCESS)
        } else {
            step_count.saturating_mul(ATTEMPT_MULTIPLIER)
        };
        let forced = if ntc.family().state_machine_steps_unbounded() {
            Some(false)
        } else if successful_steps >= step_count || self.steps_drawn >= attempt_cap {
            Some(true)
        } else if self.steps_drawn == 0 {
            Some(false)
        } else {
            None
        };
        if ntc.weighted_precise(P_STOP, forced)? {
            return Ok(None);
        }
        let index = self.select_rule(ntc)?;
        self.steps_drawn += 1;
        self.current_rule_rejected = false;
        Ok(Some(index))
    }

    /// Record that the most recently handed-out rule was rejected before it
    /// completed (a violated assumption), so it does not count toward the
    /// test case's successful-step budget.
    ///
    /// Errors with `InvalidArgument` when no rule is outstanding: either no
    /// rule has been drawn yet, or the current rule was already rejected.
    pub fn rule_rejected(&mut self) -> Result<(), EngineError> {
        if self.steps_drawn == 0 || self.current_rule_rejected {
            return Err(EngineError::InvalidArgument(
                "rule_rejected called with no outstanding rule".to_string(),
            ));
        }
        self.current_rule_rejected = true;
        self.steps_rejected += 1;
        Ok(())
    }

    /// Select the next rule's index, in `[0, num_rules)`.
    ///
    /// Up to three rejection-sampling tries, then a fallback that
    /// enumerates the enabled rules.
    fn select_rule(&mut self, ntc: &mut NativeTestCase) -> Result<i64, EngineError> {
        let n = self.rule_names.len();
        if self.flags.is_none() {
            self.flags = Some(FeatureFlags::new(ntc, n)?);
        }
        let flags = self.flags.as_mut().unwrap();

        let mut known_bad: HashSet<usize> = HashSet::default();
        for _ in 0..3 {
            let i = draw_index(ntc, n)?;
            if !known_bad.contains(&i) {
                if flags.is_enabled(ntc, i)? {
                    return Ok(i as i64);
                }
                known_bad.insert(i);
            }
        }

        let max_good = n - known_bad.len();
        let speculative = draw_index(ntc, max_good)?;
        let mut allowed: Vec<usize> = Vec::new();
        for i in 0..n {
            if known_bad.contains(&i) {
                continue;
            }
            if flags.is_enabled(ntc, i)? {
                allowed.push(i);
                if allowed.len() > speculative {
                    ntc.draw_integer_forced(
                        BigInt::from(0),
                        BigInt::from(n as i64 - 1),
                        BigInt::from(i as i64),
                    )?;
                    return Ok(i as i64);
                }
            }
        }
        hegel_internal_assert!(!allowed.is_empty());
        let k = draw_index(ntc, allowed.len())?;
        let i = allowed[k];
        ntc.draw_integer_forced(
            BigInt::from(0),
            BigInt::from(n as i64 - 1),
            BigInt::from(i as i64),
        )?;
        Ok(i as i64)
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/state_machine_tests.rs"]
mod tests;

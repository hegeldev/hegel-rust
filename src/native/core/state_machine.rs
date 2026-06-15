// Engine-owned state machines: swarm rule selection for stateful testing.
//
// Each test case enables a random subset
// of rules ("swarm testing", Groce et al. 2012); rule selection then draws
// only from the enabled subset. The flags shrink open: the disabling
// probability shrinks to zero, so swarm restrictions vanish from minimal
// counterexamples.

use std::collections::HashSet;

use super::choices::EngineError;
use super::state::NativeTestCase;
use crate::control::hegel_internal_assert;
use crate::native::bignum::{BigInt, ToPrimitive};
use crate::test_case::{invalid_argument, labels};

/// Draw a uniform index in `[0, n)`.
fn draw_index(ntc: &mut NativeTestCase, n: usize) -> Result<usize, EngineError> {
    let i = ntc.draw_integer(BigInt::from(0), BigInt::from(n as i64 - 1))?;
    Ok(i.to_i128().unwrap() as usize)
}

/// Per-test-case feature flags over rule indices, deciding which rules are
/// enabled for the current test case.
///
/// The disabling probability is decided up front so that
/// all-enabled and all-disabled subsets are reachable; rules are then
/// decided lazily as they are first asked about. Decided flags are
/// re-recorded as forced draws on later queries, so deleting the original
/// deciding draw during shrinking just moves the decision to the next
/// query point.
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
        // Drawn as an integer in [0, 254] so the shrunk value 0 enables
        // everything: we shrink in the direction of more rules enabled.
        let raw = ntc.draw_integer(BigInt::from(0), BigInt::from(254))?;
        Ok(FeatureFlags {
            p_disabled: raw.to_i128().unwrap() as f64 / 255.0,
            is_disabled: vec![None; num_rules],
            at_least_one_of: (0..num_rules).collect(),
        })
    }

    fn is_enabled(&mut self, ntc: &mut NativeTestCase, i: usize) -> Result<bool, EngineError> {
        ntc.start_span(labels::FEATURE_FLAG);
        let forced = if self.at_least_one_of.len() == 1 && self.at_least_one_of.contains(&i) {
            Some(false)
        } else {
            self.is_disabled[i]
        };
        // On Err the span is left open; `freeze` closes intervals left open
        // by an overrun.
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
}

impl NativeStateMachine {
    pub fn new(rule_names: Vec<String>, invariant_names: Vec<String>) -> Self {
        if rule_names.is_empty() {
            invalid_argument!("Stateful testing: there must be at least one rule");
        }

        NativeStateMachine {
            rule_names,
            invariant_names,
            flags: None,
        }
    }

    /// Draw the index of the next rule to run, in `[0, num_rules)`.
    ///
    /// Up to three rejection-sampling tries, then a fallback that
    /// enumerates the enabled rules.
    pub fn next_rule(&mut self, ntc: &mut NativeTestCase) -> Result<i64, EngineError> {
        let n = self.rule_names.len();
        if self.flags.is_none() {
            self.flags = Some(FeatureFlags::new(ntc, n)?);
        }
        let flags = self.flags.as_mut().unwrap();

        // Ordinary rejection sampling first: fast when most rules are
        // enabled, cheap when it fails.
        let mut known_bad: HashSet<usize> = HashSet::new();
        for _ in 0..3 {
            let i = draw_index(ntc, n)?;
            if !known_bad.contains(&i) {
                if flags.is_enabled(ntc, i)? {
                    return Ok(i as i64);
                }
                known_bad.insert(i);
            }
        }

        // Fallback: enumerate the remaining candidates. Before we know how
        // many are enabled, speculatively draw a position into the enabled
        // list so we can early-exit the enumeration at that point; if the
        // speculative position turns out to be past the end of the list,
        // draw again from the now-known list. Either way, record the chosen
        // rule index as a forced draw in the same domain as the
        // rejection-sampling tries, so shrinking and replay see a uniform
        // node shape.
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
        // Guaranteed by at_least_one_of: enumerating every rule decides them
        // all, and the last undecided candidate is forced enabled.
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

use std::cmp::min;
use std::collections::HashSet;

use super::choices::EngineError;
use super::state::NativeTestCase;
use crate::control::hegel_internal_assert;
use crate::hegel_label_t::HEGEL_LABEL_FEATURE_FLAG;
use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::draws;

/// Upper bound on the round cap drawn by [`NativeStateMachine::next_group`]
/// at concurrency 1, where each round hands out exactly one rule — so a
/// sequential test gets roughly the same total number of steps as the
/// pre-concurrency engine's per-test-case step cap.
const MAX_SEQUENTIAL_ROUND_CAP: i64 = 50;

/// Upper bound on the round cap drawn by [`NativeStateMachine::next_group`]
/// at concurrency > 1. Together with [`MAX_ROUND_STEP_CAP`] this keeps each
/// worker thread's total step budget per test case comparable to a
/// sequential test's ([`MAX_SEQUENTIAL_ROUND_CAP`]).
const MAX_CONCURRENT_ROUND_CAP: i64 = 10;

/// Upper bound on the per-thread step cap drawn each round by
/// [`NativeStateMachine::next_rule`] at concurrency > 1.
const MAX_ROUND_STEP_CAP: i64 = 5;

/// Draw a uniform index in `[0, n)`.
fn draw_index(ntc: &mut NativeTestCase, n: usize) -> Result<usize, EngineError> {
    let i = ntc.draw_integer(BigInt::from(0), BigInt::from(n as i64 - 1))?;
    Ok(i.to_i128().unwrap() as usize)
}

/// Draw a cap in `[1, max_cap]`: an integer in `[1, i64::MAX]` truncated to
/// `max_cap` (so usually exactly `max_cap`, but shrinkable all the way down
/// to 1).
fn draw_cap(ntc: &mut NativeTestCase, max_cap: i64) -> Result<i64, EngineError> {
    let raw = draws::generate_integer(ntc, &BigInt::from(1), &BigInt::from(i64::MAX))?;
    Ok(min(raw.to_i128().unwrap() as i64, max_cap))
}

/// Per-thread feature flags over rule indices, deciding which rules are
/// enabled for the calling thread over the whole test case.
///
/// The disabling probability is decided up front so that any subset from
/// all-enabled down to a single surviving rule per group is reachable
/// (all-disabled is not: see `at_least_one_of`); rules are then decided
/// lazily as they are first asked about. Decided flags are re-recorded as
/// forced draws on later queries, so deleting the original deciding draw
/// during shrinking just moves the decision to the next query point.
struct FeatureFlags {
    p_disabled: f64,
    /// Decision per global rule index; `None` until first queried.
    is_disabled: Vec<Option<bool>>,
    /// Per concurrency group: the global rule indices still candidates for
    /// that group's "at least one rule enabled" guarantee. Each starts as
    /// the group's full membership and is emptied when any member is
    /// enabled. When a group's set shrinks to a single undecided candidate,
    /// that rule is forced enabled — disabling every rule of a group would
    /// leave rounds on that group unable to progress.
    at_least_one_of: Vec<HashSet<usize>>,
}

impl FeatureFlags {
    fn new(
        ntc: &mut NativeTestCase,
        groups: &[Vec<usize>],
        num_rules: usize,
    ) -> Result<Self, EngineError> {
        let raw = ntc.draw_integer(BigInt::from(0), BigInt::from(254))?;
        Ok(FeatureFlags {
            p_disabled: raw.to_i128().unwrap() as f64 / 255.0,
            is_disabled: vec![None; num_rules],
            at_least_one_of: groups
                .iter()
                .map(|members| members.iter().copied().collect())
                .collect(),
        })
    }

    fn is_enabled(
        &mut self,
        ntc: &mut NativeTestCase,
        group: usize,
        i: usize,
    ) -> Result<bool, EngineError> {
        ntc.start_span(HEGEL_LABEL_FEATURE_FLAG as u64);
        let candidates = &self.at_least_one_of[group];
        let forced = if candidates.len() == 1 && candidates.contains(&i) {
            Some(false)
        } else {
            self.is_disabled[i]
        };
        let is_disabled = ntc.weighted(self.p_disabled, forced)?;
        self.is_disabled[i] = Some(is_disabled);
        if !is_disabled {
            self.at_least_one_of[group].clear();
        }
        self.at_least_one_of[group].remove(&i);
        ntc.stop_span(false);
        Ok(!is_disabled)
    }
}

/// Per-thread-index state, fully constructed at machine creation and
/// refreshed in place at every join point — so `next_rule` only ever reads
/// state that already exists.
///
/// The flags' disabling probability and the per-round step caps are drawn
/// from the *creating* handle's stream (at machine creation and in
/// `next_group` respectively), both quiescent moments, so draws on one
/// worker thread never affect draws on another; the per-rule enable
/// decisions inside [`FeatureFlags`] stay lazy and are drawn from the
/// querying thread's own stream.
struct ThreadState {
    /// Swarm feature flags, persisting for the whole test case.
    flags: FeatureFlags,
    /// This round's step cap, written by `next_group` at every join point
    /// (always 1 at concurrency 1; drawn at concurrency > 1).
    step_cap: i64,
    /// Rules handed to this thread so far this round; reset by `next_group`.
    steps_drawn: i64,
}

/// Engine-side driver for a single stateful (rule-based) test case,
/// sequential or concurrent.
///
/// The test body registers a fixed set of rules — each belonging to exactly
/// one concurrency group — plus the invariants and the concurrency level,
/// and drives execution in rounds: the root handle asks [`Self::next_group`]
/// whether to run another round (and which group is current), then each
/// worker thread pulls rules for that round via [`Self::next_rule`] until it
/// returns `None`. Rules in the same group may run concurrently; rules in
/// different groups never overlap, because only the current group's rules
/// are handed out and the group changes only at the join points between
/// rounds. A sequential machine is the special case of one group and
/// concurrency 1, where each round hands out exactly one rule.
pub struct NativeStateMachine {
    /// Registered for future use (e.g. diagnostics); selection works on the
    /// per-group index lists below.
    #[allow(dead_code)]
    rule_names: Vec<String>,
    /// Registered for future use (e.g. per-invariant metrics); the engine does
    /// not drive invariant execution.
    #[allow(dead_code)]
    invariant_names: Vec<String>,
    /// Registered for future use (e.g. diagnostics); selection works on
    /// group indices only.
    #[allow(dead_code)]
    group_names: Vec<String>,
    /// Per group: the global indices of its member rules, in registration
    /// order. Selection draws range over the current group's list only, so
    /// every selection is in-group by construction.
    groups: Vec<Vec<usize>>,
    concurrency: i64,
    /// The group whose rules are handed out this round, written by every
    /// `next_group` call. Meaningful only once `rounds_started > 0`;
    /// `next_rule` rejects calls made before the first round.
    current_group: usize,
    /// Per-test-case cap on the number of rounds, drawn at machine creation
    /// from the creating handle's stream. Zero — and never consulted — for
    /// families marked as unbounded at creation.
    round_cap: i64,
    rounds_started: i64,
    threads: Vec<ThreadState>,
}

impl NativeStateMachine {
    /// Create a machine, fully constructed: the round cap and every
    /// thread's swarm disabling probability are drawn here, from the
    /// creating handle's stream, so no per-thread state is ever pending.
    /// For families marked as unbounded (single-test-case runs) no round
    /// cap is drawn: rounds continue forever.
    pub fn new(
        ntc: &mut NativeTestCase,
        group_names: Vec<String>,
        rule_names: Vec<String>,
        rule_groups: Vec<usize>,
        invariant_names: Vec<String>,
        concurrency: i64,
    ) -> Result<Self, EngineError> {
        hegel_internal_assert!(
            !rule_names.is_empty(),
            "Stateful testing: there must be at least one rule"
        );
        hegel_internal_assert!(
            !group_names.is_empty(),
            "Stateful testing: there must be at least one concurrency group"
        );
        hegel_internal_assert!(
            rule_groups.len() == rule_names.len(),
            "Stateful testing: rule_groups must be parallel to rule_names"
        );
        hegel_internal_assert!(
            concurrency >= 1,
            "Stateful testing: concurrency must be at least 1"
        );

        let mut groups: Vec<Vec<usize>> = vec![Vec::new(); group_names.len()];
        for (rule, &group) in rule_groups.iter().enumerate() {
            hegel_internal_assert!(
                group < group_names.len(),
                "Stateful testing: rule group index out of range"
            );
            groups[group].push(rule);
        }
        for members in &groups {
            hegel_internal_assert!(
                !members.is_empty(),
                "Stateful testing: every concurrency group must have at least one rule"
            );
        }

        let round_cap = if ntc.family().state_machine_steps_unbounded() {
            0
        } else {
            let max_cap = if concurrency == 1 {
                MAX_SEQUENTIAL_ROUND_CAP
            } else {
                MAX_CONCURRENT_ROUND_CAP
            };
            draw_cap(ntc, max_cap)?
        };
        let threads = (0..concurrency)
            .map(|_| {
                Ok(ThreadState {
                    flags: FeatureFlags::new(ntc, &groups, rule_names.len())?,
                    step_cap: 0,
                    steps_drawn: 0,
                })
            })
            .collect::<Result<Vec<ThreadState>, EngineError>>()?;
        Ok(NativeStateMachine {
            rule_names,
            invariant_names,
            group_names,
            groups,
            concurrency,
            current_group: 0,
            round_cap,
            rounds_started: 0,
            threads,
        })
    }

    /// Start the next round: draw whether another round should run at all
    /// and, if so, which concurrency group is current for it and each
    /// thread's step budget. Returns the current group's index, or `None`
    /// once the test case has run enough rounds.
    ///
    /// Must be called from the root handle at each join point, including
    /// before the first `next_rule` call. Families marked as unbounded at
    /// creation (single-test-case runs) never return `None`: rounds
    /// continue forever.
    pub fn next_group(&mut self, ntc: &mut NativeTestCase) -> Result<Option<usize>, EngineError> {
        if !ntc.family().state_machine_steps_unbounded() && self.rounds_started >= self.round_cap {
            return Ok(None);
        }
        let group = if self.groups.len() == 1 {
            0
        } else {
            draw_index(ntc, self.groups.len())?
        };
        for thread_idx in 0..self.threads.len() {
            let step_cap = if self.concurrency == 1 {
                1
            } else {
                draw_cap(ntc, MAX_ROUND_STEP_CAP)?
            };
            let thread = &mut self.threads[thread_idx];
            thread.step_cap = step_cap;
            thread.steps_drawn = 0;
        }
        self.current_group = group;
        self.rounds_started += 1;
        Ok(Some(group))
    }

    /// Draw the index of the next rule for `thread_index` to run this round
    /// — always a rule belonging to the current group, in
    /// `[0, num_rules)` — or `None` once the thread's round budget is
    /// exhausted and it should wait for the next join point.
    ///
    /// Consults only per-thread state (plus the machine's current group), so
    /// draws on one thread never affect draws on another. At concurrency 1
    /// every round's budget is exactly one rule, so a join point follows
    /// each rule.
    pub fn next_rule(
        &mut self,
        ntc: &mut NativeTestCase,
        thread_index: i64,
    ) -> Result<Option<i64>, EngineError> {
        let thread_idx = usize::try_from(thread_index)
            .ok()
            .filter(|&t| t < self.threads.len())
            .ok_or_else(|| {
                EngineError::InvalidArgument(format!(
                    "thread_index must be in [0, {}), got {thread_index}",
                    self.concurrency
                ))
            })?;
        if self.rounds_started == 0 {
            return Err(EngineError::InvalidArgument(
                "state machine rule requested before the first next_group call".to_string(),
            ));
        }

        if self.threads[thread_idx].steps_drawn >= self.threads[thread_idx].step_cap {
            return Ok(None);
        }
        let index = self.select_rule(ntc, thread_idx, self.current_group)?;
        self.threads[thread_idx].steps_drawn += 1;
        Ok(Some(index))
    }

    /// Select the next rule's global index from the current group's member
    /// list.
    ///
    /// Every selection draw is an index in `[0, group_size)` mapped back to
    /// the global rule index, so each selection is in-group by construction.
    /// Up to three rejection-sampling tries against the thread's swarm
    /// flags, then a fallback that enumerates the group's enabled rules.
    fn select_rule(
        &mut self,
        ntc: &mut NativeTestCase,
        thread_idx: usize,
        group: usize,
    ) -> Result<i64, EngineError> {
        let members = &self.groups[group];
        let n = members.len();
        let flags = &mut self.threads[thread_idx].flags;

        let mut known_bad: HashSet<usize> = HashSet::new();
        for _ in 0..3 {
            let k = draw_index(ntc, n)?;
            if !known_bad.contains(&k) {
                if flags.is_enabled(ntc, group, members[k])? {
                    return Ok(members[k] as i64);
                }
                known_bad.insert(k);
            }
        }

        let max_good = n - known_bad.len();
        let speculative = draw_index(ntc, max_good)?;
        let mut allowed: Vec<usize> = Vec::new();
        for (k, &rule) in members.iter().enumerate() {
            if known_bad.contains(&k) {
                continue;
            }
            if flags.is_enabled(ntc, group, rule)? {
                allowed.push(k);
                if allowed.len() > speculative {
                    ntc.draw_integer_forced(
                        BigInt::from(0),
                        BigInt::from(n as i64 - 1),
                        BigInt::from(k as i64),
                    )?;
                    return Ok(rule as i64);
                }
            }
        }
        hegel_internal_assert!(!allowed.is_empty());
        let j = draw_index(ntc, allowed.len())?;
        let k = allowed[j];
        ntc.draw_integer_forced(
            BigInt::from(0),
            BigInt::from(n as i64 - 1),
            BigInt::from(k as i64),
        )?;
        Ok(members[k] as i64)
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/state_machine_tests.rs"]
mod tests;

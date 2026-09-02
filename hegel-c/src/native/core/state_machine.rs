use crate::native::HashSet;
use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use super::choices::EngineError;
use super::state::NativeTestCase;
use crate::control::hegel_internal_assert;
use crate::hegel_label_t::HEGEL_LABEL_FEATURE_FLAG;
use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::draws;

/// Probability that the per-round stop decision in
/// [`NativeStateMachine::next_group`] halts a stateful test case, when the
/// engine is free to choose (2^-16).
const P_STOP: f64 = 1.0 / 65536.0;

/// Multiplier bounding attempts against successful work: on rounds
/// (relative to `stateful_step_count`) so a sequential machine whose rules
/// mostly fail their assumptions cannot retry indefinitely, and on each
/// worker's per-round rule attempts (relative to [`MAX_ROUND_RULES`]) so a
/// worker whose rules keep rejecting cannot stall its round forever.
const ATTEMPT_MULTIPLIER: i64 = 10;

/// Minimum round-attempt budget for a machine that has not completed a
/// single rule yet, so machines with very selective assumptions still get a
/// real chance to make progress under a small step count.
const MIN_ATTEMPTS_WITHOUT_SUCCESS: i64 = 1000;

/// Upper bound on the rules each worker runs per round at concurrency > 1.
/// The count is uniform in `[0, MAX_ROUND_RULES]`, decided one boolean draw
/// at a time (see [`NativeStateMachine::next_rule`]).
const MAX_ROUND_RULES: i64 = 5;

/// Probability that [`draw_concurrency`] draws `max_value` outright rather
/// than a uniform level in `[min_value, max_value]`.
const P_MAX_CONCURRENCY: f64 = 0.75;

/// Draw the machine's concurrency level in `[min_value, max_value]`.
///
/// The distribution is weighted toward `max_value` (concurrency bugs need
/// concurrency) rather than shrink-biased toward `min_value`: with
/// probability [`P_MAX_CONCURRENCY`] the draw is `max_value` outright,
/// otherwise uniform-ish over the full range. `min_value == max_value`
/// returns the value without consuming entropy.
fn draw_concurrency(
    ntc: &mut NativeTestCase,
    min_value: i64,
    max_value: i64,
) -> Result<i64, EngineError> {
    if min_value == max_value {
        return Ok(max_value);
    }
    draws::spanned(ntc, draws::LABEL_CONCURRENCY, |ntc| {
        if ntc.weighted(P_MAX_CONCURRENCY, None)? {
            return Ok(max_value);
        }
        let v = ntc.draw_integer(BigInt::from(min_value), BigInt::from(max_value))?;
        Ok(v.to_i128().unwrap() as i64)
    })
}

/// Draw a uniform index in `[0, n)`.
fn draw_index(ntc: &mut NativeTestCase, n: usize) -> Result<usize, EngineError> {
    let i = ntc.draw_integer(BigInt::from(0), BigInt::from(n as i64 - 1))?;
    Ok(i.to_i128().unwrap() as usize)
}

/// Per-worker feature flags over rule indices, deciding which rules are
/// enabled for the calling worker over the whole test case.
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

/// Per-worker state, fully constructed at machine creation and refreshed in
/// place at every join point — so `next_rule` only ever reads state that
/// already exists.
///
/// The flags' disabling probability is drawn from the *creating* handle's
/// stream (a quiescent moment), so draws on one worker never affect draws
/// on another; the per-rule enable decisions inside [`FeatureFlags`] and
/// the per-round continue/stop decisions stay lazy and are drawn from the
/// querying worker's own stream.
struct WorkerState {
    /// Swarm feature flags, persisting for the whole test case.
    flags: FeatureFlags,
    /// Rules handed to this worker so far this round, including rejected
    /// ones; reset by `next_group`.
    steps_drawn: i64,
    /// Handed-out rules reported back as rejected this round via
    /// [`NativeStateMachine::rule_rejected`]; reset by `next_group`.
    steps_rejected: i64,
    /// Whether this worker has a handed-out rule that has been neither
    /// implicitly completed (by the worker's next `next_rule` call) nor
    /// reported as rejected. Cleared at every join point.
    rule_outstanding: bool,
    /// Whether this worker's most recent hand-out was rejected, so the next
    /// `next_rule` call is a retry: it skips the continue/stop decision
    /// (recording a forced continue) rather than re-drawing it, keeping
    /// exactly one random stop decision per rule slot. Cleared on the
    /// next hand-out and at every join point.
    retry_pending: bool,
}

/// Engine-side driver for a single stateful (rule-based) test case,
/// sequential or concurrent.
///
/// The test body registers a fixed set of rules — each belonging to exactly
/// one concurrency group — plus the invariants and the concurrency bounds
/// (the level itself is drawn at creation), and drives execution in rounds:
/// the root handle asks [`Self::next_group`] whether to run another round
/// (and which group is current), then each worker pulls rules for that
/// round via [`Self::next_rule`] until it returns `None`. Rules in the same
/// group may run concurrently; rules in different groups never overlap,
/// because only the current group's rules are handed out and the group
/// changes only at the join points between rounds. A sequential machine is
/// the special case of one group and concurrency 1, where each round hands
/// out exactly one rule.
pub struct NativeStateMachine {
    /// Per group: the global indices of its member rules, in registration
    /// order. Selection draws range over the current group's list only, so
    /// every selection is in-group by construction. Groups are indexed by
    /// order of first appearance in the creating `rule_groups`; the
    /// caller-supplied identifiers live in `group_ids`, parallel to this.
    groups: Vec<Vec<usize>>,
    /// Per group: the caller-supplied identifier, as it appeared in
    /// `rule_groups`. `next_group` reports the current group by this id.
    group_ids: Vec<i64>,
    concurrency: i64,
    /// The group whose rules are handed out this round, written by every
    /// `next_group` call. Meaningful only once `rounds_started > 0`;
    /// `next_rule` rejects calls made before the first round.
    current_group: usize,
    /// Number of rounds started so far, including rejected ones.
    rounds_started: i64,
    /// Rounds whose rule was reported as rejected via
    /// [`Self::rule_rejected`]. Only tracked at concurrency 1 — where each
    /// round is exactly one rule, so a rejected rule is a rejected round —
    /// to preserve the sequential budget semantics: rejected rules do not
    /// count toward `stateful_step_count`. At concurrency > 1 every started
    /// round counts and rejections refund only the worker's within-round
    /// budget.
    rounds_rejected: i64,
    /// Number of registered invariants, bounding the indices
    /// [`Self::should_check_invariant`] accepts.
    num_invariants: usize,
    workers: Vec<WorkerState>,
}

impl NativeStateMachine {
    /// Create a machine, fully constructed: the concurrency level (in
    /// `[min_concurrency, max_concurrency]`, weighted toward the maximum —
    /// see [`draw_concurrency`]) and every worker's swarm disabling
    /// probability are drawn here, from the creating handle's stream, so no
    /// per-worker state is ever pending.
    pub fn new(
        ntc: &mut NativeTestCase,
        rule_groups: Vec<i64>,
        num_invariants: usize,
        min_concurrency: i64,
        max_concurrency: i64,
    ) -> Result<Self, EngineError> {
        hegel_internal_assert!(
            !rule_groups.is_empty(),
            "Stateful testing: there must be at least one rule"
        );
        hegel_internal_assert!(
            min_concurrency >= 1 && min_concurrency <= max_concurrency,
            "Stateful testing: concurrency bounds must satisfy 1 <= min <= max"
        );

        let mut group_ids: Vec<i64> = Vec::new();
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for (rule, &id) in rule_groups.iter().enumerate() {
            let group = group_ids.iter().position(|&g| g == id).unwrap_or_else(|| {
                group_ids.push(id);
                groups.push(Vec::new());
                groups.len() - 1
            });
            groups[group].push(rule);
        }

        let concurrency = draw_concurrency(ntc, min_concurrency, max_concurrency)?;
        let workers = (0..concurrency)
            .map(|_| {
                Ok(WorkerState {
                    flags: FeatureFlags::new(ntc, &groups, rule_groups.len())?,
                    steps_drawn: 0,
                    steps_rejected: 0,
                    rule_outstanding: false,
                    retry_pending: false,
                })
            })
            .collect::<Result<Vec<WorkerState>, EngineError>>()?;
        Ok(NativeStateMachine {
            groups,
            group_ids,
            concurrency,
            current_group: 0,
            rounds_started: 0,
            rounds_rejected: 0,
            num_invariants,
            workers,
        })
    }

    /// The concurrency level drawn at creation: the number of workers that
    /// will pull rules from this machine.
    pub fn concurrency(&self) -> i64 {
        self.concurrency
    }

    /// Start the next round: draw whether another round should run at all
    /// and, if so, which concurrency group is current for it. Returns the
    /// current group's caller-supplied id, or `None` once the test case has
    /// run enough rounds.
    ///
    /// Each call first makes a per-round stop decision: a boolean draw with
    /// probability [`P_STOP`] of halting, recorded in the choice sequence
    /// so the shrinker can truncate the round sequence at any boundary.
    /// Every test case runs at least one round and at most
    /// `stateful_step_count` counted rounds — at concurrency 1 a round
    /// whose rule was rejected ([`Self::rule_rejected`]) does not count,
    /// and total rounds including rejected ones are then bounded by
    /// [`ATTEMPT_MULTIPLIER`] times the step count, or
    /// [`MIN_ATTEMPTS_WITHOUT_SUCCESS`] while no rule has succeeded.
    ///
    /// Must be called from the root handle at each join point, including
    /// before the first `next_rule` call.
    pub fn next_group(&mut self, ntc: &mut NativeTestCase) -> Result<Option<i64>, EngineError> {
        let counted_rounds = self.rounds_started - self.rounds_rejected;
        let step_count = ntc.family().stateful_step_count();
        let attempt_cap = if counted_rounds == 0 {
            step_count
                .saturating_mul(ATTEMPT_MULTIPLIER)
                .max(MIN_ATTEMPTS_WITHOUT_SUCCESS)
        } else {
            step_count.saturating_mul(ATTEMPT_MULTIPLIER)
        };
        let forced = if counted_rounds >= step_count || self.rounds_started >= attempt_cap {
            Some(true)
        } else if self.rounds_started == 0 {
            Some(false)
        } else {
            None
        };
        if ntc.weighted_precise(P_STOP, forced)? {
            return Ok(None);
        }
        let group = if self.groups.len() == 1 {
            0
        } else {
            draw_index(ntc, self.groups.len())?
        };
        for worker in &mut self.workers {
            worker.steps_drawn = 0;
            worker.steps_rejected = 0;
            worker.rule_outstanding = false;
            worker.retry_pending = false;
        }
        self.current_group = group;
        self.rounds_started += 1;
        Ok(Some(self.group_ids[group]))
    }

    /// Draw the index of the next rule for `worker_index` to run this round
    /// — always a rule belonging to the current group, in `[0, num_rules)`
    /// — or `None` once the worker's round is over and it should wait for
    /// the next join point.
    ///
    /// At concurrency 1 every round is exactly one rule, so a join point
    /// follows each rule and the per-round stop decision in
    /// [`Self::next_group`] carries the whole step budget. At higher
    /// concurrency each worker runs between zero and [`MAX_ROUND_RULES`]
    /// rules per round, distributed uniformly: every call makes a
    /// continue/stop decision as a boolean draw from the worker's own
    /// stream, drawn as a *continue* probability
    /// (`(MAX_ROUND_RULES - completed) / (MAX_ROUND_RULES - completed + 1)`)
    /// so that the simplest value stops the round — the shrinker shortens a
    /// worker's round by simplifying any boundary's draw, and the forced
    /// simplest test case runs no rules at all. Rules reported as rejected
    /// ([`Self::rule_rejected`]) do not advance that decision, and total
    /// hand-outs per worker per round are bounded by
    /// [`ATTEMPT_MULTIPLIER`] times [`MAX_ROUND_RULES`].
    ///
    /// Consults only per-worker state (plus the machine's current group), so
    /// draws on one worker never affect draws on another.
    pub fn next_rule(
        &mut self,
        ntc: &mut NativeTestCase,
        worker_index: i64,
    ) -> Result<Option<i64>, EngineError> {
        let worker_idx = self.checked_worker(worker_index)?;
        if self.rounds_started == 0 {
            return Err(EngineError::InvalidArgument(
                "state machine rule requested before the first next_group call".to_string(),
            ));
        }

        if self.concurrency == 1 {
            if self.workers[worker_idx].steps_drawn >= 1 {
                return Ok(None);
            }
        } else {
            let worker = &self.workers[worker_idx];
            let completed = worker.steps_drawn - worker.steps_rejected;
            let attempt_cap = MAX_ROUND_RULES.saturating_mul(ATTEMPT_MULTIPLIER);
            let p_continue = if worker.steps_drawn >= attempt_cap || completed >= MAX_ROUND_RULES {
                0.0
            } else if worker.retry_pending {
                1.0
            } else {
                (MAX_ROUND_RULES - completed) as f64 / (MAX_ROUND_RULES - completed + 1) as f64
            };
            if !ntc.weighted(p_continue, None)? {
                return Ok(None);
            }
        }
        let index = self.select_rule(ntc, worker_idx, self.current_group)?;
        self.workers[worker_idx].steps_drawn += 1;
        self.workers[worker_idx].rule_outstanding = true;
        self.workers[worker_idx].retry_pending = false;
        Ok(Some(index))
    }

    /// Record that `worker_index`'s most recently handed-out rule was
    /// rejected before it completed (a violated assumption), so it does not
    /// count toward the test case's budget: at concurrency 1 the round does
    /// not count toward `stateful_step_count`, and at concurrency > 1 the
    /// rule does not advance the worker's per-round continue/stop decision.
    ///
    /// Errors with `InvalidArgument` when the worker has no outstanding
    /// rule: no rule has been handed to it this round, its current rule was
    /// already rejected, or it has already pulled another rule (implicitly
    /// completing the previous one).
    pub fn rule_rejected(&mut self, worker_index: i64) -> Result<(), EngineError> {
        let worker_idx = self.checked_worker(worker_index)?;
        let worker = &mut self.workers[worker_idx];
        if !worker.rule_outstanding {
            return Err(EngineError::InvalidArgument(
                "rule_rejected called with no outstanding rule".to_string(),
            ));
        }
        worker.rule_outstanding = false;
        worker.steps_rejected += 1;
        worker.retry_pending = true;
        if self.concurrency == 1 {
            self.rounds_rejected += 1;
        }
        Ok(())
    }

    /// Decide whether the caller should run invariant `invariant_index` at
    /// the current join point: a recorded boolean draw that is `true` with
    /// probability `1 / stateful_step_count`, so over a full-length test
    /// case each invariant's expected number of sampled runs is one,
    /// regardless of the step count. The caller owns the machine's
    /// guaranteed checks — its initial state and the final state after the
    /// last round — and runs those without consulting this draw.
    ///
    /// Errors with `InvalidArgument` when `invariant_index` is outside the
    /// registered invariants.
    pub fn should_check_invariant(
        &mut self,
        ntc: &mut NativeTestCase,
        invariant_index: i64,
    ) -> Result<bool, EngineError> {
        let valid = usize::try_from(invariant_index)
            .ok()
            .filter(|&i| i < self.num_invariants);
        if valid.is_none() {
            return Err(EngineError::InvalidArgument(format!(
                "invariant_index must be in [0, {}), got {invariant_index}",
                self.num_invariants
            )));
        }
        let p = 1.0 / ntc.family().stateful_step_count() as f64;
        ntc.weighted_precise(p, None)
    }

    /// Validate a caller-supplied worker index against the drawn
    /// concurrency level.
    fn checked_worker(&self, worker_index: i64) -> Result<usize, EngineError> {
        usize::try_from(worker_index)
            .ok()
            .filter(|&w| w < self.workers.len())
            .ok_or_else(|| {
                EngineError::InvalidArgument(format!(
                    "worker_index must be in [0, {}), got {worker_index}",
                    self.concurrency
                ))
            })
    }

    /// Select the next rule's global index from the current group's member
    /// list.
    ///
    /// Every selection draw is an index in `[0, group_size)` mapped back to
    /// the global rule index, so each selection is in-group by construction.
    /// Up to three rejection-sampling tries against the worker's swarm
    /// flags, then a fallback that enumerates the group's enabled rules.
    fn select_rule(
        &mut self,
        ntc: &mut NativeTestCase,
        worker_idx: usize,
        group: usize,
    ) -> Result<i64, EngineError> {
        let members = &self.groups[group];
        let n = members.len();
        let flags = &mut self.workers[worker_idx].flags;

        let mut known_bad: HashSet<usize> = HashSet::default();
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

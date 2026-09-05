//! Per-origin lifecycle for failures under nondeterministic handling.
//!
//! Confirmation gates origin admission on every path (decisions 20, 21,
//! 24): a raw interesting execution may fill a vacant origin, pending
//! confirmation; an occupied origin changes only through validated accepts;
//! an origin becomes `Confirmed` through a discovery-bar accept (sweep,
//! shrink admission, or backtrack), any failure in the final replay's
//! pooled review, or, for a trusted origin, a failing shrink-time evidence
//! batch; confirmed and trusted origins carry replay state. Origins
//! reproduced from the database are `Trusted` on reproduction, exempt
//! from the bar's verdict — the prior run persisted only confirmed
//! origins, and subjecting real p ~ 0.1 bugs to the bar again would drop
//! them ~55% of the time.
//!
//! The lifecycle also accumulates each origin's physical replay evidence —
//! fails and replays across confirmation batches, reuse reproductions, and
//! shrink-time batches, with the report-time final replay's counts kept
//! apart — and renders the failure's caveat from it (decision 3): the
//! wording quotes only in-run measurements and weights the
//! environment-modification hypothesis only when non-reproduction is
//! surprising given the evidence.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::native::core::ChoiceValue;
use crate::native::test_runner::RunResult;

/// One origin's confirmation state. The only transitions are the ones
/// [`OriginLifecycle`]'s methods implement. `Unconfirmed → Confirmed` has
/// three admission paths: a discovery-bar accept (the post-generation
/// sweep or shrink admission; an accept requires a reproducing replay in
/// the batch itself), a backtrack's bar accept, and the final replay's
/// pooled review, which confirms on any failure with no bar. `Unconfirmed
/// → Trusted` is database reproduction; `Trusted → Confirmed` is
/// promotion by a failing evidence batch. Rejection never demotes and
/// never removes state.
pub(crate) enum OriginState {
    /// Observed interesting; hasn't passed the discovery bar.
    /// `fails`/`replays` accumulate the physical evidence behind rejected
    /// confirmation batches, for the caveated report.
    Unconfirmed { fails: u64, replays: u64 },
    /// Reproduced from the database: exempt from eviction (decision 24).
    /// Carries the stored entry's timeline pool (empty for v1 entries) but
    /// no anchor until an evidence batch promotes it.
    Trusted {
        /// Timelines decoded from the reproducing v2 entry, so an aligned
        /// reuse hit that skips shrink re-persists them instead of
        /// forgetting the pool.
        pool: Vec<Vec<ChoiceValue>>,
        /// Physical replay evidence from the reproduction that earned
        /// trust plus any zero-fail shrink batch, quoted by the caveat.
        fails: u64,
        replays: u64,
        /// The report-time final replay's evidence, kept apart from the
        /// reuse counts (the caveat quotes both).
        report_fails: u64,
        report_replays: u64,
    },
    /// Past the discovery bar, or promoted from `Trusted` by a failing
    /// evidence batch.
    Confirmed {
        /// Failure-rate anchor the gauntlet prices shrink candidates
        /// against; monotone, raised only by validated accepts
        /// (decision 19).
        anchor: f64,
        /// The confirmation run the shrinker starts from; taken once.
        witness: Option<RunResult>,
        /// Captured failing timelines, incumbent first, for replay
        /// fallback and persistence.
        pool: Vec<Vec<ChoiceValue>>,
        /// Physical replay evidence from confirmation batches, quoted by
        /// the caveat.
        fails: u64,
        replays: u64,
        /// The report-time final replay's evidence, kept apart from the
        /// confirmation counts (the caveat quotes both).
        report_fails: u64,
        report_replays: u64,
    },
}

#[derive(Default)]
pub(crate) struct OriginLifecycle {
    origins: BTreeMap<String, OriginState>,
    /// Per-origin starting evidence from the first-interesting check's
    /// replays (seam plan step 2): the discovery bar begins partially
    /// filled instead of from zero. Taken once, by the origin's first
    /// evidence batch.
    seeds: BTreeMap<String, super::Evidence>,
}

impl OriginLifecycle {
    /// Seed `origin`'s next evidence batch with the first-interesting
    /// check's replay observations. Unlike [`Self::reject`], this counts
    /// no rejection — the check is detection, not a bar verdict.
    pub(crate) fn seed_evidence(&mut self, origin: &str, evidence: super::Evidence) {
        self.seeds.insert(origin.to_string(), evidence);
    }

    /// The seeded starting evidence for `origin`, taken at most once.
    pub(crate) fn take_seed(&mut self, origin: &str) -> Option<super::Evidence> {
        self.seeds.remove(origin)
    }

    /// A raw interesting execution observed `origin`. Creates the
    /// `Unconfirmed` entry on first sighting and never changes existing
    /// state — a raw run is selection, not evidence.
    pub(crate) fn observe(&mut self, origin: &str) {
        if !self.origins.contains_key(origin) {
            self.origins.insert(
                origin.to_string(),
                OriginState::Unconfirmed {
                    fails: 0,
                    replays: 0,
                },
            );
        }
    }

    /// The database reproduced `origin` this run: trusted without
    /// re-running the bar (decision 24). `pool` carries the reproducing
    /// entry's stored timelines (empty for v1 entries), truncated to
    /// [`super::POOL_CAP`] — a decoded entry can carry up to the looser
    /// format bound. `evidence` is the reproducing replay batch's physical
    /// (fails, replays), folded into the trusted counts. Never demotes
    /// `Confirmed`, and never replaces an existing pool with an empty one.
    pub(crate) fn trust(
        &mut self,
        origin: &str,
        mut pool: Vec<Vec<ChoiceValue>>,
        evidence: (u64, u64),
    ) {
        pool.truncate(super::POOL_CAP);
        match self.origins.get_mut(origin) {
            Some(OriginState::Confirmed { .. }) => {}
            Some(OriginState::Trusted {
                pool: existing,
                fails,
                replays,
                ..
            }) => {
                if !pool.is_empty() {
                    *existing = pool;
                }
                *fails += evidence.0;
                *replays += evidence.1;
            }
            Some(state @ OriginState::Unconfirmed { .. }) => {
                *state = OriginState::Trusted {
                    pool,
                    fails: evidence.0,
                    replays: evidence.1,
                    report_fails: 0,
                    report_replays: 0,
                };
            }
            None => {
                self.origins.insert(
                    origin.to_string(),
                    OriginState::Trusted {
                        pool,
                        fails: evidence.0,
                        replays: evidence.1,
                        report_fails: 0,
                        report_replays: 0,
                    },
                );
            }
        }
    }

    /// A shrink-time evidence batch on a trusted origin produced no
    /// failure: fold its physical (fails, replays) into the trusted counts
    /// — the origin stays `Trusted`, skips shrinking, and is still
    /// reported and persisted. No-op in any other state.
    pub(crate) fn record_trusted_batch(&mut self, origin: &str, evidence: (u64, u64)) {
        if let Some(OriginState::Trusted { fails, replays, .. }) = self.origins.get_mut(origin) {
            *fails += evidence.0;
            *replays += evidence.1;
        }
    }

    /// Whether `origin` still has to face the discovery bar before its
    /// replay state is usable.
    pub(crate) fn needs_confirmation(&self, origin: &str) -> bool {
        matches!(
            self.origins.get(origin),
            None | Some(OriginState::Unconfirmed { .. })
        )
    }

    /// The discovery bar accepted `origin`, or a failing evidence batch
    /// promoted it from `Trusted`: store its replay state. `evidence` is
    /// the batch's physical (fails, replays), folded into the origin's
    /// cumulative counts. The pool is truncated to [`super::POOL_CAP`] —
    /// the lifecycle is the single writer of stored pools, so the
    /// invariant is enforced here. Confirming a confirmed origin is a
    /// violated invariant: every caller sits behind a
    /// `needs_confirmation`/`take_witness` check.
    pub(crate) fn confirm(
        &mut self,
        origin: &str,
        anchor: f64,
        witness: Option<RunResult>,
        mut pool: Vec<Vec<ChoiceValue>>,
        evidence: (u64, u64),
    ) -> Result<(), crate::control::InternalError> {
        pool.truncate(super::POOL_CAP);
        let (prior_fails, prior_replays, report_fails, report_replays) =
            match self.origins.get(origin) {
                Some(OriginState::Unconfirmed { fails, replays }) => (*fails, *replays, 0, 0),
                Some(OriginState::Trusted {
                    fails,
                    replays,
                    report_fails,
                    report_replays,
                    ..
                }) => (*fails, *replays, *report_fails, *report_replays),
                Some(OriginState::Confirmed { .. }) => {
                    crate::control::hegel_internal_error!(
                        "OriginLifecycle::confirm: {origin} is already confirmed"
                    );
                }
                None => (0, 0, 0, 0),
            };
        self.origins.insert(
            origin.to_string(),
            OriginState::Confirmed {
                anchor,
                witness,
                pool,
                fails: prior_fails + evidence.0,
                replays: prior_replays + evidence.1,
                report_fails,
                report_replays,
            },
        );
        Ok(())
    }

    /// A validated accept measured `origin`'s failure rate at `anchor`
    /// (a gauntlet first-accept or a boost holdout). Monotone: never
    /// lowers the stored anchor (decision 19), and a no-op unless the
    /// origin is confirmed — an anchor only exists past the bar.
    pub(crate) fn raise_anchor(&mut self, origin: &str, anchor: f64) {
        if let Some(OriginState::Confirmed { anchor: stored, .. }) = self.origins.get_mut(origin) {
            if anchor > *stored {
                *stored = anchor;
            }
        }
    }

    /// Record the report-time final replay's physical (fails, replays) in
    /// `origin`'s report counts, kept apart from the confirmation or reuse
    /// evidence — the caveat quotes both, and a zero-fail record switches
    /// its wording to dry-at-report-time. No-op unless trusted or
    /// confirmed — no other state reaches the final replay.
    pub(crate) fn record_final_replay(&mut self, origin: &str, evidence: (u64, u64)) {
        match self.origins.get_mut(origin) {
            Some(OriginState::Confirmed {
                report_fails,
                report_replays,
                ..
            })
            | Some(OriginState::Trusted {
                report_fails,
                report_replays,
                ..
            }) => {
                *report_fails += evidence.0;
                *report_replays += evidence.1;
            }
            _ => {}
        }
    }

    /// The discovery bar rejected `origin`, with the rejecting batch's
    /// physical (fails, replays). Returns whether the caller must evict it
    /// from the interesting map: true for unconfirmed origins (recording
    /// the rejection for the caveated report), false for trusted and
    /// confirmed ones, which are exempt.
    pub(crate) fn reject(&mut self, origin: &str, evidence: (u64, u64)) -> bool {
        match self.origins.get_mut(origin) {
            Some(OriginState::Unconfirmed { fails, replays }) => {
                *fails += evidence.0;
                *replays += evidence.1;
                true
            }
            None => {
                self.origins.insert(
                    origin.to_string(),
                    OriginState::Unconfirmed {
                        fails: evidence.0,
                        replays: evidence.1,
                    },
                );
                true
            }
            Some(OriginState::Trusted { .. }) | Some(OriginState::Confirmed { .. }) => false,
        }
    }

    /// Take the stored witness run and its anchor as the shrinker's
    /// starting point. Yields once per confirmation.
    pub(crate) fn take_witness(&mut self, origin: &str) -> Option<(RunResult, f64)> {
        match self.origins.get_mut(origin) {
            Some(OriginState::Confirmed {
                anchor, witness, ..
            }) => witness.take().map(|w| (w, *anchor)),
            _ => None,
        }
    }

    /// The captured timeline pool for `origin`; empty unless confirmed or
    /// trusted from a v2 entry.
    pub(crate) fn pool(&self, origin: &str) -> &[Vec<ChoiceValue>] {
        match self.origins.get(origin) {
            Some(OriginState::Confirmed { pool, .. }) | Some(OriginState::Trusted { pool, .. }) => {
                pool
            }
            _ => &[],
        }
    }

    /// The caveat attached to `origin`'s reported failure (decision 3):
    /// its confirmation standing with the in-run replay evidence, worded
    /// per the state. `None` for an origin the lifecycle never saw — a
    /// deterministic failure carries no caveat.
    pub(crate) fn caveat(&self, origin: &str) -> Option<String> {
        Some(match self.origins.get(origin)? {
            OriginState::Confirmed {
                fails,
                replays,
                report_fails,
                report_replays,
                ..
            } => {
                if *report_replays > 0 && *report_fails == 0 {
                    format!(
                        "nondeterministic failure, confirmed earlier this run \
                         (failed {fails} of {replays} replays) but not reproduced \
                         at report time — a rare failure, or something in the \
                         environment changed after discovery"
                    )
                } else if *report_replays > 0 {
                    format!(
                        "nondeterministic failure, confirmed: failed {fails} of \
                         {replays} replays at confirmation and {report_fails} of \
                         {report_replays} at report time"
                    )
                } else {
                    format!(
                        "nondeterministic failure, confirmed: failed {fails} of \
                         {replays} replays this run"
                    )
                }
            }
            OriginState::Trusted {
                fails,
                replays,
                report_fails,
                report_replays,
                ..
            } => {
                if *report_replays > 0 && *report_fails == 0 {
                    format!(
                        "nondeterministic failure, reproduced from stored \
                         timelines earlier this run (failed {fails} of {replays} \
                         replays) but not reproduced at report time — a rare \
                         failure, or something in the environment changed after \
                         discovery"
                    )
                } else if *report_replays > 0 {
                    format!(
                        "nondeterministic failure, reproduced from stored \
                         timelines: failed {fails} of {replays} replays at reuse \
                         and {report_fails} of {report_replays} at report time"
                    )
                } else {
                    format!(
                        "nondeterministic failure, reproduced from stored \
                         timelines: failed {fails} of {replays} replays this run"
                    )
                }
            }
            OriginState::Unconfirmed { fails, replays } => {
                if *fails > 0 {
                    format!(
                        "unconfirmed failure: failed {fails} of {replays} replays \
                         this run, below the confirmation bar — likely rare"
                    )
                } else if *replays == 0 {
                    "unconfirmed failure: observed once, never replayed — a rare \
                     failure, or the environment changed between executions"
                        .to_string()
                } else {
                    format!(
                        "unconfirmed failure: failed 0 of {replays} replays after \
                         the observed failure — a rare failure, or the environment \
                         changed between executions"
                    )
                }
            }
        })
    }

    /// Origins observed but never confirmed — bar rejects and never-barred
    /// sightings alike — in origin order: the caveated-failure report
    /// (decision 3), used only when nothing confirmed (decision 24).
    pub(crate) fn unconfirmed(&self) -> impl Iterator<Item = &str> {
        self.origins.iter().filter_map(|(origin, state)| {
            matches!(state, OriginState::Unconfirmed { .. }).then_some(origin.as_str())
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/nd/lifecycle_tests.rs"]
mod tests;

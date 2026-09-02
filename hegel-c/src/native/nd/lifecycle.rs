//! Per-origin lifecycle for failures under nondeterministic handling.
//!
//! Confirmation gates origin admission on every path (decisions 20, 21,
//! 24): a raw interesting execution may fill a vacant origin, pending
//! confirmation; an occupied origin changes only through validated accepts;
//! only the discovery bar moves an origin to `Confirmed`, and only
//! confirmed origins carry replay state. Origins reproduced from the
//! database are `Trusted` on reproduction — the prior run persisted only
//! confirmed origins, and re-running the bar would drop real p ~ 0.1 bugs
//! ~55% of the time.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::native::core::ChoiceValue;
use crate::native::test_runner::RunResult;

/// One origin's confirmation state. The only transitions are the ones
/// [`OriginLifecycle`]'s methods implement: `Unconfirmed → Confirmed` (bar
/// accept), `Unconfirmed → Trusted` (database reproduction), `Trusted →
/// Confirmed` (evidence-gathering bar accept). Rejection never demotes and
/// never removes state.
pub(crate) enum OriginState {
    /// Observed interesting; hasn't passed the discovery bar. `rejections`
    /// counts failed confirmation batches, for the caveated report.
    Unconfirmed { rejections: u64 },
    /// Reproduced from the database: exempt from eviction (decision 24).
    /// Carries the stored entry's timeline pool (empty for v1 entries) but
    /// no anchor until a confirmation batch gathers evidence.
    Trusted {
        /// Timelines decoded from the reproducing v2 entry, so an aligned
        /// reuse hit that skips shrink re-persists them instead of
        /// forgetting the pool.
        pool: Vec<Vec<ChoiceValue>>,
    },
    /// Past the discovery bar (or trusted with gathered evidence).
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
    },
}

#[derive(Default)]
pub(crate) struct OriginLifecycle {
    origins: BTreeMap<String, OriginState>,
}

impl OriginLifecycle {
    /// A raw interesting execution observed `origin`. Creates the
    /// `Unconfirmed` entry on first sighting and never changes existing
    /// state — a raw run is selection, not evidence.
    pub(crate) fn observe(&mut self, origin: &str) {
        if !self.origins.contains_key(origin) {
            self.origins.insert(
                origin.to_string(),
                OriginState::Unconfirmed { rejections: 0 },
            );
        }
    }

    /// The database reproduced `origin` this run: trusted without
    /// re-running the bar (decision 24). `pool` carries the reproducing
    /// entry's stored timelines (empty for v1 entries). Never demotes
    /// `Confirmed`, and never replaces an existing pool with an empty one.
    pub(crate) fn trust(&mut self, origin: &str, pool: Vec<Vec<ChoiceValue>>) {
        match self.origins.get_mut(origin) {
            Some(OriginState::Confirmed { .. }) => {}
            Some(OriginState::Trusted { pool: existing }) => {
                if !pool.is_empty() {
                    *existing = pool;
                }
            }
            Some(state @ OriginState::Unconfirmed { .. }) => {
                *state = OriginState::Trusted { pool };
            }
            None => {
                self.origins
                    .insert(origin.to_string(), OriginState::Trusted { pool });
            }
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

    /// The discovery bar accepted `origin`: store its replay state.
    pub(crate) fn confirm(
        &mut self,
        origin: &str,
        anchor: f64,
        witness: Option<RunResult>,
        pool: Vec<Vec<ChoiceValue>>,
    ) {
        self.origins.insert(
            origin.to_string(),
            OriginState::Confirmed {
                anchor,
                witness,
                pool,
            },
        );
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

    /// The discovery bar rejected `origin`. Returns whether the caller
    /// must evict it from the interesting map: true for unconfirmed
    /// origins (recording the rejection for the caveated report), false
    /// for trusted and confirmed ones, which are exempt.
    pub(crate) fn reject(&mut self, origin: &str) -> bool {
        match self.origins.get_mut(origin) {
            Some(OriginState::Unconfirmed { rejections }) => {
                *rejections += 1;
                true
            }
            None => {
                self.origins.insert(
                    origin.to_string(),
                    OriginState::Unconfirmed { rejections: 1 },
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
            Some(OriginState::Confirmed { pool, .. }) | Some(OriginState::Trusted { pool }) => pool,
            _ => &[],
        }
    }

    /// Origins observed but never confirmed, with rejection counts, in
    /// origin order — the caveated-failure report (decision 3), used only
    /// when nothing confirmed (decision 24).
    pub(crate) fn unconfirmed(&self) -> impl Iterator<Item = (&str, u64)> {
        self.origins
            .iter()
            .filter_map(|(origin, state)| match state {
                OriginState::Unconfirmed { rejections } if *rejections > 0 => {
                    Some((origin.as_str(), *rejections))
                }
                _ => None,
            })
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/nd/lifecycle_tests.rs"]
mod tests;

//! Statistics for nondeterministic-test handling: the weighted evidence type
//! every replay-counting decision shares, the discovery-confirmation bar,
//! the shrink gauntlet, the boost schedule arithmetic, and the replay
//! budgets. Everything here is pure arithmetic — no engine state, no
//! executions — so each rule is tested directly against the exact-DP and
//! simulation results that derived it (`notes/experiments/`, decisions
//! 7, 11, 16, 17, 19, 22, 23 in `notes/decisions.md`).

/// Replay evidence for one proposition ("this timeline reproduces this
/// origin"). Failures always count in full; a non-failure counts `weight`,
/// less than 1.0 when the replay structurally diverged from the timeline it
/// was probing — a diverged run is weak evidence of non-reproduction
/// (decision 22). The physical run count is tracked separately for cost
/// caps.
#[derive(Clone, Copy, Default)]
pub(crate) struct Evidence {
    fails: u64,
    physical: u64,
    weighted_misses: f64,
}

impl Evidence {
    pub(crate) fn record(&mut self, failed: bool, weight: f64) {
        self.physical += 1;
        if failed {
            self.fails += 1;
        } else {
            self.weighted_misses += weight;
        }
    }

    pub(crate) fn fails(&self) -> u64 {
        self.fails
    }

    /// Physical replay count, for cost accounting.
    pub(crate) fn runs(&self) -> u64 {
        self.physical
    }

    fn weighted_total(&self) -> f64 {
        self.fails as f64 + self.weighted_misses
    }

    /// Wilson lower confidence bound on the failure rate.
    pub(crate) fn lower_bound(&self) -> f64 {
        wilson_bound(self.fails as f64, self.weighted_total(), false)
    }

    /// Wilson upper confidence bound on the failure rate.
    pub(crate) fn upper_bound(&self) -> f64 {
        wilson_bound(self.fails as f64, self.weighted_total(), true)
    }
}

fn wilson_bound(fails: f64, runs: f64, upper: bool) -> f64 {
    if runs <= 0.0 {
        return if upper { 1.0 } else { 0.0 };
    }
    let z = 1.96f64;
    let p = fails / runs;
    let z2 = z * z;
    let denom = 1.0 + z2 / runs;
    let center = p + z2 / (2.0 * runs);
    let margin = z * libm::sqrt((p * (1.0 - p) + z2 / (4.0 * runs)) / runs);
    let bound = if upper {
        (center + margin) / denom
    } else {
        (center - margin) / denom
    };
    bound.clamp(0.0, 1.0)
}

pub(crate) const GATE_RUNS: u64 = 10;
pub(crate) const CONFIRM_CAP: u64 = 40;
pub(crate) const CONFIRM_MIN_FAILS: u64 = 4;

pub(crate) enum BarVerdict {
    Accept,
    Reject,
    Continue,
}

/// The discovery-confirmation bar (decision 23): gate then extend — reject
/// on zero failures in the first [`GATE_RUNS`] replays, otherwise continue
/// to [`CONFIRM_CAP`] total, accepting early on the [`CONFIRM_MIN_FAILS`]th
/// failure. Operating points (exact DP, experiment 005A): 0.6% false accept
/// per p = 0.02 fluke, 45% per-discovery power at the p = 0.1 target, ~15
/// replays per rejected fluke, ~4.4 per p = 0.9 confirmation. The gate
/// reads weighted misses, so diverged replays reject more slowly; the cap
/// is physical cost and stays exact.
pub(crate) fn discovery_bar(evidence: &Evidence) -> BarVerdict {
    if evidence.fails >= CONFIRM_MIN_FAILS {
        return BarVerdict::Accept;
    }
    if evidence.fails == 0 && evidence.weighted_misses >= GATE_RUNS as f64 {
        return BarVerdict::Reject;
    }
    if evidence.fails + CONFIRM_CAP.saturating_sub(evidence.physical) < CONFIRM_MIN_FAILS {
        return BarVerdict::Reject;
    }
    BarVerdict::Continue
}

pub(crate) const GAUNTLET_CAP: u64 = 30;
pub(crate) const GAUNTLET_GAMMA: f64 = 0.8;
pub(crate) const GAUNTLET_FLOOR: f64 = 0.05;

pub(crate) enum GauntletVerdict {
    Accept { lower_bound: f64 },
    Reject,
    Continue,
}

/// The shrink-candidate gauntlet (decisions 7 and 17): accept when the
/// evidence's lower bound clears `max(GAUNTLET_GAMMA * anchor,
/// GAUNTLET_FLOOR)`, reject when the upper bound proves it never will or
/// the physical cap is spent, otherwise keep rerunning. The returned lower
/// bound lets the caller raise its monotone anchor — never lower it, and
/// never from replay-sourced evidence (decision 19).
pub(crate) fn gauntlet(evidence: &Evidence, anchor: f64) -> GauntletVerdict {
    let threshold = (GAUNTLET_GAMMA * anchor).max(GAUNTLET_FLOOR);
    let lower = evidence.lower_bound();
    if lower >= threshold {
        return GauntletVerdict::Accept { lower_bound: lower };
    }
    if evidence.upper_bound() < threshold || evidence.physical >= GAUNTLET_CAP {
        return GauntletVerdict::Reject;
    }
    GauntletVerdict::Continue
}

pub(crate) const POOL_CAP: usize = 10;
pub(crate) const BOOST_POOL: usize = 16;
pub(crate) const BOOST_HOLDOUT: u64 = 10;

/// Boost runs only for origins whose confirmation anchor sits below this
/// floor (gate G2): replay of a sub-floor origin is unreliable enough
/// that hunting a steadier timeline is worth the measurement runs, while
/// above it the halving race buys nothing a user would notice.
pub(crate) const BOOST_RELIABILITY_FLOOR: f64 = 0.5;

/// Candidates surviving one successive-halving boost round.
pub(crate) fn boost_keep(candidates: usize) -> usize {
    candidates.div_ceil(2)
}

/// Continuation budget for replaying a timeline of flattened length `len`:
/// the timeline plus `max(4, len / 8)` fresh draws. Experiment 004: a budget
/// of 4 absorbs all net elongation on plateau bodies and larger flat budgets
/// buy nothing; the `len / 8` term scales for long timelines.
pub(crate) fn continuation_budget(len: usize) -> usize {
    len + (len / 8).max(4)
}

/// The reproduction target (decision 16): the machinery is sized for tests
/// failing at least this often.
pub(crate) const TARGET_FAILURE_RATE: f64 = 0.1;

const REUSE_MISS_TOLERANCE: f64 = 0.05;

/// Replays before concluding a stored timeline no longer fails: enough that
/// a bug failing at `rate` slips through with probability at most
/// `tolerance` (decision 11; a flat 10 misses a rate-0.1 bug 35% of the
/// time, decision 16). Callers exit early on the first failure, so the
/// expected cost on a live bug is ~1/rate.
pub(crate) fn replay_budget(rate: f64, tolerance: f64) -> u64 {
    libm::ceil(libm::log(tolerance) / libm::log(1.0 - rate)) as u64
}

/// [`replay_budget`] at the standing target.
pub(crate) fn reuse_replay_budget() -> u64 {
    replay_budget(TARGET_FAILURE_RATE, REUSE_MISS_TOLERANCE)
}

/// The verbatim watermark (decision 22): the fraction of `stored` a replay
/// tracked before first diverging, which is the weight of that replay's
/// non-failure as evidence about `stored` — a diverged run says little
/// about the timeline it abandoned. Linear, no floor; the physical caps
/// bound the cost of heavily-diverged probing. An empty timeline is
/// trivially fully tracked.
pub(crate) fn verbatim_weight(
    stored: &[crate::native::core::ChoiceValue],
    realized: &[crate::native::core::ChoiceValue],
) -> f64 {
    if stored.is_empty() {
        return 1.0;
    }
    let matched = stored
        .iter()
        .zip(realized)
        .take_while(|(s, r)| *s == *r)
        .count();
    matched as f64 / stored.len() as f64
}

/// Positional splices tried after the whole pool misses (decision 25;
/// experiment 006: ~6 splice replays recover 65-100% of full-pool misses).
pub(crate) const REPRODUCE_SPLICES: u64 = 6;

/// Fresh generations tried at the end of the report-time final replay,
/// after the pool and its splices miss. A fresh reproduction is still a
/// reportable failing execution; its misses carry no weight.
pub(crate) const FINAL_REPLAY_FRESH: u64 = 4;

pub(crate) mod lifecycle;

#[cfg(test)]
#[path = "../../../tests/embedded/native/nd/mod_tests.rs"]
mod tests;

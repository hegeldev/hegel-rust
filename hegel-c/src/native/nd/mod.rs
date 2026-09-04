//! Statistics for nondeterministic-test handling: the weighted evidence type
//! every replay-counting decision shares, the discovery-confirmation bar,
//! the shrink gauntlet, the boost schedule arithmetic, and the replay
//! budgets. Everything here is pure arithmetic — no engine state, no
//! executions — so each rule is tested directly against the exact-DP and
//! simulation results that derived it (`notes/experiments/`, decisions
//! 7, 11, 16, 17, 19, 22, 23, 54-56 in `notes/decisions.md`).

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

/// Floor of the accept threshold, derived in experiment 008: 0.05 <
/// LCB(4/30) = 0.0531, the [`GAUNTLET_MIN_FAILS`] acceptance boundary at
/// [`GAUNTLET_CAP`], so the floor costs no power — and it is the largest
/// such value, at 0.02 false accepts per entered gauntlet against a
/// q = 0.02 fluke.
pub(crate) const GAUNTLET_FLOOR: f64 = 0.05;

/// Failures a gauntlet accept requires. A single failure on a fresh ledger
/// bounds the rate above 0.2065 (Wilson at 1/1), so without a minimum every
/// threshold below that accepts on the recruiting run and the whole
/// low-anchor regime degenerates to single-run accepts (experiment 008,
/// H1: 33% bug loss at the decision-16 target). Falling short is never
/// grounds to reject — the verdict stays Continue and evidence accumulates.
pub(crate) const GAUNTLET_MIN_FAILS: u64 = 4;

/// Anchors at or above this run the gauntlet at gamma 1.0 instead of
/// [`GAUNTLET_GAMMA`]. With [`ANCHOR_SEED_RUNS`]-sized seeding only
/// zero-miss evidence reaches it (LCB(20/20) = 0.839; 19/20 gives 0.764),
/// so it marks incumbents indistinguishable from deterministic and refuses
/// to trade their reliability down: experiment 008's D2 displacement drops
/// from 33% to zero, for +26% replay cost on near-deterministic landscapes
/// (decision 55's G6 trade).
pub(crate) const RETENTION_HIGH_WATER: f64 = 0.8;

/// Physical runs an anchor-seeding batch extends to past its accept: the
/// discovery bar's batch keeps replaying, and a gauntlet accept tops the
/// candidate's ledger up, before either seeds the anchor. Stopping at the
/// accept itself biases the anchor toward the stopping rule (a
/// four-straight-fail bar batch seeds 0.51 whatever the true rate); 20 is
/// the largest size whose all-fail LCB (0.839) a candidate can still match
/// within [`GAUNTLET_CAP`] — 40-run seeding stalls shrinking outright
/// (experiment 008).
pub(crate) const ANCHOR_SEED_RUNS: u64 = 20;

pub(crate) enum GauntletVerdict {
    Accept,
    Reject,
    Continue,
}

/// The shrink-candidate gauntlet (decisions 7, 17, 54): accept when the
/// evidence carries [`GAUNTLET_MIN_FAILS`] failures and its lower bound
/// clears `max(gamma * anchor, GAUNTLET_FLOOR)`, with gamma
/// [`GAUNTLET_GAMMA`] below [`RETENTION_HIGH_WATER`] and 1.0 at or above
/// it; reject when the upper bound proves the threshold unreachable or the
/// physical cap is spent, otherwise keep rerunning — short of the failure
/// minimum the verdict is Continue, never Reject. Operating points: the
/// exact-DP rows in experiment 008, pinned by
/// `gauntlet_matches_the_008_operating_points`; worst-case false accept is
/// 4.0e-4 per proposal against a q = 0.02 fluke, under the 1e-3 target,
/// which also absorbs the check-per-run stopping bias (z stays 1.96). The
/// caller raises its monotone anchor from the accepted candidate's
/// topped-up ledger — never lowering it, and post-accept re-measurement
/// of the standing incumbent never feeds it (decision 19).
pub(crate) fn gauntlet(evidence: &Evidence, anchor: f64) -> GauntletVerdict {
    let gamma = if anchor >= RETENTION_HIGH_WATER {
        1.0
    } else {
        GAUNTLET_GAMMA
    };
    let threshold = (gamma * anchor).max(GAUNTLET_FLOOR);
    if evidence.fails >= GAUNTLET_MIN_FAILS && evidence.lower_bound() >= threshold {
        return GauntletVerdict::Accept;
    }
    if evidence.upper_bound() < threshold || evidence.physical >= GAUNTLET_CAP {
        return GauntletVerdict::Reject;
    }
    GauntletVerdict::Continue
}

/// Total stored timelines per origin, incumbent included — the invariant
/// every stored, persisted, or replayed pool obeys
/// (`test_runner::pooled_timelines` builds them; the lifecycle's writers
/// truncate incoming pools). Decision 22 measured K=5 as near-ceiling and
/// K=10 as the plateau, so incumbent-plus-nine sits inside the measured
/// range. The decode-side format bound
/// ([`crate::native::blob::ND_STATE_MAX_TIMELINES`]) is deliberately
/// looser.
pub(crate) const POOL_CAP: usize = 10;
pub(crate) const BOOST_POOL: usize = 16;

/// Holdout replays scoring a boost winner — [`ANCHOR_SEED_RUNS`], so a
/// boost-raised anchor is estimated on the same batch size as a seeded one.
pub(crate) const BOOST_HOLDOUT: u64 = ANCHOR_SEED_RUNS;

/// Boost runs only for origins whose confirmation anchor sits below this
/// floor (gate G2): replay of a sub-floor origin is unreliable enough
/// that hunting a steadier timeline is worth the measurement runs, while
/// above it the halving race buys nothing a user would notice. In
/// [`ANCHOR_SEED_RUNS`]-batch LCB units the boundary image of decision
/// 28's "true rate below 0.5" class is LCB(10/20) ~= 0.30 (experiment
/// 008: recall 0.991, precision 1.000 on the G7 population; the literal
/// 0.5 triggers on 59% of true-0.7 incumbents, 0.30 on 5%).
pub(crate) const BOOST_RELIABILITY_FLOOR: f64 = 0.30;

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

/// The verbatim watermark (decisions 22, 45): the flat-length-weighted
/// fraction of `stored` a replay tracked before first diverging, which is
/// the weight of that replay's non-failure as evidence about `stored` — a
/// diverged run says little about the timeline it abandoned. Each element
/// counts its flattened length, and a diverged clone pair still earns
/// credit for the tracked prefix inside it, so a long clone stream is not
/// written off by one late fall-off. Linear, no floor; the physical caps
/// bound the cost of heavily-diverged probing. An empty timeline is
/// trivially fully tracked.
pub(crate) fn verbatim_weight(
    stored: &[crate::native::core::ChoiceValue],
    realized: &[crate::native::core::ChoiceValue],
) -> f64 {
    use crate::native::core::ChoiceValueRef;
    if stored.is_empty() {
        return 1.0;
    }
    let credit = tracked_credit(
        stored.iter().map(ChoiceValueRef::from),
        realized.iter().map(ChoiceValueRef::from),
    );
    credit as f64 / crate::native::core::flattened_values_len(stored) as f64
}

/// Flat weight of the stored prefix that `realized` tracked: a matched
/// element earns its flattened length, and the first mismatch ends the
/// walk — earning `1 + tracked_credit(children)` when both sides are clone
/// streams and nothing otherwise.
fn tracked_credit<'s, 'r>(
    stored: impl Iterator<Item = crate::native::core::ChoiceValueRef<'s>>,
    realized: impl Iterator<Item = crate::native::core::ChoiceValueRef<'r>>,
) -> usize {
    use crate::native::core::ChoiceValueRef;
    let mut credit = 0;
    for (s, r) in stored.zip(realized) {
        if s == r {
            credit += match s {
                ChoiceValueRef::Clone(c) => 1 + c.flat_len(),
                _ => 1,
            };
            continue;
        }
        if let (ChoiceValueRef::Clone(cs), ChoiceValueRef::Clone(cr)) = (s, r) {
            credit += 1 + tracked_credit(cs.values(), cr.values());
        }
        break;
    }
    credit
}

/// Dump hook for experiment 009a: when armed, every measurement replay
/// (`test_runner::nd_replay_once`) records its stored and realized
/// timelines, its evidence weight, and whether it reproduced the failure,
/// for the harness to drain.
#[cfg(feature = "__bench")]
pub mod watermark_dump {
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicBool, Ordering};

    use crate::native::core::ChoiceValue;
    use crate::sys::sync::Mutex;

    pub struct WatermarkSample {
        pub stored: Vec<ChoiceValue>,
        pub realized: Vec<ChoiceValue>,
        pub weight: f64,
        pub failed: bool,
    }

    static ARMED: AtomicBool = AtomicBool::new(false);
    static SAMPLES: Mutex<Vec<WatermarkSample>> = Mutex::new(Vec::new());

    pub fn arm() {
        ARMED.store(true, Ordering::Relaxed);
    }

    pub fn drain() -> Vec<WatermarkSample> {
        core::mem::take(&mut *SAMPLES.lock())
    }

    pub(crate) fn record(
        stored: &[ChoiceValue],
        realized: &[ChoiceValue],
        weight: f64,
        failed: bool,
    ) {
        if !ARMED.load(Ordering::Relaxed) {
            return;
        }
        SAMPLES.lock().push(WatermarkSample {
            stored: stored.to_vec(),
            realized: realized.to_vec(),
            weight,
            failed,
        });
    }
}

/// Dump hook for experiment 011: when armed, every nondeterminism flip
/// records its detection site, the run's call count, and the interesting
/// map at flip time, and every reject-eviction records the evicted
/// incumbent, for the harness to drain.
#[cfg(feature = "__bench")]
pub mod seam_dump {
    use alloc::string::String;
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicBool, Ordering};

    use crate::native::core::ChoiceValue;
    use crate::sys::sync::Mutex;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum FlipSite {
        Concurrency,
        TreeMismatch,
        ShrinkVerify,
        FinalReplay,
        StoredV2Reuse,
        StoredV2Blob,
    }

    pub enum SeamEvent {
        Flip {
            site: FlipSite,
            calls: u64,
            incumbents: Vec<(String, Vec<ChoiceValue>)>,
        },
        Evict {
            origin: String,
            values: Vec<ChoiceValue>,
            at_final_replay: bool,
        },
    }

    static ARMED: AtomicBool = AtomicBool::new(false);
    static EVENTS: Mutex<Vec<SeamEvent>> = Mutex::new(Vec::new());

    pub fn arm() {
        ARMED.store(true, Ordering::Relaxed);
    }

    pub fn drain() -> Vec<SeamEvent> {
        core::mem::take(&mut *EVENTS.lock())
    }

    pub(crate) fn record(event: SeamEvent) {
        if !ARMED.load(Ordering::Relaxed) {
            return;
        }
        EVENTS.lock().push(event);
    }
}

/// Positional splices tried after the whole pool misses (decision 25).
/// Experiment 006 measured the 65-100% rescue rate at a cap of 10 splice
/// candidates, costing 1.6-6.3 replays per rescue; the shipped 6 was a
/// transcription error, corrected by decision 52.
pub(crate) const REPRODUCE_SPLICES: u64 = 10;

/// Fresh generations tried at the end of the report-time final replay,
/// after the pool and its splices miss. A fresh reproduction is still a
/// reportable failing execution; its misses carry no weight. Chosen, not
/// derived (decision 53): a small tail behind the budgeted pool and
/// splice replays — no experiment prices it.
pub(crate) const FINAL_REPLAY_FRESH: u64 = 4;

/// Replay attempts for a v1 exact-choice blob, each with the standard
/// continuation budget (the reuse path's semantics). One exact
/// no-continuation replay reproduced never-flipped runs' blobs at 13%
/// against v2's 100% at p = 0.9 (009a); four continuation attempts bound
/// the worst-case joint escape-then-miss at 1.2e-3 (seam plan).
pub(crate) const V1_BLOB_REPLAYS: u64 = 4;

pub(crate) mod lifecycle;

#[cfg(test)]
#[path = "../../../tests/embedded/native/nd/mod_tests.rs"]
mod tests;

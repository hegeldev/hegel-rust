//! Statistics for nondeterministic-test handling: the evidence type every
//! replay-counting decision shares, the discovery-confirmation bar,
//! the shrink gauntlet, the boost schedule arithmetic, and the replay
//! budgets. Everything here is pure arithmetic — no engine state, no
//! executions — so each rule is tested directly against the exact-DP and
//! simulation results that derived it (`notes/experiments/`, decisions
//! 7, 11, 16, 17, 19, 23, 54-56, 71 in `notes/decisions.md`).

use alloc::collections::BTreeMap;

/// Replay evidence for one proposition ("this test case reproduces this
/// origin"). Every replay is one Bernoulli trial of the test case under the
/// standing replay procedure — whatever timeline it realized — so failures
/// and misses both count in full (decision 71: the statistics are about the
/// test case, which can realize many timelines, not about tracking one
/// realized timeline).
#[derive(Clone, Copy, Default)]
pub(crate) struct Evidence {
    fails: u64,
    runs: u64,
}

impl Evidence {
    pub(crate) fn record(&mut self, failed: bool) {
        self.runs += 1;
        if failed {
            self.fails += 1;
        }
    }

    pub(crate) fn fails(&self) -> u64 {
        self.fails
    }

    pub(crate) fn runs(&self) -> u64 {
        self.runs
    }

    /// Wilson lower confidence bound on the failure rate.
    pub(crate) fn lower_bound(&self) -> f64 {
        wilson_bound(self.fails as f64, self.runs as f64, false)
    }

    /// Wilson upper confidence bound on the failure rate.
    pub(crate) fn upper_bound(&self) -> f64 {
        wilson_bound(self.fails as f64, self.runs as f64, true)
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
/// replays per rejected fluke, ~4.4 per p = 0.9 confirmation. Since
/// decision 71 the arithmetic runs on plain counts — exactly the Bernoulli
/// setting the DP modelled.
pub(crate) fn discovery_bar(evidence: &Evidence) -> BarVerdict {
    if evidence.fails >= CONFIRM_MIN_FAILS {
        return BarVerdict::Accept;
    }
    if evidence.fails == 0 && evidence.runs >= GATE_RUNS {
        return BarVerdict::Reject;
    }
    if evidence.fails + CONFIRM_CAP.saturating_sub(evidence.runs) < CONFIRM_MIN_FAILS {
        return BarVerdict::Reject;
    }
    BarVerdict::Continue
}

/// Discovery-bar batches one origin may spend per run across the sweep,
/// shrink admission, and the pooled review (decision 72): recycling a
/// re-sighted origin into fresh batches compounds the bar's per-batch
/// false accept without bound over a long run (21% per q = 0.02 fluke by
/// 200 sweep epochs, experiment 014); five attempts pin it at 2.9% and
/// keep >95% power at the target rate. At the cap the origin is rejected
/// without a batch.
pub(crate) const BAR_ATTEMPTS_PER_RUN: u64 = 5;

/// Bar attempts per origin across its backtracks (gate G25): a probed
/// entry reaches the bar at roughly its true reproduction rate, each
/// attempt holds 45% target-regime power, and three compose to ~83%. A
/// budget separate from [`BAR_ATTEMPTS_PER_RUN`] because backtrack
/// candidates come from history, which is disproportionately the real
/// bug's pre-flip sightings (experiment 014's mixed-origin table); the
/// two compose to a per-origin ceiling of eight batches, 4.6% false
/// confirm per q = 0.02 fluke.
pub(crate) const BACKTRACK_BAR_ATTEMPTS: u64 = 3;

pub(crate) const GAUNTLET_CAP: u64 = 30;
pub(crate) const GAUNTLET_GAMMA: f64 = 0.8;

/// Floor of the accept threshold, derived in experiment 008: 0.05 <
/// LCB(4/30) = 0.0531, the [`GAUNTLET_MIN_FAILS`] acceptance boundary at
/// [`GAUNTLET_CAP`], so the floor costs no power — and it is the largest
/// such value, at 0.02 false accepts per entered gauntlet against a
/// q = 0.02 fluke.
pub(crate) const GAUNTLET_FLOOR: f64 = 0.05;

/// Base failure minimum for a gauntlet accept. A single failure on a fresh
/// ledger bounds the rate above 0.2065 (Wilson at 1/1), so without a
/// minimum every threshold below that accepts on the recruiting run and the
/// whole low-anchor regime degenerates to single-run accepts (experiment
/// 008, H1: 33% bug loss at the decision-16 target). Falling short is never
/// grounds to reject — the verdict stays Continue and evidence accumulates.
/// [`GauntletSpend`] escalates the minimum within a run (decision 72).
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

/// The highest anchor an accept may raise an incumbent to (decision 77):
/// the all-fail LCB at [`ANCHOR_SEED_RUNS`], the resolution the anchor was
/// seeded at. An accept needs its LCB at or above the anchor and then
/// becomes the anchor, so raising to the LCB of longer ledgers ratchets
/// the anchor up a little on every accept until candidates need more
/// straight fails than any ledger can hold.
pub(crate) fn anchor_ceiling() -> f64 {
    let mut evidence = Evidence::default();
    for _ in 0..ANCHOR_SEED_RUNS {
        evidence.record(true);
    }
    evidence.lower_bound()
}

pub(crate) enum GauntletVerdict {
    Accept,
    Reject,
    Continue,
}

/// The accept threshold the gauntlet prices candidates against for an
/// incumbent at `anchor`: `max(gamma * anchor, GAUNTLET_FLOOR)`, with gamma
/// [`GAUNTLET_GAMMA`] below [`RETENTION_HIGH_WATER`] and 1.0 at or above
/// it.
pub(crate) fn gauntlet_threshold(anchor: f64) -> f64 {
    let gamma = if anchor >= RETENTION_HIGH_WATER {
        1.0
    } else {
        GAUNTLET_GAMMA
    };
    (gamma * anchor).max(GAUNTLET_FLOOR)
}

/// The shrink-candidate gauntlet (decisions 7, 17, 54): accept when the
/// evidence carries `min_fails` failures and its lower bound clears
/// [`gauntlet_threshold`]; reject when the upper bound proves the threshold
/// unreachable or the cap is spent, otherwise keep rerunning — short of the
/// failure minimum the verdict is Continue, never Reject. `min_fails` is
/// [`GAUNTLET_MIN_FAILS`] until the alpha budget escalates it (decision 72),
/// and is pinned per candidate: a stopping rule never changes mid-test.
/// Operating points: the exact-DP rows in experiments 008 and 014, pinned
/// by `gauntlet_matches_the_008_operating_points`; the check-per-run
/// stopping bias is absorbed in those numbers (z stays 1.96). The caller
/// raises its monotone anchor from the accepted candidate's topped-up
/// ledger — never lowering it, and post-accept re-measurement of the
/// standing incumbent never feeds it (decision 19).
pub(crate) fn gauntlet(evidence: &Evidence, anchor: f64, min_fails: u64) -> GauntletVerdict {
    gauntlet_at(evidence, gauntlet_threshold(anchor), min_fails)
}

fn gauntlet_at(evidence: &Evidence, threshold: f64, min_fails: u64) -> GauntletVerdict {
    if evidence.fails >= min_fails && evidence.lower_bound() >= threshold {
        return GauntletVerdict::Accept;
    }
    if evidence.upper_bound() < threshold || evidence.runs >= GAUNTLET_CAP {
        return GauntletVerdict::Reject;
    }
    GauntletVerdict::Continue
}

/// False-accept budget one origin's gauntlet proposals share per run
/// (decision 72): every proposal is charged its exact false-accept mass
/// against a [`CHARGE_FLUKE_RATE`] fluke before it runs, so expected false
/// accepts stay under the budget however many candidates the body realizes
/// — the uncharged rule's exposure reaches 33% by a thousand
/// floor-threshold proposals (experiment 014). At the floor the budget
/// affords ~50 fast-sweep proposals before the failure minimum escalates.
pub(crate) const GAUNTLET_ALPHA_BUDGET: f64 = 0.02;

/// Escalation ceiling for the failure minimum: at eight required failures
/// a proposal's charge is at most ~1e-7, so proposals past an exhausted
/// budget still run instead of stalling the shrink, adding a negligible
/// tail (experiment 014).
pub(crate) const GAUNTLET_MIN_FAILS_CEILING: u64 = 8;

/// The design fluke rate charges are priced against (decisions 16, 23).
const CHARGE_FLUKE_RATE: f64 = 0.02;

/// One origin's per-run gauntlet alpha-spending state (decision 72).
pub(crate) struct GauntletSpend {
    remaining: f64,
    min_fails: u64,
}

impl Default for GauntletSpend {
    fn default() -> Self {
        GauntletSpend {
            remaining: GAUNTLET_ALPHA_BUDGET,
            min_fails: GAUNTLET_MIN_FAILS,
        }
    }
}

impl GauntletSpend {
    /// Charge one gauntlet proposal and return the failure minimum its
    /// verdicts use. `seed` is the candidate's ledger before the proposal;
    /// the charge is the proposal's false-accept mass against a
    /// [`CHARGE_FLUKE_RATE`] fluke — recruit-then-drive in a fast sweep,
    /// drive-to-bound under `drive` (a confirmation sweep). A candidate
    /// already `pinned` is charged at its own minimum even past the budget
    /// (its stopping rule cannot change, and per-candidate overdraft is
    /// bounded by one charge); a new candidate is pinned at the current
    /// minimum, escalated up to [`GAUNTLET_MIN_FAILS_CEILING`] first when
    /// the remainder cannot afford it. An unreachable threshold charges
    /// zero — the high-anchor cost lottery (experiment 012) spends nothing.
    pub(crate) fn charge(
        &mut self,
        seed: &Evidence,
        anchor: f64,
        drive: bool,
        pinned: Option<u64>,
    ) -> u64 {
        let threshold = gauntlet_threshold(anchor);
        let alpha = |min_fails: u64| {
            if drive {
                gauntlet_alpha(*seed, threshold, min_fails)
            } else {
                let recruited = Evidence {
                    fails: seed.fails + 1,
                    runs: seed.runs + 1,
                };
                CHARGE_FLUKE_RATE * gauntlet_alpha(recruited, threshold, min_fails)
            }
        };
        if let Some(min_fails) = pinned {
            self.remaining -= alpha(min_fails);
            return min_fails;
        }
        loop {
            let charge = alpha(self.min_fails);
            if charge <= self.remaining || self.min_fails >= GAUNTLET_MIN_FAILS_CEILING {
                self.remaining -= charge;
                return self.min_fails;
            }
            self.min_fails += 1;
        }
    }
}

/// P(the evidence loop accepts | the candidate is a [`CHARGE_FLUKE_RATE`]
/// fluke): exact DP over the (runs, fails) probability mass from `seed`
/// under the per-replay verdict checks — the false-accept mass one driven
/// proposal contributes (experiment 014). Terminates because every
/// Continue adds a run and [`GAUNTLET_CAP`] runs force a verdict.
fn gauntlet_alpha(seed: Evidence, threshold: f64, min_fails: u64) -> f64 {
    let mut mass = BTreeMap::from([((seed.runs, seed.fails), 1.0f64)]);
    let mut accept = 0.0;
    while !mass.is_empty() {
        let mut next = BTreeMap::new();
        for (&(runs, fails), &m) in &mass {
            match gauntlet_at(&Evidence { fails, runs }, threshold, min_fails) {
                GauntletVerdict::Accept => accept += m,
                GauntletVerdict::Reject => {}
                GauntletVerdict::Continue => {
                    *next.entry((runs + 1, fails + 1)).or_insert(0.0) += m * CHARGE_FLUKE_RATE;
                    *next.entry((runs + 1, fails)).or_insert(0.0) += m * (1.0 - CHARGE_FLUKE_RATE);
                }
            }
        }
        mass = next;
    }
    accept
}

/// Total stored timelines per origin, incumbent included — the invariant
/// every stored, persisted, or replayed pool obeys
/// (`counterexample::pooled_timelines` builds them; `Counterexample`'s
/// `confirm` and `trust` truncate incoming pools). Decision 22 measured K=5 as near-ceiling and
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

/// Candidate perturbations per ND targeting race — [`BOOST_POOL`], the same
/// halving-race width boost runs (decision 68).
pub(crate) const TARGET_ND_POOL: usize = 16;

/// Fresh replays scoring an ND targeting race's winner against the
/// reference, and re-estimating the reference after an adoption —
/// [`ANCHOR_SEED_RUNS`], so score references are estimated on the same
/// batch size as anchors. Experiment 013: at 20 the sign test needs 15
/// beats (LCB(15/20) = 0.53), passing a true 75%-beat improvement 62% of
/// the time per gate at a 2.1% false-adopt rate; 10 runs need 9 beats
/// (24% power) and stall on tie-heavy scores, and 30 buys nothing over 20
/// for ~15% more replays.
pub(crate) const TARGET_ND_HOLDOUT: u64 = ANCHOR_SEED_RUNS;

/// Races per targeting firing under ND handling. A race costs ~130
/// halving replays plus pool probes and up to two holdout batches, and a
/// race that adopts is always followed by another, so a live gradient is
/// climbed until this cap. Experiment 013: at four races a full run costs
/// ~950 replays (~410 per adopted step) and reaches the landscape maximum
/// everywhere the race can move; two races reach it too at roughly half
/// the cost, and eight double the cost and the flat-landscape false-adopt
/// rate for no progress.
pub(crate) const TARGET_ND_RACES: u64 = 4;

/// Adoption rule for an ND targeting race winner (decision 68): the Wilson
/// lower bound of `beats / runs` must clear 0.5, where a beat is a fresh
/// holdout run whose score strictly exceeds the reference — a sign test
/// that the winner's median score beats the reference, with ties and
/// unobserved runs counting against. Winner's-curse-resistant because the
/// holdout is fresh and the reference was never estimated from a selected
/// maximum.
pub(crate) fn target_adopt(beats: u64, runs: u64) -> bool {
    wilson_bound(beats as f64, runs as f64, false) > 0.5
}

/// Reference score for an ND target: the upper-middle order statistic of
/// the observed scores, so an even-sized batch reads conservatively high
/// and adoption stays strict. `None` when nothing was observed.
pub(crate) fn target_median(scores: &[f64]) -> Option<f64> {
    if scores.is_empty() {
        return None;
    }
    let mut sorted = scores.to_vec();
    sorted.sort_by(f64::total_cmp);
    Some(sorted[sorted.len() / 2])
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
        CacheMismatch,
        FirstCheck,
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
        Backtrack {
            origin: String,
            restored: Vec<ChoiceValue>,
            history_best: Vec<ChoiceValue>,
            history_bytes: usize,
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

#[cfg(test)]
#[path = "../../../tests/embedded/native/nd/mod_tests.rs"]
mod tests;

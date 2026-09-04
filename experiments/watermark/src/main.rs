//! Experiment 009a: off-ceiling watermark measurement on racy bodies.
//! Spec and results: notes/experiments/009a-watermark/notes.md
//!
//! The `composed` subcommand is experiment 009b: the same episode protocol
//! re-collected against the composed-rules engine, with the bar/gauntlet
//! replay mirroring the post-phase-12 arithmetic.
//! Spec and results: notes/experiments/009b-composed-rules/notes.md

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use hegel::__bench::{watermark_dump, ChoiceValue};
use hegel::generators as gs;
use hegel::{Hegel, Phase, Settings, TestCase, Verbosity};

const EPISODES: usize = 200;
const SIM_STREAMS: usize = 10_000;
const RESERVOIR: usize = 400_000;
const PS: [f64; 3] = [0.1, 0.3, 0.9];
const BODIES: [Body; 2] = [Body::CloneStream, Body::Machine];
const MARKER: &str = "watermark: lost update";

const CLONE_ROUNDS: usize = 8;
const CLONE_RACE: f64 = 0.15;
const MACHINE_STEPS: usize = 4;
const MACHINE_RACE: f64 = 0.2;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Seed scheme, fixed in code so outputs are byte-identical across reruns:
/// cell = body·3 + p-index; episode i salts 1 (hidden schedule), 2 (discovery
/// engine), 3 (reuse engine), 4 (blob engine); offline sims salt 100+.
fn seed(body: usize, pi: usize, i: usize, salt: u64) -> u64 {
    let cell = (body * PS.len() + pi) as u64;
    (cell
        .wrapping_mul(1_000_003)
        .wrapping_add(i as u64)
        .wrapping_add(1))
    .wrapping_mul(0x9E3779B97F4A7C15)
        ^ salt.wrapping_mul(0xA5A5_5A5A_C3C3_3C3C)
}

// The evidence arithmetic below mirrors hegel-c/src/native/nd/mod.rs at the
// measured commit, the same convention as experiments/shrink-sim's model.

#[derive(Clone, Copy, Default)]
struct Evidence {
    fails: u64,
    physical: u64,
    weighted_misses: f64,
}

impl Evidence {
    fn record(&mut self, failed: bool, weight: f64) {
        self.physical += 1;
        if failed {
            self.fails += 1;
        } else {
            self.weighted_misses += weight;
        }
    }

    fn weighted_total(&self) -> f64 {
        self.fails as f64 + self.weighted_misses
    }

    fn lower_bound(&self) -> f64 {
        wilson(self.fails as f64, self.weighted_total(), false)
    }

    fn upper_bound(&self) -> f64 {
        wilson(self.fails as f64, self.weighted_total(), true)
    }
}

fn wilson(fails: f64, runs: f64, upper: bool) -> f64 {
    if runs <= 0.0 {
        return if upper { 1.0 } else { 0.0 };
    }
    let z = 1.96f64;
    let p = fails / runs;
    let z2 = z * z;
    let denom = 1.0 + z2 / runs;
    let center = p + z2 / (2.0 * runs);
    let margin = z * ((p * (1.0 - p) + z2 / (4.0 * runs)) / runs).sqrt();
    let bound = if upper {
        (center + margin) / denom
    } else {
        (center - margin) / denom
    };
    bound.clamp(0.0, 1.0)
}

const GATE_RUNS: u64 = 10;
const CONFIRM_CAP: u64 = 40;
const CONFIRM_MIN_FAILS: u64 = 4;
const GAUNTLET_CAP: u64 = 30;
const GAUNTLET_GAMMA: f64 = 0.8;
const GAUNTLET_FLOOR: f64 = 0.05;

enum BarVerdict {
    Accept,
    Reject,
    Continue,
}

fn discovery_bar(evidence: &Evidence) -> BarVerdict {
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

#[derive(Clone, Copy, PartialEq)]
enum Body {
    CloneStream,
    Machine,
}

impl Body {
    fn name(self) -> &'static str {
        match self {
            Body::CloneStream => "clone",
            Body::Machine => "machine",
        }
    }

    fn idx(self) -> usize {
        match self {
            Body::CloneStream => 0,
            Body::Machine => 1,
        }
    }

    fn run(self, tc: &TestCase, hidden: &mut Rng, p: f64) {
        match self {
            Body::CloneStream => clone_stream_body(tc, hidden, p),
            Body::Machine => machine_body(tc, hidden, p),
        }
    }
}

/// One clone stream of scalar work draws; the hidden schedule injects retry
/// draws (a disjoint value range, so a shifted replay cannot pun) at
/// CLONE_RACE per round, and the failure fires at rate `p` independent of
/// the drawn values, so every timeline's true reproduction rate is `p`.
fn clone_stream_body(tc: &TestCase, hidden: &mut Rng, p: f64) {
    let child = tc.clone();
    for _ in 0..CLONE_ROUNDS {
        child.draw(gs::integers::<i64>().min_value(0).max_value(9));
        if hidden.f64() < CLONE_RACE {
            child.draw(gs::integers::<i64>().min_value(100).max_value(109));
        }
    }
    if hidden.f64() < p {
        panic!("{MARKER}");
    }
}

/// Two worker clone streams shaped like a concurrent state machine run: each
/// worker-step draws a rule and an argument, and contention (MACHINE_RACE
/// per worker-step) adds a re-read draw. Ranges are disjoint per role.
fn machine_body(tc: &TestCase, hidden: &mut Rng, p: f64) {
    let workers = [tc.clone(), tc.clone()];
    for _ in 0..MACHINE_STEPS {
        for worker in &workers {
            worker.draw(gs::integers::<i64>().min_value(0).max_value(2));
            worker.draw(gs::integers::<i64>().min_value(10).max_value(19));
            if hidden.f64() < MACHINE_RACE {
                worker.draw(gs::integers::<i64>().min_value(100).max_value(199));
            }
        }
    }
    if hidden.f64() < p {
        panic!("{MARKER}");
    }
}

/// The pre-decision-45 weighting: matched scalar prefix over stored element
/// count, a diverged clone pair earning nothing.
fn old_weight(stored: &[ChoiceValue], realized: &[ChoiceValue]) -> f64 {
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

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

/// Run one Hegel invocation with captured output. Returns the panic message
/// (None if the run passed) and the captured output lines.
fn run_one(build: impl FnOnce()) -> (Option<String>, Vec<String>) {
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink_lines = Arc::clone(&lines);
    let sink: Arc<dyn Fn(&str) + Send + Sync> =
        Arc::new(move |s: &str| sink_lines.lock().unwrap().push(s.to_string()));
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        hegel::with_output_override(sink, build);
    }));
    let msg = outcome.err().map(|p| panic_text(&*p));
    let captured = lines.lock().unwrap().clone();
    (msg, captured)
}

fn reproduced(msg: &Option<String>) -> bool {
    msg.as_deref().is_some_and(|m| m.contains(MARKER))
}

fn extract_blob(line: &str) -> Option<String> {
    let needle = "reproduce_failure(\"";
    let start = line.find(needle)? + needle.len();
    let rest = &line[start..];
    Some(rest[..rest.find('"')?].to_string())
}

struct Episode {
    misses: Vec<(f64, f64)>,
    fail_samples: usize,
    reported: bool,
    with_blob: bool,
    reuse_reproduced: Option<bool>,
    blob_reproduced: Option<bool>,
}

fn run_episode(body: Body, pi: usize, i: usize) -> Episode {
    let p = PS[pi];
    let hidden = Rc::new(RefCell::new(Rng::new(seed(body.idx(), pi, i, 1))));
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db").to_str().unwrap().to_string();

    watermark_dump::drain();
    let body_fn = {
        let hidden = Rc::clone(&hidden);
        move |tc: TestCase| body.run(&tc, &mut hidden.borrow_mut(), p)
    };
    let (msg, lines) = run_one(|| {
        Hegel::new(body_fn)
            .__database_key("watermark".to_string())
            .settings(
                Settings::new()
                    .test_cases(100)
                    .database(Some(db.clone()))
                    .seed(Some(seed(body.idx(), pi, i, 2)))
                    .print_blob(true)
                    .verbosity(Verbosity::Quiet),
            )
            .run();
    });
    let samples = watermark_dump::drain();
    let mut misses = Vec::new();
    let mut fail_samples = 0;
    for s in &samples {
        if s.failed {
            fail_samples += 1;
        } else {
            misses.push((s.weight, old_weight(&s.stored, &s.realized)));
        }
    }
    let reported = reproduced(&msg);
    let blob = lines.iter().find_map(|l| extract_blob(l));

    let mut reuse_reproduced = None;
    let mut blob_reproduced = None;
    if let Some(blob) = &blob {
        let body_fn = {
            let hidden = Rc::clone(&hidden);
            move |tc: TestCase| body.run(&tc, &mut hidden.borrow_mut(), p)
        };
        let (msg, _) = run_one(|| {
            Hegel::new(body_fn)
                .__database_key("watermark".to_string())
                .settings(
                    Settings::new()
                        .database(Some(db.clone()))
                        .phases([Phase::Reuse])
                        .seed(Some(seed(body.idx(), pi, i, 3)))
                        .verbosity(Verbosity::Quiet),
                )
                .run();
        });
        watermark_dump::drain();
        reuse_reproduced = Some(reproduced(&msg));

        let body_fn = {
            let hidden = Rc::clone(&hidden);
            move |tc: TestCase| body.run(&tc, &mut hidden.borrow_mut(), p)
        };
        let (msg, _) = run_one(|| {
            Hegel::new(body_fn)
                .settings(
                    Settings::new()
                        .database(None)
                        .seed(Some(seed(body.idx(), pi, i, 4)))
                        .verbosity(Verbosity::Quiet),
                )
                .reproduce_failure(blob.clone())
                .run();
        });
        watermark_dump::drain();
        blob_reproduced = Some(reproduced(&msg));
    }

    Episode {
        misses,
        fail_samples,
        reported,
        with_blob: blob.is_some(),
        reuse_reproduced,
        blob_reproduced,
    }
}

struct Cell {
    body: Body,
    p: f64,
    reported: usize,
    with_blob: usize,
    miss_seen: usize,
    miss_new: Vec<f64>,
    miss_old: Vec<f64>,
    fail_samples: usize,
    reuse_hits: usize,
    reuse_tries: usize,
    blob_hits: usize,
    blob_tries: usize,
}

impl Cell {
    /// Reservoir-sample the paired (new, old) miss weights at [`RESERVOIR`]
    /// so the heavy cells stay in memory; `miss_seen` keeps the exact count.
    fn push_miss(&mut self, new: f64, old: f64, rng: &mut Rng) {
        self.miss_seen += 1;
        if self.miss_new.len() < RESERVOIR {
            self.miss_new.push(new);
            self.miss_old.push(old);
        } else {
            let j = rng.below(self.miss_seen);
            if j < RESERVOIR {
                self.miss_new[j] = new;
                self.miss_old[j] = old;
            }
        }
    }
}

fn run_cell(body: Body, pi: usize) -> Cell {
    let mut cell = Cell {
        body,
        p: PS[pi],
        reported: 0,
        with_blob: 0,
        miss_seen: 0,
        miss_new: Vec::new(),
        miss_old: Vec::new(),
        fail_samples: 0,
        reuse_hits: 0,
        reuse_tries: 0,
        blob_hits: 0,
        blob_tries: 0,
    };
    let mut reservoir_rng = Rng::new(seed(body.idx(), pi, 0, 5));
    for i in 0..EPISODES {
        let ep = run_episode(body, pi, i);
        cell.reported += ep.reported as usize;
        cell.with_blob += ep.with_blob as usize;
        for (new, old) in ep.misses {
            cell.push_miss(new, old, &mut reservoir_rng);
        }
        cell.fail_samples += ep.fail_samples;
        if let Some(hit) = ep.reuse_reproduced {
            cell.reuse_tries += 1;
            cell.reuse_hits += hit as usize;
        }
        if let Some(hit) = ep.blob_reproduced {
            cell.blob_tries += 1;
            cell.blob_hits += hit as usize;
        }
    }
    eprintln!(
        "cell {}/p={} done: {} miss samples",
        body.name(),
        PS[pi],
        cell.miss_seen
    );
    cell
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

fn pctl(values: &[f64], q: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let rank = ((q * sorted.len() as f64).ceil() as usize).max(1) - 1;
    sorted[rank.min(sorted.len() - 1)]
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn share(values: &[f64], pred: impl Fn(f64) -> bool) -> f64 {
    values.iter().filter(|&&v| pred(v)).count() as f64 / values.len() as f64
}

/// One discovery-bar episode at true failure rate `p`, miss weights resampled
/// from `weights`. Returns (accepted, physical cost, LCB at accept).
fn bar_stream(p: f64, weights: &[f64], rng: &mut Rng) -> (bool, u64, f64) {
    let mut e = Evidence::default();
    loop {
        let failed = rng.f64() < p;
        let w = if failed {
            1.0
        } else {
            weights[rng.below(weights.len())]
        };
        e.record(failed, w);
        match discovery_bar(&e) {
            BarVerdict::Accept => return (true, e.physical, e.lower_bound()),
            BarVerdict::Reject => return (false, e.physical, 0.0),
            BarVerdict::Continue => {}
        }
    }
}

enum GauntletOutcome {
    Accept,
    ProofReject,
    CapReject,
}

/// One gauntlet episode for a fluke candidate (recruiting failure counted at
/// full weight, reruns never fail), verdict checked before each rerun as the
/// probe does. Returns the outcome and the physical cost including the
/// recruit.
fn gauntlet_stream(weights: &[f64], anchor: f64, rng: &mut Rng) -> (GauntletOutcome, u64) {
    let threshold = (GAUNTLET_GAMMA * anchor).max(GAUNTLET_FLOOR);
    let mut e = Evidence::default();
    e.record(true, 1.0);
    loop {
        if e.lower_bound() >= threshold {
            return (GauntletOutcome::Accept, e.physical);
        }
        if e.upper_bound() < threshold {
            return (GauntletOutcome::ProofReject, e.physical);
        }
        if e.physical >= GAUNTLET_CAP {
            return (GauntletOutcome::CapReject, e.physical);
        }
        e.record(false, weights[rng.below(weights.len())]);
    }
}

struct BarNumbers {
    fluke_cost_med: f64,
    fluke_cost_mean: f64,
    anchor_med: f64,
    accept_share: f64,
}

fn bar_numbers(p: f64, weights: &[f64], seed: u64) -> BarNumbers {
    let mut rng = Rng::new(seed);
    let mut fluke_costs = Vec::new();
    for _ in 0..SIM_STREAMS {
        let (_, cost, _) = bar_stream(0.0, weights, &mut rng);
        fluke_costs.push(cost as f64);
    }
    let mut anchors = Vec::new();
    let mut accepts = 0usize;
    for _ in 0..SIM_STREAMS {
        let (accepted, _, lcb) = bar_stream(p, weights, &mut rng);
        if accepted {
            accepts += 1;
            anchors.push(lcb);
        }
    }
    BarNumbers {
        fluke_cost_med: median(&fluke_costs),
        fluke_cost_mean: mean(&fluke_costs),
        anchor_med: if anchors.is_empty() {
            f64::NAN
        } else {
            median(&anchors)
        },
        accept_share: accepts as f64 / SIM_STREAMS as f64,
    }
}

struct GauntletNumbers {
    anchor: f64,
    accept_share: f64,
    reject_cost_med: f64,
    proof_share: f64,
}

fn gauntlet_numbers(anchor: f64, weights: &[f64], seed: u64) -> GauntletNumbers {
    let mut rng = Rng::new(seed);
    let mut reject_costs = Vec::new();
    let mut accepts = 0usize;
    let mut proofs = 0usize;
    for _ in 0..SIM_STREAMS {
        let (outcome, cost) = gauntlet_stream(weights, anchor, &mut rng);
        match outcome {
            GauntletOutcome::Accept => accepts += 1,
            GauntletOutcome::ProofReject => {
                proofs += 1;
                reject_costs.push(cost as f64);
            }
            GauntletOutcome::CapReject => reject_costs.push(cost as f64),
        }
    }
    GauntletNumbers {
        anchor,
        accept_share: accepts as f64 / SIM_STREAMS as f64,
        reject_cost_med: if reject_costs.is_empty() {
            f64::NAN
        } else {
            median(&reject_costs)
        },
        proof_share: if reject_costs.is_empty() {
            f64::NAN
        } else {
            proofs as f64 / reject_costs.len() as f64
        },
    }
}

// The composed arithmetic below mirrors hegel-c/src/native/nd/mod.rs after
// phase 12 (decisions 54-56).

const GAUNTLET_MIN_FAILS: u64 = 4;
const RETENTION_HIGH_WATER: f64 = 0.8;
const ANCHOR_SEED_RUNS: u64 = 20;
const FLUKE_RATE: f64 = 0.02;

/// One composed-bar episode: the 009a bar plus decision 54's extension — an
/// accepted batch keeps replaying to [`ANCHOR_SEED_RUNS`] physical runs, and
/// the anchor is the extended batch's LCB. Rejects stop at the bar.
fn composed_bar_stream(p: f64, weights: &[f64], rng: &mut Rng) -> (bool, u64, f64) {
    let mut e = Evidence::default();
    let accepted = loop {
        let failed = rng.f64() < p;
        let w = if failed {
            1.0
        } else {
            weights[rng.below(weights.len())]
        };
        e.record(failed, w);
        match discovery_bar(&e) {
            BarVerdict::Accept => break true,
            BarVerdict::Reject => break false,
            BarVerdict::Continue => {}
        }
    };
    while accepted && e.physical < ANCHOR_SEED_RUNS {
        let failed = rng.f64() < p;
        let w = if failed {
            1.0
        } else {
            weights[rng.below(weights.len())]
        };
        e.record(failed, w);
    }
    (accepted, e.physical, e.lower_bound())
}

fn composed_threshold(anchor: f64) -> f64 {
    let gamma = if anchor >= RETENTION_HIGH_WATER {
        1.0
    } else {
        GAUNTLET_GAMMA
    };
    (gamma * anchor).max(GAUNTLET_FLOOR)
}

/// One composed-gauntlet episode for a candidate recruited on a failure
/// (ledgered at weight 1.0) whose reruns fail at rate `q`: verdict before
/// each rerun as the probe does, accept requires [`GAUNTLET_MIN_FAILS`], and
/// an accept tops the ledger up to [`ANCHOR_SEED_RUNS`] before returning.
fn composed_gauntlet_stream(
    q: f64,
    weights: &[f64],
    anchor: f64,
    rng: &mut Rng,
) -> (GauntletOutcome, u64) {
    let threshold = composed_threshold(anchor);
    let mut e = Evidence::default();
    e.record(true, 1.0);
    let mut accepted = false;
    loop {
        if !accepted {
            if e.fails >= GAUNTLET_MIN_FAILS && e.lower_bound() >= threshold {
                accepted = true;
            } else if e.upper_bound() < threshold {
                return (GauntletOutcome::ProofReject, e.physical);
            } else if e.physical >= GAUNTLET_CAP {
                return (GauntletOutcome::CapReject, e.physical);
            }
        }
        if accepted && e.physical >= ANCHOR_SEED_RUNS {
            return (GauntletOutcome::Accept, e.physical);
        }
        let failed = rng.f64() < q;
        let w = if failed {
            1.0
        } else {
            weights[rng.below(weights.len())]
        };
        e.record(failed, w);
    }
}

fn composed_bar_numbers(p: f64, weights: &[f64], seed: u64) -> BarNumbers {
    let mut rng = Rng::new(seed);
    let mut fluke_costs = Vec::new();
    for _ in 0..SIM_STREAMS {
        let (_, cost, _) = composed_bar_stream(0.0, weights, &mut rng);
        fluke_costs.push(cost as f64);
    }
    let mut anchors = Vec::new();
    let mut accepts = 0usize;
    for _ in 0..SIM_STREAMS {
        let (accepted, _, lcb) = composed_bar_stream(p, weights, &mut rng);
        if accepted {
            accepts += 1;
            anchors.push(lcb);
        }
    }
    BarNumbers {
        fluke_cost_med: median(&fluke_costs),
        fluke_cost_mean: mean(&fluke_costs),
        anchor_med: if anchors.is_empty() {
            f64::NAN
        } else {
            median(&anchors)
        },
        accept_share: accepts as f64 / SIM_STREAMS as f64,
    }
}

struct ComposedGauntletNumbers {
    reject_cost_med: f64,
    proof_share: f64,
    accept_share: f64,
    false_accept: f64,
}

/// Fluke pricing at rate 0 (reject cost, proof share, accept share) plus the
/// false-accept share at rate [`FLUKE_RATE`], conditional on the recruiting
/// failure — 008's DP row prices the same conditional event.
fn composed_gauntlet_numbers(anchor: f64, weights: &[f64], seed: u64) -> ComposedGauntletNumbers {
    let mut rng = Rng::new(seed);
    let mut reject_costs = Vec::new();
    let mut accepts = 0usize;
    let mut proofs = 0usize;
    for _ in 0..SIM_STREAMS {
        let (outcome, cost) = composed_gauntlet_stream(0.0, weights, anchor, &mut rng);
        match outcome {
            GauntletOutcome::Accept => accepts += 1,
            GauntletOutcome::ProofReject => {
                proofs += 1;
                reject_costs.push(cost as f64);
            }
            GauntletOutcome::CapReject => reject_costs.push(cost as f64),
        }
    }
    let mut false_accepts = 0usize;
    for _ in 0..SIM_STREAMS {
        if matches!(
            composed_gauntlet_stream(FLUKE_RATE, weights, anchor, &mut rng).0,
            GauntletOutcome::Accept
        ) {
            false_accepts += 1;
        }
    }
    ComposedGauntletNumbers {
        reject_cost_med: if reject_costs.is_empty() {
            f64::NAN
        } else {
            median(&reject_costs)
        },
        proof_share: if reject_costs.is_empty() {
            f64::NAN
        } else {
            proofs as f64 / reject_costs.len() as f64
        },
        accept_share: accepts as f64 / SIM_STREAMS as f64,
        false_accept: false_accepts as f64 / SIM_STREAMS as f64,
    }
}

fn run_composed() {
    let mut cells = Vec::new();
    for &body in &BODIES {
        for pi in 0..PS.len() {
            cells.push(run_cell(body, pi));
        }
    }

    println!("# 009b: composed-rules re-verification\n");
    println!(
        "Episode protocol, bodies, and seeds as 009a ({EPISODES} episodes per cell), \
         collected against the composed-rules engine; offline sims replay the composed \
         bar/gauntlet arithmetic (min-fails {GAUNTLET_MIN_FAILS}, floor {GAUNTLET_FLOOR}, \
         gamma 1.0 at anchors >= {RETENTION_HIGH_WATER}, {ANCHOR_SEED_RUNS}-run anchor \
         seeding at both sites) over resampled measured weights, {SIM_STREAMS} streams \
         per number.\n"
    );

    println!("## Episode accounting\n");
    println!("| body | p | reported | with blob | miss samples | fail samples |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for c in &cells {
        println!(
            "| {} | {} | {}/{} | {} | {} | {} |",
            c.body.name(),
            c.p,
            c.reported,
            EPISODES,
            c.with_blob,
            c.miss_seen,
            c.fail_samples
        );
    }

    println!("\n## Miss-weight distribution (discovery-run measurement replays)\n");
    println!("| body | p | W50 | mean | p10 | p90 | share 0 | share 1 |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    for c in &cells {
        if c.miss_new.is_empty() {
            println!("| {} | {} | (no miss samples) |", c.body.name(), c.p);
            continue;
        }
        println!(
            "| {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
            c.body.name(),
            c.p,
            median(&c.miss_new),
            mean(&c.miss_new),
            pctl(&c.miss_new, 0.1),
            pctl(&c.miss_new, 0.9),
            share(&c.miss_new, |w| w == 0.0),
            share(&c.miss_new, |w| w == 1.0),
        );
    }

    println!("\n## Discovery bar with the extended anchor batch\n");
    println!("| body | p | fluke reject med/mean | anchor med (accept) |");
    println!("| --- | --- | --- | --- |");
    let mut anchors = Vec::new();
    for (ci, c) in cells.iter().enumerate() {
        if c.miss_new.is_empty() {
            println!("| {} | {} | (no miss samples) |", c.body.name(), c.p);
            anchors.push(f64::NAN);
            continue;
        }
        let bar = composed_bar_numbers(c.p, &c.miss_new, seed(ci, 0, 0, 110));
        println!(
            "| {} | {} | {:.0} / {:.1} | {:.3} ({:.2}) |",
            c.body.name(),
            c.p,
            bar.fluke_cost_med,
            bar.fluke_cost_mean,
            bar.anchor_med,
            bar.accept_share,
        );
        anchors.push(bar.anchor_med);
    }

    println!("\n## Shrink gauntlet, fluke candidate at the cell's median confirmed anchor\n");
    println!(
        "| body | p | anchor | threshold | reject med | proof share | accept share | false accept (q={FLUKE_RATE}) | per proposal |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for (ci, c) in cells.iter().enumerate() {
        if c.miss_new.is_empty() || anchors[ci].is_nan() {
            println!("| {} | {} | (no anchor) |", c.body.name(), c.p);
            continue;
        }
        let anchor = anchors[ci];
        let g = composed_gauntlet_numbers(anchor, &c.miss_new, seed(ci, 0, 0, 111));
        println!(
            "| {} | {} | {:.3} | {:.3} | {} | {} | {:.2} | {:.4} | {:.1e} |",
            c.body.name(),
            c.p,
            anchor,
            composed_threshold(anchor),
            fmt_or_dash(g.reject_cost_med, 0),
            fmt_or_dash(g.proof_share, 2),
            g.accept_share,
            g.false_accept,
            g.false_accept * FLUKE_RATE,
        );
    }

    println!("\n## Reproduction of persisted state (composed engine)\n");
    println!("| body | p | DB reuse | blob replay |");
    println!("| --- | --- | --- | --- |");
    for c in &cells {
        println!(
            "| {} | {} | {}/{} | {}/{} |",
            c.body.name(),
            c.p,
            c.reuse_hits,
            c.reuse_tries,
            c.blob_hits,
            c.blob_tries
        );
    }
}

fn fmt_or_dash(value: f64, decimals: usize) -> String {
    if value.is_nan() {
        "-".to_string()
    } else {
        format!("{value:.decimals$}")
    }
}

fn main() {
    watermark_dump::arm();

    if std::env::args().nth(1).as_deref() == Some("composed") {
        run_composed();
        return;
    }

    let mut cells = Vec::new();
    for &body in &BODIES {
        for pi in 0..PS.len() {
            cells.push(run_cell(body, pi));
        }
    }

    println!("# 009a: off-ceiling watermark measurement\n");
    println!(
        "{} episodes per cell; race rates: clone {CLONE_RACE}/round, machine \
         {MACHINE_RACE}/worker-step; miss-weight statistics over a {}-sample \
         reservoir per cell; offline sims {} streams per number.\n",
        EPISODES, RESERVOIR, SIM_STREAMS
    );

    println!("## Episode accounting\n");
    println!("| body | p | reported | with blob | miss samples | fail samples |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for c in &cells {
        println!(
            "| {} | {} | {}/{} | {} | {} | {} |",
            c.body.name(),
            c.p,
            c.reported,
            EPISODES,
            c.with_blob,
            c.miss_seen,
            c.fail_samples
        );
    }

    println!("\n## Miss-weight distribution (discovery-run measurement replays)\n");
    println!(
        "| body | p | W50 new | mean new | p10 | p90 | share 0 | share 1 | W50 old | mean old | share 0 old |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for c in &cells {
        if c.miss_new.is_empty() {
            println!("| {} | {} | (no miss samples) |", c.body.name(), c.p);
            continue;
        }
        println!(
            "| {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
            c.body.name(),
            c.p,
            median(&c.miss_new),
            mean(&c.miss_new),
            pctl(&c.miss_new, 0.1),
            pctl(&c.miss_new, 0.9),
            share(&c.miss_new, |w| w == 0.0),
            share(&c.miss_new, |w| w == 1.0),
            median(&c.miss_old),
            mean(&c.miss_old),
            share(&c.miss_old, |w| w == 0.0),
        );
    }

    println!("\n## Discovery bar, empirical replay over resampled measured weights\n");
    println!(
        "| body | p | fluke reject med/mean new | fluke reject med/mean old | anchor med new (accept) | anchor med old (accept) |"
    );
    println!("| --- | --- | --- | --- | --- | --- |");
    let mut anchors_new = Vec::new();
    for (ci, c) in cells.iter().enumerate() {
        if c.miss_new.is_empty() {
            println!("| {} | {} | (no miss samples) |", c.body.name(), c.p);
            anchors_new.push(f64::NAN);
            continue;
        }
        let new = bar_numbers(c.p, &c.miss_new, seed(ci, 0, 0, 100));
        let old = bar_numbers(c.p, &c.miss_old, seed(ci, 0, 0, 101));
        println!(
            "| {} | {} | {:.0} / {:.1} | {:.0} / {:.1} | {:.3} ({:.2}) | {:.3} ({:.2}) |",
            c.body.name(),
            c.p,
            new.fluke_cost_med,
            new.fluke_cost_mean,
            old.fluke_cost_med,
            old.fluke_cost_mean,
            new.anchor_med,
            new.accept_share,
            old.anchor_med,
            old.accept_share,
        );
        anchors_new.push(new.anchor_med);
    }

    println!("\n## Shrink gauntlet, fluke candidate at the cell's median confirmed anchor\n");
    println!(
        "| body | p | anchor | reject med new | proof share new | accept share new | reject med old | proof share old | accept share old |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for (ci, c) in cells.iter().enumerate() {
        if c.miss_new.is_empty() || anchors_new[ci].is_nan() {
            println!("| {} | {} | (no anchor) |", c.body.name(), c.p);
            continue;
        }
        let anchor = anchors_new[ci];
        let new = gauntlet_numbers(anchor, &c.miss_new, seed(ci, 0, 0, 102));
        let old = gauntlet_numbers(anchor, &c.miss_old, seed(ci, 0, 0, 103));
        println!(
            "| {} | {} | {:.3} | {} | {} | {:.2} | {} | {} | {:.2} |",
            c.body.name(),
            c.p,
            new.anchor,
            fmt_or_dash(new.reject_cost_med, 0),
            fmt_or_dash(new.proof_share, 2),
            new.accept_share,
            fmt_or_dash(old.reject_cost_med, 0),
            fmt_or_dash(old.proof_share, 2),
            old.accept_share,
        );
    }

    println!("\n## G10: off-ceiling reproduction of persisted state\n");
    println!("| body | p | DB reuse | blob replay |");
    println!("| --- | --- | --- | --- |");
    for c in &cells {
        println!(
            "| {} | {} | {}/{} | {}/{} |",
            c.body.name(),
            c.p,
            c.reuse_hits,
            c.reuse_tries,
            c.blob_hits,
            c.blob_tries
        );
    }
}

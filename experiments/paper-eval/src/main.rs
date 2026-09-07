//! Experiment 015: consolidated evaluation on the final engine, for the
//! paper. Spec and results: notes/experiments/015-paper-eval/notes.md
//!
//! `landscapes [quiet|error] [cell]` re-runs experiment 011's landscape
//! cells (plus an N0 noise-only cell) through the public C ABI under the
//! given strictness, based on `/experiments/gauntlet-calibration`.
//! `episodes [cell]` re-runs experiment 012's episode protocol (plus a
//! clone p = 0.05 cell and a passing cell) through the Rust frontend,
//! based on `/experiments/detection-escape`.

use std::cell::RefCell;
use std::ffi::{c_void, CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use hegel::generators as gs;
use hegel::{Hegel, Phase, Settings, TestCase, Verbosity};
use hegel_c::__bench::{blob_is_nd, seam_dump};
use hegel_c::{
    hegel_result_t, hegel_run_status_t, hegel_status_t, HegelContext, HegelFailure, HegelRun,
    HegelRunResult, HegelSettings, HegelTestCase,
};

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(1))
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
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = (q * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

const SEEDS: u64 = 100;
const TEST_CASES: u64 = 500;
const BUG_ATOM: i64 = 10;
const MAX_ATOMS: i64 = 20;
const CORE_ATOM: i64 = 95;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Landscape {
    Rising,
    Constant,
    NoiseFloor,
    NoiseFloorLo,
    DetCore,
    DetOnly,
    NoiseOnly,
}

const ALL_LANDSCAPES: [Landscape; 7] = [
    Landscape::Rising,
    Landscape::Constant,
    Landscape::NoiseFloor,
    Landscape::NoiseFloorLo,
    Landscape::DetCore,
    Landscape::DetOnly,
    Landscape::NoiseOnly,
];

impl Landscape {
    fn bug_atoms(atoms: &[i64]) -> usize {
        atoms.iter().filter(|&&a| a >= BUG_ATOM).count()
    }

    fn has_core(atoms: &[i64]) -> bool {
        atoms.iter().any(|&a| a >= CORE_ATOM)
    }

    fn has_bug(self, atoms: &[i64]) -> bool {
        match self {
            Landscape::Rising => Self::bug_atoms(atoms) >= 3,
            Landscape::Constant | Landscape::NoiseFloor | Landscape::NoiseFloorLo => {
                Self::bug_atoms(atoms) >= 1
            }
            Landscape::DetCore => Self::has_core(atoms) || Self::bug_atoms(atoms) >= 3,
            Landscape::DetOnly => Self::has_core(atoms),
            Landscape::NoiseOnly => false,
        }
    }

    fn p(self, atoms: &[i64]) -> f64 {
        let has_bug = self.has_bug(atoms);
        match self {
            Landscape::Rising => {
                if has_bug {
                    (0.1 + 0.08 * (atoms.len().saturating_sub(1)) as f64).clamp(0.1, 0.95)
                } else {
                    0.0
                }
            }
            Landscape::Constant => {
                if has_bug {
                    0.5
                } else {
                    0.0
                }
            }
            Landscape::NoiseFloor => {
                if has_bug {
                    0.9
                } else {
                    0.02
                }
            }
            Landscape::NoiseFloorLo => {
                if has_bug {
                    0.1
                } else {
                    0.02
                }
            }
            Landscape::DetCore => {
                if Self::has_core(atoms) {
                    1.0
                } else if has_bug {
                    0.7
                } else {
                    0.0
                }
            }
            Landscape::DetOnly => {
                if has_bug {
                    1.0
                } else {
                    0.0
                }
            }
            Landscape::NoiseOnly => 0.02,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Landscape::Rising => "L1 rising",
            Landscape::Constant => "L3 constant",
            Landscape::NoiseFloor => "L4 noise-floor",
            Landscape::NoiseFloorLo => "L4b noise-floor-lo",
            Landscape::DetCore => "D2 det-core 0.7",
            Landscape::DetOnly => "D0 det-control",
            Landscape::NoiseOnly => "N0 noise-only",
        }
    }

    fn from_cli(name: &str) -> Landscape {
        match name {
            "l1" => Landscape::Rising,
            "l3" => Landscape::Constant,
            "l4" => Landscape::NoiseFloor,
            "l4b" => Landscape::NoiseFloorLo,
            "d2" => Landscape::DetCore,
            "d0" => Landscape::DetOnly,
            "n0" => Landscape::NoiseOnly,
            other => panic!("unknown cell {other} (expected l1, l3, l4, l4b, d2, d0, or n0)"),
        }
    }
}

struct Ctx(*mut HegelContext);

impl Ctx {
    fn new() -> Ctx {
        Ctx(hegel_c::hegel_context_new())
    }

    fn err(&self) -> String {
        let p = unsafe { hegel_c::hegel_context_last_error(self.0) };
        if p.is_null() {
            return String::new();
        }
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }

    fn ok(&self, rc: hegel_result_t) {
        assert!(
            rc == hegel_result_t::HEGEL_OK,
            "libhegel call failed ({rc:?}): {}",
            self.err()
        );
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        unsafe {
            let _ = hegel_c::hegel_context_free(self.0);
        }
    }
}

unsafe extern "C" fn discard_output(_: *mut c_void, _: *const c_char, _: usize) {}

enum BodyResult {
    Completed { atoms: Vec<i64> },
    Overrun,
}

unsafe fn drive_draws(ctx: &Ctx, tc: *mut HegelTestCase) -> BodyResult {
    let mut n: i64 = 0;
    let rc = hegel_c::hegel_generate_integer(ctx.0, tc, 0, MAX_ATOMS, &mut n);
    if rc == hegel_result_t::HEGEL_E_STOP_TEST {
        return BodyResult::Overrun;
    }
    ctx.ok(rc);
    let mut atoms = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let mut a: i64 = 0;
        let rc = hegel_c::hegel_generate_integer(ctx.0, tc, 0, 100, &mut a);
        if rc == hegel_result_t::HEGEL_E_STOP_TEST {
            return BodyResult::Overrun;
        }
        ctx.ok(rc);
        atoms.push(a);
    }
    BodyResult::Completed { atoms }
}

unsafe fn run_body(
    ctx: &Ctx,
    tc: *mut HegelTestCase,
    landscape: Landscape,
    hidden: &RefCell<Rng>,
    origin: &CStr,
) {
    let status = match drive_draws(ctx, tc) {
        BodyResult::Overrun => (hegel_status_t::HEGEL_STATUS_OVERRUN, false),
        BodyResult::Completed { atoms } => {
            let p = landscape.p(&atoms);
            if hidden.borrow_mut().f64() < p {
                (hegel_status_t::HEGEL_STATUS_INTERESTING, true)
            } else {
                (hegel_status_t::HEGEL_STATUS_VALID, false)
            }
        }
    };
    let origin_ptr = if status.1 {
        origin.as_ptr()
    } else {
        ptr::null()
    };
    ctx.ok(hegel_c::hegel_mark_complete(
        ctx.0,
        tc,
        status.0 as u32,
        origin_ptr,
    ));
}

unsafe fn replay_final(ctx: &Ctx, settings: *mut HegelSettings, blob: &CStr) -> Option<Vec<i64>> {
    let mut tc: *mut HegelTestCase = ptr::null_mut();
    ctx.ok(hegel_c::hegel_test_case_from_blob(
        ctx.0,
        settings,
        blob.as_ptr(),
        Some(discard_output),
        ptr::null_mut(),
        &mut tc,
    ));
    let result = drive_draws(ctx, tc);
    ctx.ok(hegel_c::hegel_mark_complete(
        ctx.0,
        tc,
        hegel_status_t::HEGEL_STATUS_VALID as u32,
        ptr::null(),
    ));
    ctx.ok(hegel_c::hegel_test_case_free(ctx.0, tc));
    match result {
        BodyResult::Completed { atoms } => Some(atoms),
        BodyResult::Overrun => None,
    }
}

#[derive(Clone)]
enum Outcome {
    Aborted,
    NoBug,
    CaveatOnly {
        execs: u64,
    },
    Shrunk {
        atoms: Vec<i64>,
        execs: u64,
        nd: bool,
    },
}

const HEGEL_NONDETERMINISM_ERROR: u32 = 2;

fn run_trial(landscape: Landscape, strict: bool, seed: u64) -> Outcome {
    let ctx = Ctx::new();
    let hidden = RefCell::new(Rng::new(seed.wrapping_mul(0xC0FFEE) ^ 0xD15EA5E));
    let origin = CString::new("bug").unwrap();
    unsafe {
        let mut settings: *mut HegelSettings = ptr::null_mut();
        ctx.ok(hegel_c::hegel_settings_new(ctx.0, &mut settings));
        ctx.ok(hegel_c::hegel_settings_set_test_cases(
            ctx.0, settings, TEST_CASES,
        ));
        ctx.ok(hegel_c::hegel_settings_set_verbosity(ctx.0, settings, 0));
        if strict {
            ctx.ok(hegel_c::hegel_settings_set_nondeterminism_strictness(
                ctx.0,
                settings,
                HEGEL_NONDETERMINISM_ERROR,
            ));
        }
        ctx.ok(hegel_c::hegel_settings_set_seed(
            ctx.0,
            settings,
            seed ^ 0xF00D,
            true,
        ));
        let no_db = CString::new("").unwrap();
        ctx.ok(hegel_c::hegel_settings_set_database(
            ctx.0,
            settings,
            no_db.as_ptr(),
        ));

        let mut run: *mut HegelRun = ptr::null_mut();
        ctx.ok(hegel_c::hegel_run_start(
            ctx.0,
            settings,
            Some(discard_output),
            ptr::null_mut(),
            &mut run,
        ));

        let mut execs = 0u64;
        loop {
            let mut tc: *mut HegelTestCase = ptr::null_mut();
            ctx.ok(hegel_c::hegel_next_test_case(ctx.0, run, &mut tc));
            if tc.is_null() {
                break;
            }
            execs += 1;
            run_body(&ctx, tc, landscape, &hidden, &origin);
            ctx.ok(hegel_c::hegel_test_case_free(ctx.0, tc));
        }

        let mut result: *mut HegelRunResult = ptr::null_mut();
        ctx.ok(hegel_c::hegel_run_result(ctx.0, run, &mut result));
        let mut status = hegel_run_status_t::HEGEL_RUN_STATUS_PASSED;
        ctx.ok(hegel_c::hegel_run_result_status(ctx.0, result, &mut status));

        let outcome = match status {
            hegel_run_status_t::HEGEL_RUN_STATUS_ERROR => {
                let mut msg: *const c_char = ptr::null();
                ctx.ok(hegel_c::hegel_run_result_error(ctx.0, result, &mut msg));
                Outcome::Aborted
            }
            hegel_run_status_t::HEGEL_RUN_STATUS_PASSED => Outcome::NoBug,
            hegel_run_status_t::HEGEL_RUN_STATUS_FAILED => {
                let mut count: usize = 0;
                ctx.ok(hegel_c::hegel_run_result_failure_count(
                    ctx.0, result, &mut count,
                ));
                assert!(count >= 1);
                let mut failure: *mut HegelFailure = ptr::null_mut();
                ctx.ok(hegel_c::hegel_run_result_failure(
                    ctx.0,
                    result,
                    0,
                    &mut failure,
                ));
                let mut caveat: *const c_char = ptr::null();
                ctx.ok(hegel_c::hegel_failure_caveat(ctx.0, failure, &mut caveat));
                let nd = !caveat.is_null();
                let mut blob: *const c_char = ptr::null();
                ctx.ok(hegel_c::hegel_failure_reproduction_blob(
                    ctx.0, failure, &mut blob,
                ));
                let outcome = if blob.is_null() {
                    Outcome::CaveatOnly { execs }
                } else {
                    match replay_final(&ctx, settings, CStr::from_ptr(blob)) {
                        Some(atoms) => Outcome::Shrunk { atoms, execs, nd },
                        None => Outcome::Aborted,
                    }
                };
                ctx.ok(hegel_c::hegel_failure_free(ctx.0, failure));
                outcome
            }
        };

        ctx.ok(hegel_c::hegel_run_result_free(ctx.0, result));
        ctx.ok(hegel_c::hegel_run_free(ctx.0, run));
        ctx.ok(hegel_c::hegel_settings_free(ctx.0, settings));
        outcome
    }
}

fn run_landscape_cell(landscape: Landscape, strict: bool) -> Vec<Outcome> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(SEEDS as usize);
    let mut outcomes: Vec<(u64, Outcome)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                scope.spawn(move || {
                    ((t as u64)..SEEDS)
                        .step_by(threads)
                        .map(|seed| (seed, run_trial(landscape, strict, seed)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    outcomes.sort_by_key(|(seed, _)| *seed);
    outcomes.into_iter().map(|(_, o)| o).collect()
}

fn print_landscape_row(landscape: Landscape, outcomes: &[Outcome]) {
    let aborted = outcomes
        .iter()
        .filter(|o| matches!(o, Outcome::Aborted))
        .count();
    let nobug = outcomes
        .iter()
        .filter(|o| matches!(o, Outcome::NoBug))
        .count();
    let caveat_only = outcomes
        .iter()
        .filter(|o| matches!(o, Outcome::CaveatOnly { .. }))
        .count();
    let shrunk: Vec<(&Vec<i64>, u64, bool)> = outcomes
        .iter()
        .filter_map(|o| match o {
            Outcome::Shrunk { atoms, execs, nd } => Some((atoms, *execs, *nd)),
            _ => None,
        })
        .collect();
    let n = shrunk.len();
    let bug_kept = shrunk
        .iter()
        .filter(|(atoms, _, _)| landscape.has_bug(atoms))
        .count();
    let nd_runs = shrunk.iter().filter(|(_, _, nd)| *nd).count();
    let mut ps: Vec<f64> = shrunk.iter().map(|(a, _, _)| landscape.p(a)).collect();
    let mut lens: Vec<f64> = shrunk.iter().map(|(a, _, _)| a.len() as f64).collect();
    let mut ex: Vec<f64> = outcomes
        .iter()
        .filter_map(|o| match o {
            Outcome::Shrunk { execs, .. } | Outcome::CaveatOnly { execs } => Some(*execs as f64),
            _ => None,
        })
        .collect();
    ps.sort_by(f64::total_cmp);
    lens.sort_by(f64::total_cmp);
    ex.sort_by(f64::total_cmp);
    println!(
        "| {} | {} | {} | {} | {} | {}/{} | {:.0} | {:.2} / {:.2} / {:.2} | {:.0} ({:.0}) | {} |",
        landscape.name(),
        n,
        aborted,
        nobug,
        caveat_only,
        bug_kept,
        n,
        percentile(&lens, 0.5),
        percentile(&ps, 0.1),
        percentile(&ps, 0.5),
        percentile(&ps, 0.9),
        percentile(&ex, 0.5),
        percentile(&ex, 0.9),
        nd_runs,
    );
}

fn landscapes(strict: bool, cells: &[Landscape]) {
    println!(
        "# paper-eval landscape suite, strictness {} ({SEEDS} seeds per cell, {TEST_CASES}-case budget)\n",
        if strict { "error" } else { "quiet" },
    );
    println!(
        "| cell | shrunk | aborted | no-bug | caveat-only | bug kept | len med | final p p10/p50/p90 | execs med (p90) | nd |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for &landscape in cells {
        let outcomes = run_landscape_cell(landscape, strict);
        print_landscape_row(landscape, &outcomes);
    }
}

const CLONE_PS: [f64; 4] = [0.05, 0.1, 0.3, 0.9];
const MACHINE_PS: [f64; 3] = [0.1, 0.3, 0.9];
const MARKER: &str = "paper-eval: lost update";

const CLONE_ROUNDS: usize = 8;
const CLONE_RACE: f64 = 0.15;
const MACHINE_STEPS: usize = 4;
const MACHINE_RACE: f64 = 0.2;

fn episodes_per_cell() -> usize {
    std::env::var("PAPER_EVAL_EPISODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200)
}

fn seed(cell: usize, i: usize, salt: u64) -> u64 {
    ((cell as u64)
        .wrapping_mul(1_000_003)
        .wrapping_add(i as u64)
        .wrapping_add(1))
    .wrapping_mul(0x9E3779B97F4A7C15)
        ^ salt.wrapping_mul(0xA5A5_5A5A_C3C3_3C3C)
}

#[derive(Clone, Copy, PartialEq)]
enum Cell {
    CloneStream(usize),
    Machine(usize),
    Control,
    Pass,
}

impl Cell {
    fn name(self) -> String {
        match self {
            Cell::CloneStream(pi) => format!("clone p={}", CLONE_PS[pi]),
            Cell::Machine(pi) => format!("machine p={}", MACHINE_PS[pi]),
            Cell::Control => "det-control".to_string(),
            Cell::Pass => "pass".to_string(),
        }
    }

    fn idx(self) -> usize {
        match self {
            Cell::CloneStream(pi) => pi,
            Cell::Machine(pi) => 4 + pi,
            Cell::Control => 7,
            Cell::Pass => 8,
        }
    }

    fn run(self, tc: &TestCase, hidden: &mut Rng) {
        match self {
            Cell::CloneStream(pi) => clone_stream_body(tc, hidden, CLONE_PS[pi]),
            Cell::Machine(pi) => machine_body(tc, hidden, MACHINE_PS[pi]),
            Cell::Control => control_body(tc),
            Cell::Pass => pass_body(tc, hidden),
        }
    }
}

/// One clone stream of scalar work draws; the hidden schedule injects retry
/// draws (a disjoint value range, so a shifted replay cannot pun) at
/// CLONE_RACE per round, and the failure fires at rate `p` independent of
/// the drawn values, so every timeline's true reproduction rate is `p`.
/// Verbatim from experiments 009a and 012.
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

/// Two worker clone streams shaped like a concurrent state machine run —
/// verbatim from experiments 009a and 012.
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

/// The clone stream with the hidden schedule off and the failure at p = 1:
/// every execution realizes the same shape, so the run must stay
/// deterministic and pay exactly the first check.
fn control_body(tc: &TestCase) {
    let child = tc.clone();
    for _ in 0..CLONE_ROUNDS {
        child.draw(gs::integers::<i64>().min_value(0).max_value(9));
    }
    panic!("{MARKER}");
}

/// The clone stream with the hidden schedule on and no failure: a passing
/// suite must show zero flips and zero measurement replays.
fn pass_body(tc: &TestCase, hidden: &mut Rng) {
    let child = tc.clone();
    for _ in 0..CLONE_ROUNDS {
        child.draw(gs::integers::<i64>().min_value(0).max_value(9));
        if hidden.f64() < CLONE_RACE {
            child.draw(gs::integers::<i64>().min_value(100).max_value(109));
        }
    }
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

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

fn measurement_replays(lines: &[String]) -> Option<u64> {
    let needle = "measurement replays ";
    let line = lines.iter().find(|l| l.contains(needle))?;
    let start = line.find(needle)? + needle.len();
    let digits: String = line[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn site_name(site: seam_dump::FlipSite) -> &'static str {
    match site {
        seam_dump::FlipSite::CacheMismatch => "cache",
        seam_dump::FlipSite::FirstCheck => "first-check",
        seam_dump::FlipSite::ShrinkVerify => "verify",
        seam_dump::FlipSite::FinalReplay => "final",
        seam_dump::FlipSite::StoredV2Reuse => "v2-reuse",
        seam_dump::FlipSite::StoredV2Blob => "v2-blob",
    }
}

struct Episode {
    reported: bool,
    confirmed: bool,
    flip_site: Option<&'static str>,
    blob_nd: Option<bool>,
    replays: Option<u64>,
    reuse_reproduced: Option<bool>,
    blob_reproduced: Option<bool>,
}

fn run_episode(cell: Cell, i: usize) -> Episode {
    let hidden = Rc::new(RefCell::new(Rng::new(seed(cell.idx(), i, 1))));
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db").to_str().unwrap().to_string();

    seam_dump::drain();
    let body_fn = {
        let hidden = Rc::clone(&hidden);
        move |tc: TestCase| cell.run(&tc, &mut hidden.borrow_mut())
    };
    let (msg, lines) = run_one(|| {
        Hegel::new(body_fn)
            .__database_key("paper-eval".to_string())
            .settings(
                Settings::new()
                    .test_cases(100)
                    .database(Some(db.clone()))
                    .seed(Some(seed(cell.idx(), i, 2)))
                    .print_blob(true)
                    .show_statistics(true)
                    .verbosity(Verbosity::Quiet),
            )
            .run();
    });
    let flip_site = seam_dump::drain().iter().find_map(|e| match e {
        seam_dump::SeamEvent::Flip { site, .. } => Some(site_name(*site)),
        _ => None,
    });
    let reported = reproduced(&msg);
    let blob = lines.iter().find_map(|l| extract_blob(l));
    let replays = measurement_replays(&lines);

    let mut reuse_reproduced = None;
    let mut blob_reproduced = None;
    if let Some(blob) = &blob {
        let body_fn = {
            let hidden = Rc::clone(&hidden);
            move |tc: TestCase| cell.run(&tc, &mut hidden.borrow_mut())
        };
        let (msg, _) = run_one(|| {
            Hegel::new(body_fn)
                .__database_key("paper-eval".to_string())
                .settings(
                    Settings::new()
                        .database(Some(db.clone()))
                        .phases([Phase::Reuse])
                        .seed(Some(seed(cell.idx(), i, 3)))
                        .verbosity(Verbosity::Quiet),
                )
                .run();
        });
        seam_dump::drain();
        reuse_reproduced = Some(reproduced(&msg));

        let body_fn = {
            let hidden = Rc::clone(&hidden);
            move |tc: TestCase| cell.run(&tc, &mut hidden.borrow_mut())
        };
        let (msg, _) = run_one(|| {
            Hegel::new(body_fn)
                .settings(
                    Settings::new()
                        .database(None)
                        .seed(Some(seed(cell.idx(), i, 4)))
                        .verbosity(Verbosity::Quiet),
                )
                .reproduce_failure(blob.clone())
                .run();
        });
        seam_dump::drain();
        blob_reproduced = Some(reproduced(&msg));
    }

    Episode {
        reported,
        confirmed: blob.is_some(),
        flip_site,
        blob_nd: blob.as_deref().and_then(blob_is_nd),
        replays,
        reuse_reproduced,
        blob_reproduced,
    }
}

fn run_episode_cell(cell: Cell) {
    let episodes = episodes_per_cell();
    let mut reported = 0usize;
    let mut confirmed = 0usize;
    let mut never_flip = 0usize;
    let mut sites: Vec<(&'static str, usize)> = Vec::new();
    let mut v1 = 0usize;
    let mut v2 = 0usize;
    let mut reuse = (0usize, 0usize);
    let mut blob = (0usize, 0usize);
    let mut replay_counts: Vec<f64> = Vec::new();
    for i in 0..episodes {
        let ep = run_episode(cell, i);
        reported += ep.reported as usize;
        confirmed += ep.confirmed as usize;
        match ep.flip_site {
            Some(site) => match sites.iter_mut().find(|(l, _)| *l == site) {
                Some((_, n)) => *n += 1,
                None => sites.push((site, 1)),
            },
            None => never_flip += ep.reported as usize,
        }
        match ep.blob_nd {
            Some(true) => v2 += 1,
            Some(false) => v1 += 1,
            None => {}
        }
        if let Some(hit) = ep.reuse_reproduced {
            reuse.1 += 1;
            reuse.0 += hit as usize;
        }
        if let Some(hit) = ep.blob_reproduced {
            blob.1 += 1;
            blob.0 += hit as usize;
        }
        replay_counts.push(ep.replays.unwrap_or(0) as f64);
    }
    let site_list: Vec<String> = sites.iter().map(|(l, n)| format!("{l} {n}")).collect();
    replay_counts.sort_by(f64::total_cmp);
    let caveat_only = reported - confirmed;
    println!(
        "| {} | {reported}/{episodes} | {confirmed} | {caveat_only} | {} | {never_flip} | {v1}/{v2} | {}/{} | {}/{} | {:.0} ({:.0}) |",
        cell.name(),
        if site_list.is_empty() {
            "—".to_string()
        } else {
            site_list.join(", ")
        },
        reuse.0,
        reuse.1,
        blob.0,
        blob.1,
        percentile(&replay_counts, 0.5),
        percentile(&replay_counts, 1.0),
    );
}

fn episode_cells() -> Vec<Cell> {
    let mut cells: Vec<Cell> = (0..CLONE_PS.len()).map(Cell::CloneStream).collect();
    cells.extend((0..MACHINE_PS.len()).map(Cell::Machine));
    cells.push(Cell::Control);
    cells.push(Cell::Pass);
    cells
}

fn episode_cell_from_cli(name: &str) -> Cell {
    match name {
        "clone0.05" => Cell::CloneStream(0),
        "clone0.1" => Cell::CloneStream(1),
        "clone0.3" => Cell::CloneStream(2),
        "clone0.9" => Cell::CloneStream(3),
        "machine0.1" => Cell::Machine(0),
        "machine0.3" => Cell::Machine(1),
        "machine0.9" => Cell::Machine(2),
        "control" => Cell::Control,
        "pass" => Cell::Pass,
        other => panic!("unknown episode cell {other}"),
    }
}

fn episodes(cells: &[Cell]) {
    seam_dump::arm();
    println!(
        "# paper-eval episode suite ({} episodes per cell, sequential)\n",
        episodes_per_cell()
    );
    println!(
        "| cell | reported | confirmed | caveat-only | flips by first site | never-flip (reported) | blobs v1/v2 | reuse | blob replay | meas. replays p50 (max) |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for &cell in cells {
        run_episode_cell(cell);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("landscapes") => {
            let strict = match args.get(2).map(String::as_str) {
                Some("quiet") | None => false,
                Some("error") => true,
                Some(other) => panic!("unknown strictness {other}"),
            };
            match args.get(3) {
                Some(cell) => landscapes(strict, &[Landscape::from_cli(cell)]),
                None => landscapes(strict, &ALL_LANDSCAPES),
            }
        }
        Some("episodes") => match args.get(2) {
            Some(cell) => episodes(&[episode_cell_from_cli(cell)]),
            None => episodes(&episode_cells()),
        },
        _ => {
            eprintln!(
                "usage: paper-eval landscapes [quiet|error] [l1|l3|l4|l4b|d2|d0|n0] | episodes [<cell>]"
            );
            std::process::exit(2);
        }
    }
}

//! Experiment 008 phase-12 spot check: the 003 cells (plus L4b and D2)
//! re-run on the fixed engine through the public C ABI, under the shipped
//! nondeterministic-handling rules. Spec and results:
//! notes/experiments/008-gauntlet-calibration/notes.md

use std::cell::RefCell;
use std::ffi::{c_void, CStr, CString};
use std::os::raw::c_char;
use std::ptr;

use hegel_c::{
    hegel_result_t, hegel_run_status_t, hegel_status_t, HegelContext, HegelFailure, HegelRun,
    HegelRunResult, HegelSettings, HegelTestCase,
};

const SEEDS: u64 = 100;
const TEST_CASES: u64 = 500;
const BUG_ATOM: i64 = 10;
const MAX_ATOMS: i64 = 20;
const CORE_ATOM: i64 = 95;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(1))
    }

    fn f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Landscape {
    Rising,
    Constant,
    NoiseFloor,
    NoiseFloorLo,
    DetCore,
}

const ALL: [Landscape; 5] = [
    Landscape::Rising,
    Landscape::Constant,
    Landscape::NoiseFloor,
    Landscape::NoiseFloorLo,
    Landscape::DetCore,
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
        }
    }

    fn name(self) -> &'static str {
        match self {
            Landscape::Rising => "L1 rising",
            Landscape::Constant => "L3 constant",
            Landscape::NoiseFloor => "L4 noise-floor",
            Landscape::NoiseFloorLo => "L4b noise-floor-lo",
            Landscape::DetCore => "D2 det-core 0.7",
        }
    }

    fn from_cli(name: &str) -> Landscape {
        match name {
            "l1" => Landscape::Rising,
            "l3" => Landscape::Constant,
            "l4" => Landscape::NoiseFloor,
            "l4b" => Landscape::NoiseFloorLo,
            "d2" => Landscape::DetCore,
            other => panic!("unknown cell {other} (expected l1, l3, l4, l4b, or d2)"),
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

unsafe extern "C" fn print_output(_: *mut c_void, line: *const c_char, _: usize) {
    eprintln!("{}", CStr::from_ptr(line).to_string_lossy());
}

fn debug() -> bool {
    std::env::var("ND_DEBUG").is_ok()
}

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
    let origin_ptr = if status.1 { origin.as_ptr() } else { ptr::null() };
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

enum Outcome {
    Aborted(String),
    NoBug,
    CaveatOnly { execs: u64 },
    Shrunk { atoms: Vec<i64>, execs: u64, nd: bool },
}

fn run_trial(landscape: Landscape, seed: u64) -> Outcome {
    let ctx = Ctx::new();
    let hidden = RefCell::new(Rng::new(seed.wrapping_mul(0xC0FFEE) ^ 0xD15EA5E));
    let origin = CString::new("bug").unwrap();
    unsafe {
        let mut settings: *mut HegelSettings = ptr::null_mut();
        ctx.ok(hegel_c::hegel_settings_new(ctx.0, &mut settings));
        ctx.ok(hegel_c::hegel_settings_set_test_cases(
            ctx.0, settings, TEST_CASES,
        ));
        ctx.ok(hegel_c::hegel_settings_set_verbosity(
            ctx.0,
            settings,
            if debug() { 3 } else { 0 },
        ));
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
            Some(if debug() { print_output } else { discard_output }),
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
                Outcome::Aborted(CStr::from_ptr(msg).to_string_lossy().into_owned())
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
                    ctx.0, result, 0, &mut failure,
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
                        None => Outcome::Aborted("final blob replay overran".to_string()),
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

fn run_cell(landscape: Landscape) -> Vec<Outcome> {
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
                        .map(|seed| (seed, run_trial(landscape, seed)))
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

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = (q * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn print_row(landscape: Landscape, outcomes: &[Outcome]) {
    let aborted = outcomes
        .iter()
        .filter(|o| matches!(o, Outcome::Aborted(_)))
        .count();
    let nobug = outcomes.iter().filter(|o| matches!(o, Outcome::NoBug)).count();
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
    let mut ex: Vec<f64> = shrunk.iter().map(|(_, e, _)| *e as f64).collect();
    ps.sort_by(f64::total_cmp);
    lens.sort_by(f64::total_cmp);
    ex.sort_by(f64::total_cmp);
    let det = if landscape == Landscape::DetCore {
        let det_finals = shrunk
            .iter()
            .filter(|(a, _, _)| Landscape::has_core(a))
            .count();
        format!("{det_finals}/{n}")
    } else {
        "—".to_string()
    };
    println!(
        "| {} | {} | {} | {} | {} | {}/{} | {:.0} | {:.2} / {:.2} / {:.2} | {:.0} ({:.0}) | {} | {} |",
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
        det,
    );
}

fn spot(cells: &[Landscape]) {
    println!("# gauntlet-calibration in-engine spot check ({SEEDS} seeds per cell, {TEST_CASES} test-case budget)\n");
    println!(
        "| cell | shrunk | aborted | no-bug | caveat-only | bug kept | len med | final p p10/p50/p90 | execs med (p90) | nd | det finals |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for &landscape in cells {
        let outcomes = run_cell(landscape);
        print_row(landscape, &outcomes);
    }
}

fn one(landscape: Landscape, seed: u64) {
    match run_trial(landscape, seed) {
        Outcome::Aborted(msg) => println!("aborted: {msg}"),
        Outcome::NoBug => println!("no bug found"),
        Outcome::CaveatOnly { execs } => println!("caveat-only report in {execs} execs"),
        Outcome::Shrunk { atoms, execs, nd } => println!(
            "shrunk to {atoms:?} (p={}, nd={nd}) in {execs} execs",
            landscape.p(&atoms)
        ),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("spot") => match args.get(2) {
            Some(cell) => spot(&[Landscape::from_cli(cell)]),
            None => spot(&ALL),
        },
        Some("one") => one(Landscape::from_cli(&args[2]), args[3].parse().unwrap()),
        _ => {
            eprintln!("usage: gauntlet-calibration spot [l1|l3|l4|l4b|d2] | one <cell> <seed>");
            std::process::exit(2);
        }
    }
}

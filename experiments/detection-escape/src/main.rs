//! Experiment 012: detection-escape recheck on the phase-16 engine.
//! Spec and results: notes/experiments/012-detection-escape/notes.md
//!
//! Clones experiment 009a's episode protocol (`experiments/watermark`) —
//! {clone, machine} bodies x p in {0.1, 0.3, 0.9} plus a deterministic
//! control, 200 episodes each — and collects 011's seam-event columns
//! instead of watermark samples: flip share and first flip site per
//! episode, blob kind, reuse and blob reproduction, and the statistics
//! line's measurement-replay count for the control's cost identity.

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use hegel::__bench::{blob_is_nd, seam_dump};
use hegel::generators as gs;
use hegel::{Hegel, Phase, Settings, TestCase, Verbosity};

const EPISODES: usize = 200;
const PS: [f64; 3] = [0.1, 0.3, 0.9];
const MARKER: &str = "detection-escape: lost update";

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
}

/// Seed scheme, fixed in code so outputs are byte-identical across reruns:
/// episode i salts 1 (hidden schedule), 2 (discovery engine), 3 (reuse
/// engine), 4 (blob engine) — the 009a convention.
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
}

impl Cell {
    fn name(self) -> String {
        match self {
            Cell::CloneStream(pi) => format!("clone p={}", PS[pi]),
            Cell::Machine(pi) => format!("machine p={}", PS[pi]),
            Cell::Control => "det-control".to_string(),
        }
    }

    fn idx(self) -> usize {
        match self {
            Cell::CloneStream(pi) => pi,
            Cell::Machine(pi) => 3 + pi,
            Cell::Control => 6,
        }
    }

    fn run(self, tc: &TestCase, hidden: &mut Rng) {
        match self {
            Cell::CloneStream(pi) => clone_stream_body(tc, hidden, PS[pi]),
            Cell::Machine(pi) => machine_body(tc, hidden, PS[pi]),
            Cell::Control => control_body(tc),
        }
    }
}

/// One clone stream of scalar work draws; the hidden schedule injects retry
/// draws (a disjoint value range, so a shifted replay cannot pun) at
/// CLONE_RACE per round, and the failure fires at rate `p` independent of
/// the drawn values, so every timeline's true reproduction rate is `p`.
/// Verbatim from experiment 009a.
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
/// verbatim from experiment 009a.
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
        seam_dump::FlipSite::Concurrency => "conc",
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
            .__database_key("escape".to_string())
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
                .__database_key("escape".to_string())
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
        flip_site,
        blob_nd: blob.as_deref().and_then(blob_is_nd),
        replays,
        reuse_reproduced,
        blob_reproduced,
    }
}

fn run_cell(cell: Cell) {
    let mut reported = 0usize;
    let mut never_flip = 0usize;
    let mut sites: Vec<(&'static str, usize)> = Vec::new();
    let mut v1 = 0usize;
    let mut v2 = 0usize;
    let mut reuse = (0usize, 0usize);
    let mut blob = (0usize, 0usize);
    let mut replay_counts: Vec<u64> = Vec::new();
    for i in 0..EPISODES {
        let ep = run_episode(cell, i);
        reported += ep.reported as usize;
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
        if let Some(n) = ep.replays {
            replay_counts.push(n);
        }
    }
    let site_list: Vec<String> = sites.iter().map(|(l, n)| format!("{l} {n}")).collect();
    let replays = if replay_counts.is_empty() {
        "—".to_string()
    } else {
        let min = replay_counts.iter().min().unwrap();
        let max = replay_counts.iter().max().unwrap();
        format!("{min}..{max} ({})", replay_counts.len())
    };
    println!(
        "| {} | {reported}/{EPISODES} | {} | {never_flip} | {v1}/{v2} | {}/{} | {}/{} | {replays} |",
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
    );
}

fn main() {
    seam_dump::arm();
    println!(
        "# detection-escape recheck, experiment 012 ({EPISODES} episodes per cell, sequential)\n"
    );
    println!(
        "| cell | reported | flips by first site | never-flip (reported) | blobs v1/v2 | reuse | blob replay | measurement replays min..max (n) |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    for pi in 0..PS.len() {
        run_cell(Cell::CloneStream(pi));
    }
    for pi in 0..PS.len() {
        run_cell(Cell::Machine(pi));
    }
    run_cell(Cell::Control);
}

//! Live-set experiment harness: one process per run, driven by `drive.py`.
//!
//! Subcommands:
//! - `discover-<body> <dbdir> <seed>`: Generate+Shrink into a fresh database.
//! - `reuse-<body> <dbdir> <seed>`: Reuse+Shrink from an existing database.
//! - `replay-<body> <blob>`: replay a reproduce blob with the database disabled.
//! - `blobinfo <blob>`: decode a blob and print its timeline count and lengths.
//!
//! Every run prints `EXECUTIONS: <n>` (body invocations) and
//! `RESULT: FAILED|PASSED`.

use hegel::generators as gs;
use hegel::stateful::run_concurrent;
use hegel::{Hegel, NondeterminismStrictness, Phase, Settings, TestCase};
use hegel_c::__bench::{blob_is_nd, blob_timelines, ChoiceValue};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static EXECUTIONS: AtomicUsize = AtomicUsize::new(0);
static HIDDEN: Mutex<u64> = Mutex::new(0x9E3779B97F4A7C15);

fn seed_hidden(seed: u64) {
    let mut s = HIDDEN.lock().unwrap();
    *s = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
}

/// A coin flip the engine never sees: process-global xorshift64, P(true) = 0.5.
fn hidden_coin() -> bool {
    let mut s = HIDDEN.lock().unwrap();
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *s = x;
    (x >> 63) == 1
}

fn count_execution() {
    EXECUTIONS.fetch_add(1, Ordering::SeqCst);
}

fn small_int() -> impl gs::PrintableGenerator<i64> {
    gs::integers::<i64>().min_value(0).max_value(100)
}

struct RacyCounter {
    value: AtomicI64,
    increments: AtomicI64,
}

#[hegel::concurrent_state_machine]
impl RacyCounter {
    #[rule]
    fn racy_increment(&self, _: TestCase) {
        let value = self.value.load(Ordering::SeqCst);
        std::thread::yield_now();
        self.value.store(value + 1, Ordering::SeqCst);
        self.increments.fetch_add(1, Ordering::SeqCst);
    }

    #[invariant]
    fn no_lost_updates(&self, _: TestCase) {
        assert_eq!(
            self.value.load(Ordering::SeqCst),
            self.increments.load(Ordering::SeqCst)
        );
    }
}

fn racy_body(tc: TestCase) {
    count_execution();
    let m = RacyCounter {
        value: AtomicI64::new(0),
        increments: AtomicI64::new(0),
    };
    run_concurrent(m, tc, 2, 4);
}

static CLONE_CALLS: AtomicI64 = AtomicI64::new(0);

fn clone_flaky_body(tc: TestCase) {
    count_execution();
    let child = tc.clone();
    let x: i64 = child.draw(gs::integers::<i64>().min_value(0).max_value(1000));
    let call = CLONE_CALLS.fetch_add(1, Ordering::SeqCst);
    if call % 3 == 0 {
        assert!(x < 500, "clone-flaky: x = {x}");
    }
}

fn branch_body(tc: TestCase) {
    count_execution();
    let a = tc.draw(gs::booleans());
    let fail = if hidden_coin() {
        let b = tc.draw(gs::booleans());
        let x = tc.draw(small_int());
        a && b && x >= 60
    } else {
        let y = tc.draw(small_int());
        let z = tc.draw(small_int());
        y >= 60 && z >= 60
    };
    assert!(!fail, "branch bug");
}

fn hot_piece(tc: &TestCase) -> bool {
    if hidden_coin() {
        tc.draw(gs::booleans())
    } else {
        tc.draw(small_int()) >= 60
    }
}

fn twobranch_body(tc: TestCase) {
    count_execution();
    let a = tc.draw(gs::booleans());
    let first = hot_piece(&tc);
    let second = hot_piece(&tc);
    let fail = a && first && second;
    assert!(!fail, "branch bug");
}

fn body_for(name: &str) -> fn(TestCase) {
    match name {
        "racy" => racy_body,
        "clone" => clone_flaky_body,
        "branch" => branch_body,
        "twobranch" => twobranch_body,
        other => panic!("unknown body {other}"),
    }
}

fn flat_len(timeline: &[ChoiceValue]) -> usize {
    timeline
        .iter()
        .map(|v| match v {
            ChoiceValue::Clone(r) => 1 + r.flat_len(),
            _ => 1,
        })
        .sum()
}

fn blobinfo(blob: &str) {
    match blob_timelines(blob) {
        None => println!("TIMELINES: undecodable"),
        Some(timelines) => {
            println!("ND: {}", blob_is_nd(blob).unwrap_or(false));
            println!("TIMELINES: {}", timelines.len());
            let lengths: Vec<String> = timelines
                .iter()
                .map(|t| flat_len(t).to_string())
                .collect();
            println!("LENGTHS: {}", lengths.join(","));
        }
    }
}

fn wallclock_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    nanos ^ ((std::process::id() as u64) << 32)
}

fn db_settings(dbdir: String, seed: u64, phases: [Phase; 2]) -> Settings {
    Settings::new()
        .database(Some(dbdir))
        .test_cases(200)
        .print_blob(true)
        .seed(Some(seed))
        .phases(phases)
        .nondeterminism_strictness(NondeterminismStrictness::Quiet)
        .verbosity(if std::env::var_os("LIVESET_DEBUG").is_some() {
            hegel::Verbosity::Debug
        } else {
            hegel::Verbosity::Normal
        })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args[1].clone();
    if mode == "blobinfo" {
        blobinfo(&args[2]);
        return;
    }
    let (verb, body_name) = mode.split_once('-').expect("mode is <verb>-<body>");
    let verb = verb.to_string();
    let body = body_for(body_name);
    let arg = args[2].clone();
    let seed: Option<u64> = args.get(3).map(|s| s.parse().expect("seed is a u64"));
    let salt = match verb.as_str() {
        "discover" => 1,
        "reuse" => 2,
        _ => 3,
    };
    seed_hidden(
        seed.map(|s| s.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(salt))
            .unwrap_or_else(wallclock_seed),
    );
    let start = Instant::now();
    let key = format!("livesets-{body_name}");
    let outcome = std::panic::catch_unwind(move || {
        let h = Hegel::new(body).__database_key(key);
        match verb.as_str() {
            "discover" => h
                .settings(db_settings(
                    arg,
                    seed.unwrap(),
                    [Phase::Generate, Phase::Shrink],
                ))
                .run(),
            "reuse" => h
                .settings(db_settings(
                    arg,
                    seed.unwrap(),
                    [Phase::Reuse, Phase::Shrink],
                ))
                .run(),
            "replay" => h
                .settings(Settings::new().database(None))
                .reproduce_failure(arg)
                .run(),
            other => panic!("unknown verb {other}"),
        }
    });
    let elapsed = start.elapsed().as_secs_f64();
    println!("SECONDS: {elapsed:.3}");
    println!("EXECUTIONS: {}", EXECUTIONS.load(Ordering::SeqCst));
    match outcome {
        Ok(()) => println!("RESULT: PASSED"),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            println!("PANIC: {}", msg.replace('\n', " / "));
            println!("RESULT: FAILED");
        }
    }
}

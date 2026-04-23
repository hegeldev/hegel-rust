/*!
Stress tester for hegel-core server race conditions.

Runs many concurrent Hegel tests to trigger race conditions in the server's
stream allocation and shutdown handling. Originally based on a race reproducer
that demonstrated the non-atomic `new_stream` bug in hegel-core <= 0.4.1.

Usage:
    stress_test_2 --hegel-core /path/to/hegel-core [OPTIONS]
    stress_test_2 [OPTIONS]

When --hegel-core is given, a virtualenv is created (or reused) with an editable
install of that directory, and a .pth file is injected to reduce the GIL switch
interval to 100ns — maximizing the probability of hitting the race condition.
*/

use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use hegel::generators as gs;
use hegel::{standalone_function, HealthCheck, Hegel, Settings, TestCase, Verbosity};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn settings() -> Settings {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    Settings::new()
        .test_cases(10)
        .seed(Some(id))
        .database(None)
        .verbosity(Verbosity::Quiet)
        .suppress_health_check(HealthCheck::all())
}

// ─── Flaky tests (trigger FlakyStrategyDefinition + shrinking races) ───────

#[standalone_function(test_cases = 10)]
fn flaky_integer_test(tc: TestCase, test_id: usize, flaky_mode: bool) {
    let x: i64 = tc.draw(gs::integers());

    if flaky_mode {
        let time_based = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 3)
            == 0;
        tc.assume(time_based || x.abs() < 1000);
    } else {
        tc.assume(x.abs() < 1000);
    }

    if x > 500 {
        panic!("Found large value: {} in test {}", x, test_id);
    }
}

#[standalone_function(test_cases = 10)]
fn flaky_collection_test(tc: TestCase, test_id: usize, flaky_mode: bool) {
    let numbers: Vec<i32> = tc.draw(gs::vecs(gs::integers::<i32>()));

    if flaky_mode {
        let filter_threshold = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros()
            % 100) as i32;
        tc.assume(numbers.len() < 20);
        tc.assume(numbers.iter().all(|&n| n.abs() < filter_threshold + 50));
    } else {
        tc.assume(numbers.len() < 20);
    }

    let sum: i64 = numbers.iter().map(|&x| x as i64).sum();

    if sum > 1000 {
        panic!(
            "Large sum {} in test {} with {} elements",
            sum,
            test_id,
            numbers.len()
        );
    }
}

#[standalone_function(test_cases = 10)]
fn flaky_tuple_test(tc: TestCase, test_id: usize, flaky_mode: bool) {
    let text: String = tc.draw(gs::text());
    let number: i32 = tc.draw(gs::integers::<i32>().min_value(-100).max_value(100));
    let pair = (text, number);

    if flaky_mode {
        let dynamic_limit = ((std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            / 1000)
            % 8) as usize
            + 2;
        tc.assume(pair.0.len() <= dynamic_limit);
    } else {
        tc.assume(pair.0.len() <= 10);
    }

    if pair.0.len() > 5 && pair.1.abs() > 50 {
        panic!(
            "Found large pair: ({:?}, {}) in test {}",
            pair.0, pair.1, test_id
        );
    }
}

#[standalone_function(test_cases = 10)]
fn flaky_boolean_test(tc: TestCase, test_id: usize, flaky_mode: bool) {
    let flags: Vec<bool> = tc.draw(gs::vecs(gs::booleans()));

    if flaky_mode {
        let time_mod = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 4)
            == 0;
        tc.assume(flags.len() >= 5 && flags.len() <= 15);
        tc.assume(time_mod || flags.iter().filter(|&&b| b).count() < 8);
    } else {
        tc.assume(flags.len() >= 5 && flags.len() <= 15);
    }

    let true_count = flags.iter().filter(|&&b| b).count();
    if true_count > 10 {
        panic!("Too many trues: {} in test {}", true_count, test_id);
    }
}

#[standalone_function(test_cases = 10)]
fn flaky_simple_test(tc: TestCase, test_id: usize, flaky_mode: bool) {
    let x: i32 = tc.draw(gs::integers());
    let y: String = tc.draw(gs::text());

    if flaky_mode {
        let time_check = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            % 3
            == 0;
        tc.assume(time_check || x.abs() < 100);
        tc.assume(y.len() < 10);
    } else {
        tc.assume(x.abs() < 100);
        tc.assume(y.len() < 10);
    }

    if x > 50 && y.len() > 3 {
        panic!("Found big combo: {} / {:?} in test {}", x, y, test_id);
    }
}

// ─── Good tests (correct, just add server load) ───────────────────────────

fn test_integers_basic() {
    Hegel::new(|tc| {
        let n: i32 = tc.draw(gs::integers());
        let _ = n.checked_add(0).unwrap();
    })
    .settings(settings())
    .run();
}

fn test_bounded_integers() {
    Hegel::new(|tc| {
        let lo: i32 = tc.draw(gs::integers().min_value(-1000).max_value(0));
        let hi: i32 = tc.draw(gs::integers().min_value(lo).max_value(1000));
        assert!(lo <= hi);
    })
    .settings(settings())
    .run();
}

fn test_vec_properties() {
    Hegel::new(|tc| {
        let v: Vec<i32> = tc.draw(gs::vecs(gs::integers::<i32>()).max_size(20));
        let mut sorted = v.clone();
        sorted.sort();
        assert_eq!(sorted.len(), v.len());
    })
    .settings(settings())
    .run();
}

fn test_text_generation() {
    Hegel::new(|tc| {
        let s: String = tc.draw(gs::text().max_size(100));
        assert!(s.len() <= 400);
    })
    .settings(settings())
    .run();
}

fn test_booleans() {
    Hegel::new(|tc| {
        let b: bool = tc.draw(gs::booleans());
        assert!(b || !b);
    })
    .settings(settings())
    .run();
}

fn test_floats() {
    Hegel::new(|tc| {
        let f: f64 = tc.draw(gs::floats());
        assert!(f.is_finite() || f.is_nan() || f.is_infinite());
    })
    .settings(settings())
    .run();
}

fn test_tuples() {
    Hegel::new(|tc| {
        let (a, b): (i32, bool) = tc.draw(gs::tuples!(gs::integers::<i32>(), gs::booleans()));
        let _ = (a, b);
    })
    .settings(settings())
    .run();
}

fn test_nested_vecs() {
    Hegel::new(|tc| {
        let v: Vec<Vec<bool>> =
            tc.draw(gs::vecs(gs::vecs(gs::booleans()).max_size(5)).max_size(5));
        let total: usize = v.iter().map(Vec::len).sum();
        assert!(total <= 25);
    })
    .settings(settings())
    .run();
}

fn test_one_of() {
    Hegel::new(|tc| {
        let n: i32 = tc.draw(hegel::one_of!(
            gs::integers::<i32>().min_value(0).max_value(10),
            gs::integers::<i32>().min_value(100).max_value(110)
        ));
        assert!((0..=10).contains(&n) || (100..=110).contains(&n));
    })
    .settings(settings())
    .run();
}

fn test_filter() {
    Hegel::new(|tc| {
        let n: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(100));
        tc.assume(n % 2 == 0);
        assert!(n % 2 == 0);
    })
    .settings(settings())
    .run();
}

// ─── Test dispatch ─────────────────────────────────────────────────────────

type TestFn = fn();
type FlakyTestFn = fn(usize, bool);

const GOOD_TESTS: &[(&str, TestFn)] = &[
    ("integers_basic", test_integers_basic),
    ("bounded_integers", test_bounded_integers),
    ("vec_properties", test_vec_properties),
    ("text_generation", test_text_generation),
    ("booleans", test_booleans),
    ("floats", test_floats),
    ("tuples", test_tuples),
    ("nested_vecs", test_nested_vecs),
    ("one_of", test_one_of),
    ("filter", test_filter),
];

const FLAKY_TESTS: &[(&str, FlakyTestFn)] = &[
    ("flaky_integer", flaky_integer_test),
    ("flaky_collection", flaky_collection_test),
    ("flaky_tuple", flaky_tuple_test),
    ("flaky_boolean", flaky_boolean_test),
    ("flaky_simple", flaky_simple_test),
];

// ─── CLI argument parsing ──────────────────────────────────────────────────

struct Config {
    workers: usize,
    tests_per_worker: usize,
    timeout_secs: u64,
    flaky_mode: bool,
    verbose: bool,
    hegel_core_dir: Option<PathBuf>,
    racy_gil: bool,
}

fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();
    let mut config = Config {
        workers: 16,
        tests_per_worker: 20,
        timeout_secs: 300,
        flaky_mode: true,
        verbose: false,
        hegel_core_dir: None,
        racy_gil: true,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--workers" => {
                i += 1;
                config.workers = args[i].parse().expect("invalid --workers");
            }
            "--tests" => {
                i += 1;
                config.tests_per_worker = args[i].parse().expect("invalid --tests");
            }
            "--timeout" => {
                i += 1;
                config.timeout_secs = args[i].parse().expect("invalid --timeout");
            }
            "--flaky" => config.flaky_mode = true,
            "--no-flaky" => config.flaky_mode = false,
            "--verbose" => config.verbose = true,
            "--hegel-core" => {
                i += 1;
                config.hegel_core_dir = Some(PathBuf::from(&args[i]));
            }
            "--no-racy-gil" => config.racy_gil = false,
            "--help" | "-h" => {
                eprintln!("Usage: stress_test_2 [OPTIONS]");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  --hegel-core DIR  Path to hegel-core checkout (manages venv automatically)");
                eprintln!("  --workers N       Worker threads (default: 16)");
                eprintln!("  --tests N         Tests per worker (default: 20)");
                eprintln!("  --timeout SECS    Overall timeout (default: 300)");
                eprintln!("  --flaky           Enable flaky test mode (default)");
                eprintln!("  --no-flaky        Disable flaky test mode");
                eprintln!("  --no-racy-gil     Don't inject low GIL switch interval");
                eprintln!("  --verbose         Print per-test output");
                std::process::exit(0);
            }
            // Legacy positional args: threads tests_per_thread flaky verbose
            other if other.parse::<usize>().is_ok() && i == 1 => {
                config.workers = other.parse().unwrap();
                if let Some(v) = args.get(2) {
                    config.tests_per_worker = v.parse().unwrap_or(config.tests_per_worker);
                }
                if let Some(v) = args.get(3) {
                    config.flaky_mode = v == "true";
                }
                if let Some(v) = args.get(4) {
                    config.verbose = v == "true";
                }
                break;
            }
            other => {
                eprintln!("Unknown argument: {other}");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    config
}

// ─── Venv management ──────────────────────────────────────────────────────

fn setup_hegel_core_venv(hegel_core_dir: &Path, racy_gil: bool) -> PathBuf {
    let hegel_core_dir = hegel_core_dir.canonicalize().unwrap_or_else(|e| {
        eprintln!("Cannot resolve hegel-core path {:?}: {e}", hegel_core_dir);
        std::process::exit(1);
    });

    assert!(
        hegel_core_dir.join("pyproject.toml").exists(),
        "{:?} does not look like a hegel-core checkout (no pyproject.toml)",
        hegel_core_dir
    );

    let venv_dir = hegel_core_dir.join(".stress-test-venv");
    let python = venv_dir.join("bin/python3");
    let hegel_bin = venv_dir.join("bin/hegel");

    if !venv_dir.exists() {
        eprintln!("Creating virtualenv at {venv_dir:?}...");
        let status = Command::new("python3")
            .args(["-m", "venv", &venv_dir.to_string_lossy()])
            .status()
            .expect("failed to run python3 -m venv");
        assert!(status.success(), "venv creation failed");

        eprintln!("Installing hegel-core (editable)...");
        let status = Command::new(&python)
            .args([
                "-m",
                "pip",
                "install",
                "-e",
                &hegel_core_dir.to_string_lossy(),
            ])
            .status()
            .expect("failed to run pip install");
        assert!(status.success(), "pip install failed");
    } else {
        eprintln!("Reusing existing virtualenv at {venv_dir:?}");
    }

    if racy_gil {
        let site_packages = find_site_packages(&python);
        let pth_path = site_packages.join("racy_gil.pth");
        if !pth_path.exists() {
            eprintln!("Injecting racy GIL .pth file...");
            std::fs::write(
                &pth_path,
                "import sys; sys.setswitchinterval(0.0000001)\n",
            )
            .expect("failed to write .pth file");
        }

        let output = Command::new(&python)
            .args(["-c", "import sys; print(sys.getswitchinterval())"])
            .output()
            .expect("failed to check switch interval");
        let interval: f64 = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap_or(1.0);
        assert!(
            interval < 0.001,
            "GIL switch interval not reduced (got {interval})"
        );
        eprintln!("GIL switch interval: {interval}s");
    }

    assert!(
        hegel_bin.exists(),
        "hegel binary not found at {hegel_bin:?} — pip install may have failed"
    );

    eprintln!("Using hegel at {hegel_bin:?}");
    hegel_bin
}

fn find_site_packages(python: &Path) -> PathBuf {
    let output = Command::new(python)
        .args([
            "-c",
            "import site; print(site.getsitepackages()[0])",
        ])
        .output()
        .expect("failed to find site-packages");
    PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
}

// ─── Worker logic ──────────────────────────────────────────────────────────

fn run_worker(
    worker_id: usize,
    config: &Config,
    barrier: &Barrier,
    server_crashed: &AtomicBool,
    crash_count: &AtomicU64,
    test_count: &AtomicU64,
) {
    barrier.wait();

    let total_tests = GOOD_TESTS.len() + FLAKY_TESTS.len();

    for i in 0..config.tests_per_worker {
        if server_crashed.load(Ordering::Relaxed) {
            return;
        }

        let test_id = worker_id * config.tests_per_worker + i;
        let func_idx = test_id % total_tests;

        let result = catch_unwind(AssertUnwindSafe(|| {
            if func_idx < GOOD_TESTS.len() {
                GOOD_TESTS[func_idx].1();
            } else {
                let flaky_idx = func_idx - GOOD_TESTS.len();
                FLAKY_TESTS[flaky_idx].1(test_id, config.flaky_mode);
            }
        }));

        test_count.fetch_add(1, Ordering::Relaxed);

        if let Err(e) = result {
            let msg = if let Some(s) = e.downcast_ref::<String>() {
                s.as_str()
            } else if let Some(s) = e.downcast_ref::<&str>() {
                s
            } else {
                "<unknown>"
            };

            if msg.contains("server process exited unexpectedly") {
                crash_count.fetch_add(1, Ordering::Relaxed);
                server_crashed.store(true, Ordering::Relaxed);
                return;
            }

            if config.verbose {
                eprintln!("[Worker {worker_id}] Test {test_id} failed: {msg}");
            }
        }

        thread::yield_now();
    }
}

// ─── Log watcher ───────────────────────────────────────────────────────────

fn watch_server_log(stop: &AtomicBool) {
    let mut last_pos = 0u64;
    let mut path: Option<String> = None;

    while !stop.load(Ordering::Relaxed) {
        if path.is_none() {
            path = hegel::server_log_path();
        }
        if let Some(ref p) = path {
            if let Ok(content) = std::fs::read_to_string(p) {
                let bytes = content.as_bytes();
                if (bytes.len() as u64) > last_pos {
                    let new_content = &content[last_pos as usize..];
                    let stderr = std::io::stderr();
                    let mut lock = stderr.lock();
                    let _ = lock.write_all(new_content.as_bytes());
                    last_pos = bytes.len() as u64;
                }
            }
        }
        thread::sleep(Duration::from_millis(100));
    }

    // Final drain
    if let Some(ref p) = path {
        if let Ok(content) = std::fs::read_to_string(p) {
            if (content.len() as u64) > last_pos {
                let stderr = std::io::stderr();
                let mut lock = stderr.lock();
                let _ = lock.write_all(content[last_pos as usize..].as_bytes());
            }
        }
    }
}

// ─── Main ──────────────────────────────────────────────────────────────────

fn main() {
    let config = parse_args();

    if let Some(ref hegel_core_dir) = config.hegel_core_dir {
        let hegel_bin = setup_hegel_core_venv(hegel_core_dir, config.racy_gil);
        // SAFETY: This happens before any threads are spawned.
        unsafe { std::env::set_var("HEGEL_SERVER_COMMAND", &hegel_bin) };
    }

    // Install panic hook that filters hegel internal panics
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = if let Some(s) = info.payload().downcast_ref::<String>() {
            s.as_str()
        } else if let Some(s) = info.payload().downcast_ref::<&str>() {
            s
        } else {
            ""
        };
        if msg.contains("__HEGEL_") || msg.contains("Property test failed") {
            return;
        }
        prev_hook(info);
    }));

    eprintln!(
        "Stress tester: {} workers × {} tests{}",
        config.workers,
        config.tests_per_worker,
        if config.flaky_mode {
            " (flaky mode)"
        } else {
            ""
        }
    );
    eprintln!(
        "Timeout: {}s | {} good tests + {} flaky tests",
        config.timeout_secs,
        GOOD_TESTS.len(),
        FLAKY_TESTS.len()
    );

    let server_crashed = Arc::new(AtomicBool::new(false));
    let crash_count = Arc::new(AtomicU64::new(0));
    let test_count = Arc::new(AtomicU64::new(0));
    let barrier = Arc::new(Barrier::new(config.workers));
    let log_stop = Arc::new(AtomicBool::new(false));

    // Start log watcher
    let log_stop_clone = Arc::clone(&log_stop);
    let log_thread = thread::spawn(move || watch_server_log(&log_stop_clone));

    let start_time = Instant::now();
    let timeout = Duration::from_secs(config.timeout_secs);

    // Spawn worker threads
    let handles: Vec<_> = (0..config.workers)
        .map(|worker_id| {
            let barrier = Arc::clone(&barrier);
            let server_crashed = Arc::clone(&server_crashed);
            let crash_count = Arc::clone(&crash_count);
            let test_count = Arc::clone(&test_count);
            // Leak the config into a static ref for the thread
            let workers = config.workers;
            let tests_per_worker = config.tests_per_worker;
            let flaky_mode = config.flaky_mode;
            let verbose = config.verbose;
            let timeout_secs = config.timeout_secs;
            thread::spawn(move || {
                let thread_config = Config {
                    workers,
                    tests_per_worker,
                    timeout_secs,
                    flaky_mode,
                    verbose,
                    hegel_core_dir: None,
                    racy_gil: false,
                };
                run_worker(
                    worker_id,
                    &thread_config,
                    &barrier,
                    &server_crashed,
                    &crash_count,
                    &test_count,
                );
            })
        })
        .collect();

    // Wait for all workers or timeout
    let mut timed_out = false;
    for handle in handles {
        let remaining = timeout.saturating_sub(start_time.elapsed());
        if remaining.is_zero() {
            timed_out = true;
            break;
        }
        // Can't join with timeout in std, just join and hope workers respect crashed flag
        handle.join().ok();
    }

    let duration = start_time.elapsed();

    // Give log watcher time to drain
    thread::sleep(Duration::from_millis(500));
    log_stop.store(true, Ordering::Relaxed);
    log_thread.join().ok();

    // Summary
    let crashes = crash_count.load(Ordering::Relaxed);
    let tests = test_count.load(Ordering::Relaxed);

    eprintln!();
    eprintln!("── Summary ──────────────────────────────────────");
    eprintln!("Duration: {:.1}s", duration.as_secs_f64());
    eprintln!("Tests completed: {tests}");
    eprintln!("Server crashes: {crashes}");
    if timed_out {
        eprintln!("Status: TIMED OUT");
    } else if crashes > 0 {
        eprintln!("Status: SERVER CRASHED (race condition triggered!)");
    } else {
        eprintln!("Status: COMPLETED (no server crash detected)");
    }

    if crashes > 0 {
        std::process::exit(1);
    }
}

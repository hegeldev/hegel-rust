use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use hegel::generators::{self as gs, Generator};
use hegel::{HealthCheck, Hegel, Settings, Verbosity};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn settings() -> Settings {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    Settings::new()
        .test_cases(20)
        .seed(Some(id))
        .database(None)
        .verbosity(Verbosity::Quiet)
        .suppress_health_check(HealthCheck::all())
}

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

fn test_hashmap_generation() {
    Hegel::new(|tc| {
        let m: std::collections::HashMap<i32, String> =
            tc.draw(gs::hashmaps(gs::integers(), gs::text().max_size(10)).max_size(10));
        assert!(m.len() <= 10);
    })
    .settings(settings())
    .run();
}

fn test_filter_and_map() {
    Hegel::new(|tc| {
        let n: i32 = tc.draw(
            gs::integers::<i32>()
                .min_value(0)
                .max_value(1000)
                .filter(|&x| x % 2 == 0)
                .map(|x| x + 1),
        );
        assert!(n % 2 == 1);
    })
    .settings(settings())
    .run();
}

fn test_one_of_combinator() {
    Hegel::new(|tc| {
        let n: i32 = tc.draw(gs::one_of(vec![
            gs::integers::<i32>().min_value(0).max_value(10).boxed(),
            gs::integers::<i32>().min_value(100).max_value(110).boxed(),
        ]));
        assert!((0..=10).contains(&n) || (100..=110).contains(&n));
    })
    .settings(settings())
    .run();
}

fn test_nested_collections() {
    Hegel::new(|tc| {
        let v: Vec<Vec<bool>> = tc.draw(gs::vecs(gs::vecs(gs::booleans()).max_size(5)).max_size(5));
        for inner in &v {
            assert!(inner.len() <= 5);
        }
    })
    .settings(settings())
    .run();
}

fn test_floats() {
    Hegel::new(|tc| {
        let f: f64 = tc.draw(gs::floats().min_value(-1000.0).max_value(1000.0));
        assert!((-1000.0..=1000.0).contains(&f));
    })
    .settings(settings())
    .run();
}

fn test_optional_and_sampled() {
    Hegel::new(|tc| {
        let maybe: Option<i32> = tc.draw(gs::optional(gs::integers::<i32>()));
        if let Some(n) = maybe {
            let _ = n.checked_add(0).unwrap();
        }
        let picked: &str = tc.draw(gs::sampled_from(vec!["a", "b", "c"]));
        assert!(["a", "b", "c"].contains(&picked));
    })
    .settings(settings())
    .run();
}

// --- Deliberately flaky tests ---

fn bad_external_randomness() {
    Hegel::new(|tc| {
        let n: i32 = tc.draw(gs::integers());
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        if nanos % 137 == 0 {
            assert!(n == 0, "random flake: n={n}, nanos={nanos}");
        }
    })
    .settings(settings())
    .run();
}

fn bad_shared_state() {
    use std::sync::atomic::AtomicI64;
    static SHARED: AtomicI64 = AtomicI64::new(0);

    Hegel::new(|tc| {
        let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
        let prev = SHARED.fetch_add(n, Ordering::Relaxed);
        assert!(
            prev < 50_000,
            "shared state overflow: prev={prev}, added={n}"
        );
    })
    .settings(settings())
    .run();
}

fn bad_timing_dependent() {
    Hegel::new(|tc| {
        let n: u64 = tc.draw(gs::integers::<u64>().min_value(0).max_value(5));
        std::thread::sleep(Duration::from_millis(n));
        let elapsed_nanos = Instant::now().elapsed().as_nanos();
        assert!(elapsed_nanos < 1_000_000, "timing flake: {elapsed_nanos}ns");
    })
    .settings(settings())
    .run();
}

fn bad_concurrent_draws() {
    Hegel::new(|tc| {
        let handles: Vec<_> = [0, 1, 2, 3]
            .into_iter()
            .map(|i| {
                let tc = tc.clone();
                std::thread::spawn(move || match i {
                    0 => {
                        let _: i32 = tc.draw(gs::integers());
                    }
                    1 => {
                        let _: String = tc.draw(gs::text().max_size(10));
                    }
                    2 => {
                        let _: Vec<bool> = tc.draw(gs::vecs(gs::booleans()).max_size(5));
                    }
                    _ => {
                        let _: f64 = tc.draw(gs::floats());
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    })
    .settings(settings())
    .run();
}

fn bad_assume_heavy() {
    Hegel::new(|tc| {
        let n: i32 = tc.draw(gs::integers());
        tc.assume(n == 42);
        assert_eq!(n, 42);
    })
    .settings(settings())
    .run();
}

struct TestEntry {
    name: &'static str,
    func: fn(),
    is_flaky: bool,
}

const TESTS: &[TestEntry] = &[
    TestEntry {
        name: "integers_basic",
        func: test_integers_basic,
        is_flaky: false,
    },
    TestEntry {
        name: "bounded_integers",
        func: test_bounded_integers,
        is_flaky: false,
    },
    TestEntry {
        name: "vec_properties",
        func: test_vec_properties,
        is_flaky: false,
    },
    TestEntry {
        name: "text_generation",
        func: test_text_generation,
        is_flaky: false,
    },
    TestEntry {
        name: "hashmap_generation",
        func: test_hashmap_generation,
        is_flaky: false,
    },
    TestEntry {
        name: "filter_and_map",
        func: test_filter_and_map,
        is_flaky: false,
    },
    TestEntry {
        name: "one_of_combinator",
        func: test_one_of_combinator,
        is_flaky: false,
    },
    TestEntry {
        name: "nested_collections",
        func: test_nested_collections,
        is_flaky: false,
    },
    TestEntry {
        name: "floats",
        func: test_floats,
        is_flaky: false,
    },
    TestEntry {
        name: "optional_and_sampled",
        func: test_optional_and_sampled,
        is_flaky: false,
    },
    TestEntry {
        name: "bad_external_randomness",
        func: bad_external_randomness,
        is_flaky: true,
    },
    TestEntry {
        name: "bad_shared_state",
        func: bad_shared_state,
        is_flaky: true,
    },
    TestEntry {
        name: "bad_timing_dependent",
        func: bad_timing_dependent,
        is_flaky: true,
    },
    TestEntry {
        name: "bad_concurrent_draws",
        func: bad_concurrent_draws,
        is_flaky: true,
    },
    TestEntry {
        name: "bad_assume_heavy",
        func: bad_assume_heavy,
        is_flaky: true,
    },
];

struct Logger {
    writer: Mutex<Box<dyn Write + Send>>,
}

impl Logger {
    fn new() -> Self {
        Logger {
            writer: Mutex::new(Box::new(std::io::stderr())),
        }
    }

    fn log(&self, msg: &str) {
        let mut w = self.writer.lock().unwrap();
        let _ = writeln!(w, "[stress] {msg}");
    }
}

fn main() {
    std::panic::set_hook(Box::new(|_| {}));

    let args: Vec<String> = std::env::args().collect();

    let mut timeout_secs: u64 = 300;
    let mut workers: usize = 4;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--timeout" => {
                i += 1;
                timeout_secs = args[i]
                    .parse()
                    .expect("--timeout requires a number (seconds)");
            }
            "--workers" => {
                i += 1;
                workers = args[i].parse().expect("--workers requires a number");
            }
            "--help" | "-h" => {
                eprintln!("Usage: stress_test [--timeout SECS] [--workers N]");
                eprintln!("  --timeout  Total runtime in seconds (default: 300)");
                eprintln!("  --workers  Number of concurrent workers (default: 4)");
                std::process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {other}");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    let logger = Arc::new(Logger::new());
    let stop = Arc::new(AtomicBool::new(false));
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let task_count = Arc::new(AtomicU64::new(0));
    let pass_count = Arc::new(AtomicU64::new(0));
    let expected_fail_count = Arc::new(AtomicU64::new(0));
    let unexpected_fail_count = Arc::new(AtomicU64::new(0));
    let hang_count = Arc::new(AtomicU64::new(0));
    let server_crashed = Arc::new(AtomicBool::new(false));

    logger.log(&format!(
        "Starting stress test: {workers} workers, {timeout_secs}s timeout, {} tests ({} flaky)",
        TESTS.len(),
        TESTS.iter().filter(|t| t.is_flaky).count()
    ));

    // Warm up the server with a single test before going concurrent
    logger.log("Warming up server...");
    let warmup_result = catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(|tc| {
            let _: bool = tc.draw(gs::booleans());
        })
        .settings(Settings::new().test_cases(1).database(None))
        .run();
    }));
    if warmup_result.is_err() {
        logger.log("FATAL: Server failed during warmup");
        std::process::exit(1);
    }
    logger.log("Server ready");

    let (tx, rx) = std::sync::mpsc::channel::<(usize, Instant)>();

    // Hang monitor thread
    let hang_logger = Arc::clone(&logger);
    let hang_stop = Arc::clone(&stop);
    let hang_count_clone = Arc::clone(&hang_count);
    let pending: Arc<Mutex<Vec<(usize, Instant, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let pending_for_monitor = Arc::clone(&pending);

    let monitor_handle = std::thread::spawn(move || {
        let hang_timeout = Duration::from_secs(60);
        loop {
            if hang_stop.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
            let guard = pending_for_monitor.lock().unwrap();
            for (task_id, started, test_name) in guard.iter() {
                if started.elapsed() > hang_timeout {
                    hang_logger.log(&format!(
                        "HANG: task {task_id} ({test_name}) has been running for {:.0}s",
                        started.elapsed().as_secs_f64()
                    ));
                    hang_count_clone.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    });

    // Completion collector thread
    let pending_for_collector = Arc::clone(&pending);
    let collector_handle = std::thread::spawn(move || {
        for (task_id, _started) in rx {
            let mut guard = pending_for_collector.lock().unwrap();
            guard.retain(|(id, _, _)| *id != task_id);
        }
    });

    // Worker threads
    let mut handles = Vec::new();
    for worker_id in 0..workers {
        let logger = Arc::clone(&logger);
        let stop = Arc::clone(&stop);
        let task_count = Arc::clone(&task_count);
        let pass_count = Arc::clone(&pass_count);
        let expected_fail_count = Arc::clone(&expected_fail_count);
        let unexpected_fail_count = Arc::clone(&unexpected_fail_count);
        let server_crashed = Arc::clone(&server_crashed);
        let pending = Arc::clone(&pending);
        let tx = tx.clone();

        handles.push(
            std::thread::Builder::new()
                .name(format!("worker-{worker_id}"))
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let task_id = task_count.fetch_add(1, Ordering::Relaxed) as usize;
                        let test = &TESTS[task_id % TESTS.len()];
                        let task_start = Instant::now();

                        {
                            let mut guard = pending.lock().unwrap();
                            guard.push((task_id, task_start, test.name.to_string()));
                        }

                        let result = catch_unwind(AssertUnwindSafe(test.func));
                        let elapsed = task_start.elapsed();

                        let _ = tx.send((task_id, task_start));

                        match result {
                            Ok(()) => {
                                pass_count.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(e) => {
                                let msg = if let Some(s) = e.downcast_ref::<String>() {
                                    s.clone()
                                } else if let Some(s) = e.downcast_ref::<&str>() {
                                    s.to_string()
                                } else {
                                    "Unknown panic".to_string()
                                };

                                let is_server_crash = msg
                                    .contains("server process exited unexpectedly")
                                    || msg.contains("server failed during startup");

                                if is_server_crash {
                                    logger.log(&format!(
                                        "SERVER CRASH detected during {}: {msg}",
                                        test.name
                                    ));
                                    server_crashed.store(true, Ordering::Relaxed);
                                    stop.store(true, Ordering::Relaxed);
                                    break;
                                }

                                let is_expected = test.is_flaky
                                    || msg.contains("Property test failed")
                                    || msg.contains("Flaky test detected")
                                    || msg.contains("Health check failure");

                                if is_expected {
                                    expected_fail_count.fetch_add(1, Ordering::Relaxed);
                                } else {
                                    logger.log(&format!(
                                        "UNEXPECTED PANIC in {} (task {task_id}, {:.2}s): {msg}",
                                        test.name,
                                        elapsed.as_secs_f64()
                                    ));
                                    unexpected_fail_count.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                })
                .unwrap(),
        );
    }
    drop(tx);

    // Main thread: wait for timeout or server crash
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if start.elapsed() >= timeout {
            logger.log("Timeout reached, stopping...");
            stop.store(true, Ordering::Relaxed);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    for h in handles {
        h.join().ok();
    }
    stop.store(true, Ordering::Relaxed);
    monitor_handle.join().ok();
    collector_handle.join().ok();

    // Print server log
    let log_dir = ".hegel";
    if let Ok(entries) = std::fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "log") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if !content.trim().is_empty() {
                        logger.log(&format!("=== Server log: {} ===", path.display()));
                        for line in content.lines() {
                            logger.log(&format!("  {line}"));
                        }
                    }
                }
            }
        }
    }

    let total = task_count.load(Ordering::Relaxed);
    let passed = pass_count.load(Ordering::Relaxed);
    let expected = expected_fail_count.load(Ordering::Relaxed);
    let unexpected = unexpected_fail_count.load(Ordering::Relaxed);
    let hangs = hang_count.load(Ordering::Relaxed);
    let elapsed = start.elapsed();

    logger.log("=== Summary ===");
    logger.log(&format!("Duration: {:.1}s", elapsed.as_secs_f64()));
    logger.log(&format!("Total tasks: {total}"));
    logger.log(&format!("Passed: {passed}"));
    logger.log(&format!("Expected failures: {expected}"));
    logger.log(&format!("Unexpected failures: {unexpected}"));
    logger.log(&format!("Hangs detected: {hangs}"));
    if server_crashed.load(Ordering::Relaxed) {
        logger.log("Server crashed: YES");
    }

    if unexpected > 0 || server_crashed.load(Ordering::Relaxed) {
        std::process::exit(1);
    }
}

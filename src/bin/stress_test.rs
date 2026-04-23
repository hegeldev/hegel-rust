use std::collections::VecDeque;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use rand::RngExt;

use hegel::generators::{self as gs, Generator};
use hegel::{HealthCheck, Hegel, Settings, Verbosity};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn settings() -> Settings {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    Settings::new()
        .test_cases(200)
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

// --- Tests that always find a failure (triggers shrinking) ---

fn shrink_integers() {
    Hegel::new(|tc| {
        let n: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(10000));
        assert!(n < 100);
    })
    .settings(settings())
    .run();
}

fn shrink_vec_length() {
    Hegel::new(|tc| {
        let v: Vec<i32> = tc.draw(gs::vecs(gs::integers::<i32>()).min_size(1).max_size(50));
        assert!(v.len() < 5);
    })
    .settings(settings())
    .run();
}

fn shrink_nested() {
    Hegel::new(|tc| {
        let v: Vec<Vec<i32>> = tc.draw(
            gs::vecs(gs::vecs(gs::integers::<i32>().min_value(0).max_value(100)).max_size(10))
                .min_size(1)
                .max_size(10),
        );
        let total: i32 = v.iter().flat_map(|inner| inner.iter()).sum();
        assert!(total < 50);
    })
    .settings(settings())
    .run();
}

fn shrink_text() {
    Hegel::new(|tc| {
        let s: String = tc.draw(gs::text().min_size(1).max_size(100));
        assert!(s.len() < 3);
    })
    .settings(settings())
    .run();
}

fn shrink_filter_map() {
    Hegel::new(|tc| {
        let n: i32 = tc.draw(
            gs::integers::<i32>()
                .min_value(0)
                .max_value(10000)
                .filter(|&x| x % 3 == 0)
                .map(|x| x * 2),
        );
        assert!(n < 100);
    })
    .settings(settings())
    .run();
}

fn shrink_hashmap() {
    Hegel::new(|tc| {
        let m: std::collections::HashMap<i32, i32> = tc.draw(
            gs::hashmaps(
                gs::integers::<i32>().min_value(0).max_value(100),
                gs::integers::<i32>().min_value(0).max_value(100),
            )
            .min_size(1)
            .max_size(20),
        );
        let total: i32 = m.values().sum();
        assert!(total < 50);
    })
    .settings(settings())
    .run();
}

// --- Flaky-assume tests (trigger FlakyStrategyDefinition) ---
// Time-based assume() calls produce inconsistent results across runs during
// shrinking, causing Hypothesis to raise FlakyStrategyDefinition. This error
// path is what exposed the original race conditions in hegel-core.

fn flaky_integer() {
    Hegel::new(|tc| {
        let x: i64 = tc.draw(gs::integers());
        let time_based = (SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 3)
            == 0;
        tc.assume(time_based || x.abs() < 1000);
        assert!(x <= 500);
    })
    .settings(settings())
    .run();
}

fn flaky_collection() {
    Hegel::new(|tc| {
        let numbers: Vec<i32> = tc.draw(gs::vecs(gs::integers::<i32>()));
        let filter_threshold = (SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_micros()
            % 100) as i32;
        tc.assume(numbers.len() < 20);
        tc.assume(numbers.iter().all(|&n| n.abs() < filter_threshold + 50));
        let sum: i64 = numbers.iter().map(|&x| x as i64).sum();
        assert!(sum <= 1000);
    })
    .settings(settings())
    .run();
}

fn flaky_text() {
    Hegel::new(|tc| {
        let text: String = tc.draw(gs::text());
        let number: i32 = tc.draw(gs::integers::<i32>().min_value(-100).max_value(100));
        let dynamic_limit = ((SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            / 1000)
            % 8) as usize
            + 2;
        tc.assume(text.len() <= dynamic_limit);
        assert!(!(text.len() > 5 && number.abs() > 50));
    })
    .settings(settings())
    .run();
}

fn flaky_boolean() {
    Hegel::new(|tc| {
        let flags: Vec<bool> = tc.draw(gs::vecs(gs::booleans()));
        let time_mod = (SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 4)
            == 0;
        tc.assume(flags.len() >= 5 && flags.len() <= 15);
        tc.assume(time_mod || flags.iter().filter(|&&b| b).count() < 8);
        let true_count = flags.iter().filter(|&&b| b).count();
        assert!(true_count <= 10);
    })
    .settings(settings())
    .run();
}

fn flaky_simple() {
    Hegel::new(|tc| {
        let x: i32 = tc.draw(gs::integers());
        let y: String = tc.draw(gs::text());
        let time_check = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            % 3
            == 0;
        tc.assume(time_check || x.abs() < 100);
        tc.assume(y.len() < 10);
        assert!(!(x > 50 && y.len() > 3));
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
    expected_to_fail: bool,
}

const TESTS: &[TestEntry] = &[
    TestEntry {
        name: "integers_basic",
        func: test_integers_basic,
        expected_to_fail: false,
    },
    TestEntry {
        name: "bounded_integers",
        func: test_bounded_integers,
        expected_to_fail: false,
    },
    TestEntry {
        name: "vec_properties",
        func: test_vec_properties,
        expected_to_fail: false,
    },
    TestEntry {
        name: "text_generation",
        func: test_text_generation,
        expected_to_fail: false,
    },
    TestEntry {
        name: "hashmap_generation",
        func: test_hashmap_generation,
        expected_to_fail: false,
    },
    TestEntry {
        name: "filter_and_map",
        func: test_filter_and_map,
        expected_to_fail: false,
    },
    TestEntry {
        name: "one_of_combinator",
        func: test_one_of_combinator,
        expected_to_fail: false,
    },
    TestEntry {
        name: "nested_collections",
        func: test_nested_collections,
        expected_to_fail: false,
    },
    TestEntry {
        name: "floats",
        func: test_floats,
        expected_to_fail: false,
    },
    TestEntry {
        name: "optional_and_sampled",
        func: test_optional_and_sampled,
        expected_to_fail: false,
    },
    TestEntry {
        name: "shrink_integers",
        func: shrink_integers,
        expected_to_fail: true,
    },
    TestEntry {
        name: "shrink_vec_length",
        func: shrink_vec_length,
        expected_to_fail: true,
    },
    TestEntry {
        name: "shrink_nested",
        func: shrink_nested,
        expected_to_fail: true,
    },
    TestEntry {
        name: "shrink_text",
        func: shrink_text,
        expected_to_fail: true,
    },
    TestEntry {
        name: "shrink_filter_map",
        func: shrink_filter_map,
        expected_to_fail: true,
    },
    TestEntry {
        name: "shrink_hashmap",
        func: shrink_hashmap,
        expected_to_fail: true,
    },
    TestEntry {
        name: "flaky_integer",
        func: flaky_integer,
        expected_to_fail: true,
    },
    TestEntry {
        name: "flaky_collection",
        func: flaky_collection,
        expected_to_fail: true,
    },
    TestEntry {
        name: "flaky_text",
        func: flaky_text,
        expected_to_fail: true,
    },
    TestEntry {
        name: "flaky_boolean",
        func: flaky_boolean,
        expected_to_fail: true,
    },
    TestEntry {
        name: "flaky_simple",
        func: flaky_simple,
        expected_to_fail: true,
    },
    TestEntry {
        name: "bad_external_randomness",
        func: bad_external_randomness,
        expected_to_fail: true,
    },
    TestEntry {
        name: "bad_shared_state",
        func: bad_shared_state,
        expected_to_fail: true,
    },
    TestEntry {
        name: "bad_timing_dependent",
        func: bad_timing_dependent,
        expected_to_fail: true,
    },
    TestEntry {
        name: "bad_concurrent_draws",
        func: bad_concurrent_draws,
        expected_to_fail: true,
    },
    TestEntry {
        name: "bad_assume_heavy",
        func: bad_assume_heavy,
        expected_to_fail: true,
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
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(msg) = info.payload().downcast_ref::<&str>() {
            if msg.contains("__HEGEL_") {
                return;
            }
        }
        if let Some(msg) = info.payload().downcast_ref::<String>() {
            if msg.contains("__HEGEL_") {
                return;
            }
        }
        prev_hook(info);
    }));

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
        "Starting stress test: {workers} workers, {timeout_secs}s timeout, {} tests ({} expected-fail)",
        TESTS.len(),
        TESTS.iter().filter(|t| t.expected_to_fail).count()
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

    // Log watcher thread: tails the server log file and streams new lines to stderr
    let log_logger = Arc::clone(&logger);
    let log_stop = Arc::new(AtomicBool::new(false));
    let log_stop_clone = Arc::clone(&log_stop);
    let log_watcher_handle = std::thread::spawn(move || {
        use std::io::{Read, Seek, SeekFrom};
        let mut pos: u64 = 0;
        let mut last_path: Option<String> = None;
        let scan = |pos: &mut u64, last_path: &mut Option<String>| {
            let path = hegel::server_log_path();
            if path.is_none() {
                return;
            }
            let path = path.unwrap();
            if last_path.as_ref() != Some(&path) {
                *pos = 0;
                *last_path = Some(path.clone());
            }
            if let Ok(mut file) = std::fs::File::open(&path) {
                if let Ok(metadata) = file.metadata() {
                    let len = metadata.len();
                    if len > *pos {
                        let _ = file.seek(SeekFrom::Start(*pos));
                        let mut buf = String::new();
                        if let Ok(n) = file.read_to_string(&mut buf) {
                            *pos += n as u64;
                            for line in buf.lines() {
                                log_logger.log(&format!("[server] {line}"));
                            }
                        }
                    }
                }
            }
        };
        while !log_stop_clone.load(Ordering::Relaxed) {
            scan(&mut pos, &mut last_path);
            std::thread::sleep(Duration::from_millis(500));
        }
        scan(&mut pos, &mut last_path);
    });

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

    // Swarm work queue: scheduler fills it, workers drain it
    let queue: Arc<(Mutex<VecDeque<usize>>, Condvar)> =
        Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));

    // Swarm scheduler thread: picks 1-5 tests, enqueues 100-1000 of them
    let sched_queue = Arc::clone(&queue);
    let sched_stop = Arc::clone(&stop);
    let scheduler_handle = std::thread::spawn(move || {
        let mut rng = rand::rng();
        while !sched_stop.load(Ordering::Relaxed) {
            let pool_size = rng.random_range(1..=5);
            let batch_size = rng.random_range(100..=1000);

            let pool: Vec<usize> = (0..pool_size)
                .map(|_| rng.random_range(0..TESTS.len()))
                .collect();

            let (lock, cvar) = &*sched_queue;
            for batch_i in 0..batch_size {
                if sched_stop.load(Ordering::Relaxed) {
                    return;
                }
                let test_idx = pool[batch_i % pool.len()];
                let mut q = lock.lock().unwrap();
                q.push_back(test_idx);
                cvar.notify_one();
            }
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
        let queue = Arc::clone(&queue);

        handles.push(
            std::thread::Builder::new()
                .name(format!("worker-{worker_id}"))
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let test_idx = {
                            let (lock, cvar) = &*queue;
                            let mut q = lock.lock().unwrap();
                            loop {
                                if let Some(idx) = q.pop_front() {
                                    break idx;
                                }
                                if stop.load(Ordering::Relaxed) {
                                    return;
                                }
                                let (guard, timeout_result) =
                                    cvar.wait_timeout(q, Duration::from_millis(100)).unwrap();
                                q = guard;
                                if timeout_result.timed_out() && stop.load(Ordering::Relaxed) {
                                    return;
                                }
                            }
                        };

                        let task_id = task_count.fetch_add(1, Ordering::Relaxed) as usize;
                        let test = &TESTS[test_idx];
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

                                let is_expected = test.expected_to_fail
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

    queue.1.notify_all();
    for h in handles {
        h.join().ok();
    }
    stop.store(true, Ordering::Relaxed);
    scheduler_handle.join().ok();
    monitor_handle.join().ok();
    collector_handle.join().ok();
    // Give the server a moment to flush logs, then stop the watcher
    std::thread::sleep(Duration::from_millis(500));
    log_stop.store(true, Ordering::Relaxed);
    log_watcher_handle.join().ok();

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

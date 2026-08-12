//! Parallel test-case execution via `Settings::threads`.
//!
//! Every assertion here is schedule-independent: which test case is
//! generated next under `threads > 1` depends on completion order, but the
//! failure count, the shrunk minimum, blob/database replay, health checks,
//! and error propagation must not.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::utils::{capture_hegel_output, expect_panic};
use hegel::generators as gs;
use hegel::{Hegel, Settings, TestCase};

fn parallel_settings() -> Settings {
    Settings::new().database(None).threads(4)
}

#[test]
fn test_parallel_passing_suite_passes() {
    Hegel::new_concurrent(|tc: TestCase| {
        let n: u64 = tc.draw(gs::integers::<u64>());
        let m: u64 = tc.draw(gs::integers::<u64>());
        assert_eq!(n.wrapping_add(m), m.wrapping_add(n));
    })
    .settings(parallel_settings())
    .run_concurrent();
}

#[test]
fn test_parallel_failure_shrinks_to_the_sequential_minimum() {
    expect_panic(
        || {
            Hegel::new_concurrent(|tc: TestCase| {
                let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
                assert!(n < 10, "n was {n}");
            })
            .settings(parallel_settings())
            .run_concurrent();
        },
        "n was 10",
    );
}

#[test]
fn test_parallel_reports_exactly_one_failure() {
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new_concurrent(|tc: TestCase| {
            let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
            assert!(n < 10, "n was {n}");
        })
        .settings(parallel_settings())
        .run_concurrent();
    });
    assert!(result.is_err());
    assert!(
        !lines.iter().any(|l| l.contains("distinct failures")),
        "a single bug must not be reported as multiple failures: {lines:?}"
    );
}

#[test]
fn test_parallel_printed_blob_replays() {
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new_concurrent(|tc: TestCase| {
            let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
            assert!(n < 10, "n was {n}");
        })
        .settings(parallel_settings().print_blob(true))
        .run_concurrent();
    });
    assert!(result.is_err());
    let blob_line = lines
        .iter()
        .find(|l| l.contains("reproduce_failure"))
        .expect("print_blob must print a reproducer line");
    let blob = blob_line
        .split('"')
        .nth(1)
        .expect("the reproducer line quotes the blob")
        .to_string();

    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
                assert!(n < 10, "n was {n}");
            })
            .settings(Settings::new().database(None))
            .reproduce_failure(blob)
            .run();
        },
        "n was 10",
    );
}

#[test]
fn test_parallel_database_entry_replays_sequentially() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let key = "test_parallel_database_entry_replays_sequentially";

    expect_panic(
        || {
            Hegel::new_concurrent(|tc: TestCase| {
                let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
                assert!(n < 10, "n was {n}");
            })
            .settings(
                Settings::new()
                    .database(Some(path.clone()))
                    .derandomize(false)
                    .threads(4),
            )
            .__database_key(key.to_string())
            .run_concurrent();
        },
        "n was 10",
    );

    let calls = AtomicUsize::new(0);
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                calls.fetch_add(1, Ordering::SeqCst);
                let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
                assert!(n < 10, "n was {n}");
            })
            .settings(
                Settings::new()
                    .database(Some(path.clone()))
                    .derandomize(false),
            )
            .__database_key(key.to_string())
            .run();
        },
        "n was 10",
    );
    assert!(
        calls.load(Ordering::SeqCst) <= 3,
        "the stored minimum must replay without a fresh search, ran {} cases",
        calls.load(Ordering::SeqCst)
    );
}

#[test]
fn test_parallel_filter_too_much_fires() {
    expect_panic(
        || {
            Hegel::new_concurrent(|tc: TestCase| {
                let n: i64 = tc.draw(gs::integers::<i64>());
                tc.assume(n == i64::MIN);
            })
            .settings(parallel_settings())
            .run_concurrent();
        },
        "FilterTooMuch",
    );
}

#[test]
fn test_parallel_bodies_actually_overlap() {
    let live = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let live_in = Arc::clone(&live);
    let peak_in = Arc::clone(&peak);
    Hegel::new_concurrent(move |tc: TestCase| {
        tc.draw(gs::booleans());
        let now = live_in.fetch_add(1, Ordering::SeqCst) + 1;
        peak_in.fetch_max(now, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(10));
        live_in.fetch_sub(1, Ordering::SeqCst);
    })
    .settings(parallel_settings().test_cases(100))
    .run_concurrent();
    assert!(
        peak.load(Ordering::SeqCst) > 1,
        "100 cases of ~10ms on 4 threads must overlap at least once"
    );
}

#[test]
fn test_parallel_invalid_argument_in_a_worker_surfaces_as_the_run_error() {
    expect_panic(
        || {
            Hegel::new_concurrent(|tc: TestCase| {
                tc.target(f64::NAN);
            })
            .settings(parallel_settings())
            .run_concurrent();
        },
        "requires a finite score",
    );
}

#[test]
fn test_run_rejects_threads_above_one() {
    expect_panic(
        || {
            Hegel::new(|_tc: TestCase| {})
                .settings(parallel_settings())
                .run();
        },
        "standalone_function.*does not yet support threads > 1",
    );
}

#[test]
fn test_run_concurrent_with_one_thread_is_the_sequential_run() {
    expect_panic(
        || {
            Hegel::new_concurrent(|tc: TestCase| {
                let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
                assert!(n < 10, "n was {n}");
            })
            .settings(Settings::new().database(None).threads(1))
            .run_concurrent();
        },
        "n was 10",
    );
}

#[test]
fn test_single_test_case_mode_ignores_threads() {
    Hegel::new_concurrent(|tc: TestCase| {
        tc.draw(gs::booleans());
    })
    .settings(
        Settings::new()
            .database(None)
            .mode(hegel::Mode::SingleTestCase)
            .threads(4),
    )
    .run_concurrent();
}

#[hegel::test(threads = 2, test_cases = 20)]
fn test_hegel_test_macro_accepts_threads(tc: TestCase) {
    let n: u32 = tc.draw(gs::integers::<u32>());
    let m = n;
    assert_eq!(n, m);
}

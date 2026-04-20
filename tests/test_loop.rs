//! Snapshot tests for `TestCase::repeat`.
//!
//! These run in-process via `hegel::Hegel::new(...).run()` and capture the
//! notes and draw output of the final (shrunk) failing replay using
//! `hegel::with_output_override`. Test bodies are wrapped in
//! `hegel::rewrite_draws!` so `tc.draw(gen)` calls get the same
//! named-variable rewriting that `#[hegel::test]` performs — this is what
//! makes `let x_1 = ...` appear in the snapshots instead of `let draw_1 = ...`.

mod common;

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hegel::generators as gs;
use hegel::{Hegel, Settings};

/// Run `body` as a Hegel property test and return the lines captured during
/// the final replay of the shrunk failing case. `body` is expected to trigger
/// a failure (otherwise no final replay is emitted and the snapshot is empty).
fn capture_loop_output<F>(body: F) -> String
where
    F: FnMut(hegel::TestCase) + 'static,
{
    let buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let buf_writer = buf.clone();
    let sink: Arc<dyn Fn(&str) + Send + Sync> =
        Arc::new(move |s: &str| buf_writer.lock().unwrap().push(s.to_string()));

    let _ = catch_unwind(AssertUnwindSafe(|| {
        hegel::with_output_override(sink, || {
            Hegel::new(body)
                .settings(
                    Settings::new()
                        .test_cases(200)
                        .database(None)
                        .derandomize(true),
                )
                .run();
        });
    }));

    let lines = buf.lock().unwrap().clone();
    lines.join("\n")
}

#[test]
fn snapshot_loop_fails_on_first_iteration() {
    let output = capture_loop_output(hegel::rewrite_draws!(|tc: hegel::TestCase| {
        tc.repeat(|| {
            let x: i32 = tc.draw(gs::integers::<i32>());
            assert!(x < 10);
        });
    }));
    insta::assert_snapshot!(output, @"
    // Repetition #1
      let x_1 = 10;
    ");
}

#[test]
fn snapshot_loop_runs_multiple_iterations_before_failing() {
    let output = capture_loop_output(hegel::rewrite_draws!(|tc: hegel::TestCase| {
        let mut count = 0;
        tc.repeat(|| {
            count += 1;
            assert!(count < 3);
        });
    }));
    insta::assert_snapshot!(output, @"
    // Repetition #1
    // Repetition #2
    // Repetition #3
    ");
}

#[test]
fn snapshot_loop_with_multiple_draws_per_iteration() {
    let output = capture_loop_output(hegel::rewrite_draws!(|tc: hegel::TestCase| {
        tc.repeat(|| {
            let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(100));
            let y: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(100));
            assert!(x + y < 5);
        });
    }));
    insta::assert_snapshot!(output, @"
    // Repetition #1
      let x_1 = 0;
      let y_1 = 5;
    ");
}

#[test]
fn snapshot_loop_accumulates_state_across_iterations() {
    let output = capture_loop_output(hegel::rewrite_draws!(|tc: hegel::TestCase| {
        let total: Rc<Cell<i32>> = Rc::new(Cell::new(0));
        let total_inside = total.clone();
        tc.repeat(|| {
            let n: i32 = tc.draw(gs::integers::<i32>().min_value(1).max_value(10));
            total_inside.set(total_inside.get() + n);
            assert!(total_inside.get() < 5);
        });
    }));
    insta::assert_snapshot!(output, @"
    // Repetition #1
      let n_1 = 5;
    ");
}

#[test]
fn loop_recovers_from_assumption_failures() {
    // Every odd-indexed iteration calls assume(false). Even iterations
    // increment a successful-iteration counter and assert it stays below 3.
    // The loop must continue past the odd iterations; if it did not, the
    // successes counter would never reach 3 and the test would not fail.
    let result = catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(|tc| {
            let mut iteration = 0;
            let mut successes = 0;
            tc.repeat(|| {
                iteration += 1;
                if iteration % 2 == 1 {
                    tc.assume(false);
                }
                successes += 1;
                assert!(successes < 3);
            });
        })
        .settings(
            Settings::new()
                .test_cases(200)
                .database(None)
                .derandomize(true),
        )
        .run();
    }));
    assert!(
        result.is_err(),
        "expected the loop test to fail once successes >= 3",
    );
}

#[test]
fn loop_terminates_when_body_never_panics() {
    // With a body that always succeeds, the loop should still terminate (via
    // the backend exhausting its budget and raising StopTest). If repeat did
    // not catch StopTest, this test would fail with an Overrun.
    //
    // The native backend's repeat loop can run thousands of iterations
    // (up to BUFFER_SIZE), which in unoptimized debug builds overflows
    // the default 8 MB test-thread stack. Run in a thread with more room.
    let iterations = Arc::new(AtomicU64::new(0));
    let iterations_thread = iterations.clone();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let iterations_inside = iterations_thread.clone();
            Hegel::new(move |tc| {
                let iterations = iterations_inside.clone();
                tc.repeat(|| {
                    iterations.fetch_add(1, Ordering::Relaxed);
                });
            })
            .settings(
                Settings::new()
                    .test_cases(3)
                    .database(None)
                    .derandomize(true),
            )
            .run();
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(
        iterations.load(Ordering::Relaxed) > 0,
        "expected the loop body to execute at least once",
    );
}

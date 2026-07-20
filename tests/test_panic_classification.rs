//! Hegel's internal control flow (assume rejection, stop-test, loop-done,
//! invalid-argument) unwinds through the same `catch_unwind` that catches a
//! test body's genuine panics, so the lifecycle has to tell the two apart.
//! These tests pin that the classification cannot be confused by the
//! *content* of a user panic, and that control-flow unwinds never reach any
//! panic hook (no `thread '...' panicked` noise, on any thread).

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::exec::fixture;
use hegel::generators as gs;
use hegel::{Hegel, Settings, TestCase, Verbosity};

/// Run a property whose body always panics with `msg` and return the
/// panic message the run re-raises. The property must *fail* — a user
/// panic, no matter what its text is, is a bug in the code under test,
/// never control flow.
fn run_property_panicking_with(msg: &'static str) -> String {
    let result = catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(move |tc: TestCase| {
            tc.draw(gs::booleans());
            panic!("{}", msg);
        })
        .settings(
            Settings::new()
                .database(None)
                .derandomize(true)
                .test_cases(10)
                .verbosity(Verbosity::Quiet),
        )
        .run();
    }));
    let payload = result.expect_err("a panicking property must fail its run");
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

#[test]
fn user_panic_matching_assume_sentinel_is_a_failure() {
    assert_eq!(
        run_property_panicking_with("__HEGEL_ASSUME_FAIL"),
        "__HEGEL_ASSUME_FAIL"
    );
}

#[test]
fn user_panic_matching_stop_test_sentinel_is_a_failure() {
    assert_eq!(
        run_property_panicking_with("__HEGEL_STOP_TEST"),
        "__HEGEL_STOP_TEST"
    );
}

#[test]
fn user_panic_matching_loop_done_sentinel_is_a_failure() {
    assert_eq!(
        run_property_panicking_with("__HEGEL_LOOP_DONE"),
        "__HEGEL_LOOP_DONE"
    );
}

#[test]
fn user_panic_matching_invalid_argument_prefix_is_a_failure() {
    assert_eq!(
        run_property_panicking_with("__HEGEL_INVALID_ARGUMENT: boom"),
        "__HEGEL_INVALID_ARGUMENT: boom"
    );
}

/// Regression test: a control-flow "panic" raised on a spawned thread used
/// to hit the *default* panic hook (the suppressing hook only recognises
/// test context through a thread-local), printing a
/// `thread '<unnamed>' panicked at ...` line with the internal sentinel for
/// every rejected test case. Control-flow unwinds must be invisible on
/// stderr no matter which thread raises them.
#[test]
fn worker_thread_rejections_print_no_panic_noise() {
    let output = fixture(env!("CARGO_BIN_EXE_fixture_threaded_rejects")).run();
    assert!(
        !output.stderr.contains("panicked"),
        "worker-thread rejections must not reach any panic hook, got:\n{}",
        output.stderr
    );
}

//! In-process tests for the runner's multi-failure reporting path.
//!
//! The runner has three failure-reporting branches (`src/server/runner.rs`):
//!   - empty (`Property test failed: unknown`)
//!   - single failure (legacy `Property test failed: <msg>`)
//!   - multi-failure (new `Property-based test failed with N distinct failures.`)
//!
//! The single-failure branch is exercised by the rest of the test suite via
//! `expect_panic` / `Minimal::run`.  The multi-failure branch needs a test
//! that *deterministically* drives Hypothesis to surface multiple distinct
//! origins — relying on a happens-to-find-two test would leave the branch
//! uncovered on CI, which it did before this test was added.

use std::panic::{AssertUnwindSafe, catch_unwind};

use hegel::generators as gs;
use hegel::{Hegel, Settings, TestCase, Verbosity};

/// `#[hegel::test(report_multiple_failures = true, ...)]` (the default)
/// surfaces every distinct origin. With two panic sites in the body the
/// outer re-raise must be the new
/// `"Property-based test failed with 2 distinct failures."` panic — if the
/// macro silently dropped the argument or the runner collapsed to one,
/// `#[should_panic]` would fail to match.
#[hegel::test(
    report_multiple_failures = true,
    derandomize = true,
    test_cases = 500u64,
    verbosity = hegel::Verbosity::Quiet,
    database = None
)]
#[should_panic(expected = "Property-based test failed with 2 distinct failures.")]
fn test_macro_report_multiple_failures_true_surfaces_both(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(40));
    if x > 30 {
        panic!("big branch: {}", x);
    }
    if x < 10 {
        panic!("small branch: {}", x);
    }
}

/// `#[hegel::test(report_multiple_failures = false, ...)]` asks the server
/// to collapse multi-bug runs to a single failure, so the same body falls
/// into the single-failure re-raise path. The `expected` substring is
/// chosen specifically so a multi-failure panic (which starts with
/// `"Property-based test failed"` — note the hyphen, no colon) would not
/// match and the test would fail.
#[hegel::test(
    report_multiple_failures = false,
    derandomize = true,
    test_cases = 500u64,
    verbosity = hegel::Verbosity::Quiet,
    database = None
)]
#[should_panic(expected = "Property test failed: ")]
fn test_macro_report_multiple_failures_false_collapses(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(40));
    if x > 30 {
        panic!("big branch: {}", x);
    }
    if x < 10 {
        panic!("small branch: {}", x);
    }
}

/// Run a test body with two distinct `panic!` call sites and assert the
/// outer panic is the multi-failure message.  `verbosity` is parameterised
/// so we can cover both the verbose-print branch (eprintln of the header
/// and per-failure diagnostics) and the quiet-suppress branch.
fn run_two_origin_failure(verbosity: Verbosity) -> String {
    let result = catch_unwind(AssertUnwindSafe(move || {
        Hegel::new(|tc: TestCase| {
            let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(40));
            if x > 30 {
                panic!("big branch: {}", x);
            }
            if x < 10 {
                panic!("small branch: {}", x);
            }
        })
        .settings(
            Settings::new()
                .database(None)
                .derandomize(true)
                .verbosity(verbosity)
                .test_cases(500),
        )
        .run();
    }));

    let payload = result.expect_err("expected run() to panic with multiple failures");
    let msg = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    assert!(
        msg.starts_with("Property-based test failed with ") && msg.ends_with(" distinct failures."),
        "expected multi-failure outer panic, got: {msg:?}"
    );
    msg
}

/// Default verbosity: covers the eprintln branch that prints the
/// "Hegel found N failing test cases:" header and each diagnostic.
#[test]
fn test_multi_failure_panic_with_default_verbosity() {
    // Just need to swallow the captured value to silence unused warnings.
    let _ = run_two_origin_failure(Verbosity::Normal);
}

/// Quiet verbosity: covers the branch that suppresses the per-failure
/// stderr output but still re-panics with the multi-failure count.
#[test]
fn test_multi_failure_panic_quiet_suppresses_stderr() {
    let _ = run_two_origin_failure(Verbosity::Quiet);
}

/// `report_multiple_failures(false)` makes the server (Hypothesis with
/// `report_multiple_bugs=False`) surface only the first origin, so the
/// runner sees a single Failure and falls into the legacy single-failure
/// re-raise path (`Property test failed: <msg>`) rather than the
/// multi-failure `"... with N distinct failures."` path.
#[test]
fn test_report_multiple_failures_false_collapses_to_single_failure_panic() {
    let result = catch_unwind(AssertUnwindSafe(move || {
        Hegel::new(|tc: TestCase| {
            let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(40));
            if x > 30 {
                panic!("big branch: {}", x);
            }
            if x < 10 {
                panic!("small branch: {}", x);
            }
        })
        .settings(
            Settings::new()
                .database(None)
                .derandomize(true)
                .verbosity(Verbosity::Quiet)
                .test_cases(500)
                .report_multiple_failures(false),
        )
        .run();
    }));

    let payload = result.expect_err("expected run() to panic");
    let msg = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    assert!(
        msg.starts_with("Property test failed: ") && !msg.contains("distinct failures"),
        "expected single-failure outer panic (one branch should win), got: {msg:?}"
    );
}

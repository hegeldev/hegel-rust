//! In-process tests for the runner's multi-failure reporting path.
//!
//! The runner has three failure-reporting branches (`src/run_lifecycle.rs`):
//!   - empty (`Property test failed: unknown`)
//!   - single failure (the test's own panic, re-raised)
//!   - multi-failure (new `Property-based test failed with N distinct failures.`)
//!
//! The single-failure branch is exercised by the rest of the test suite via
//! `expect_panic` / `Minimal::run`.  The multi-failure branch needs a test
//! that *deterministically* drives the engine to surface multiple distinct
//! origins — relying on a happens-to-find-two test would leave the branch
//! uncovered on CI, which it did before this test was added.

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::project::TempRustProject;
use common::utils::assert_matches_regex;
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

/// `#[hegel::test(report_multiple_failures = false, ...)]` asks the engine
/// to collapse multi-bug runs to a single failure, so the same body falls
/// into the single-failure re-raise path. The `expected` substring is
/// chosen specifically so a multi-failure panic (`"Property-based test
/// failed with N distinct failures."`) would not match and the test would
/// fail: only a single re-raised branch panic contains `"branch: "`.
#[hegel::test(
    report_multiple_failures = false,
    derandomize = true,
    test_cases = 500u64,
    verbosity = hegel::Verbosity::Quiet,
    database = None
)]
#[should_panic(expected = "branch: ")]
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
/// "Property-based test failed with N distinct failures." headline and
/// each diagnostic.
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

/// `report_multiple_failures(false)` makes the engine surface only the first
/// origin, so the runner sees a single failure and re-raises that test
/// panic itself rather than the multi-failure
/// `"... with N distinct failures."` panic.
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
        msg.contains("branch: ") && !msg.contains("distinct failures"),
        "expected a single re-raised branch panic (one branch should win), got: {msg:?}"
    );
}

const TWO_BUG_CODE: &str = r#"
use hegel::{Hegel, Settings};
use hegel::generators as gs;

fn main() {
    Hegel::new(|tc| {
        let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(40));
        if x > 30 {
            panic!("big branch: {}", x);
        }
        if x < 10 {
            panic!("small branch: {}", x);
        }
    })
    .settings(Settings::new().database(None).derandomize(true).test_cases(500))
    .run();
}
"#;

/// Regression test for the multi-failure report's stderr layout. The draws
/// of each final replay used to print live while the diagnostics were
/// buffered and printed at the end, so a two-failure report came out as
/// both `let ... = ...;` lines first, then both panic diagnostics — with no
/// way to tell which counterexample belonged to which stack trace. The
/// headline count also only appeared at the very end (it was only ever the
/// final `panic!` payload, which the panic hook prints after the report).
///
/// The report must instead lead with the count, followed by one
/// self-contained block per failure: that failure's draws immediately
/// followed by its panic diagnostic.
#[test]
fn test_multi_failure_report_groups_draws_with_their_diagnostics() {
    let output = TempRustProject::new()
        .main_file(TWO_BUG_CODE)
        .invoke()
        // No backtraces: each diagnostic must end right after its panic
        // message so the block structure below is exact.
        .env_remove("RUST_BACKTRACE")
        .expect_failure("Property-based test failed with 2 distinct failures.")
        .cargo_run(&[]);
    let stderr = &output.stderr;

    // The headline comes before any drawn value or diagnostic.
    let headline_pos = stderr
        .find("Property-based test failed with 2 distinct failures.")
        .unwrap();
    let first_draw_pos = stderr.find("let draw_1 = ").unwrap();
    let first_diagnostic_pos = stderr.find("panicked at src").unwrap();
    assert!(
        headline_pos < first_draw_pos && headline_pos < first_diagnostic_pos,
        "expected the failure-count headline before the per-failure blocks:\n{stderr}"
    );

    // Two blocks, each a draw line immediately followed by its diagnostic.
    assert_matches_regex(
        stderr,
        concat!(
            r"Property-based test failed with 2 distinct failures\.\n",
            r"\n",
            r"let draw_1 = \d+;\n",
            r"thread '[^']+' \(\d+\) panicked at src[/\\]main\.rs:\d+:\d+:\n",
            r"(?:big|small) branch: \d+\n",
            r"\n",
            r"let draw_1 = \d+;\n",
            r"thread '[^']+' \(\d+\) panicked at src[/\\]main\.rs:\d+:\d+:\n",
            // The report is the last thing on stderr: the closing unwind is
            // a hook-silent re-raise, so nothing prints after the blocks.
            r"(?:big|small) branch: \d+$",
        ),
    );
}

/// A second bug lurking just below the first bug's shrink boundary: from the
/// full `u64` range, generation essentially never produces 998 or 999, so
/// the "shadow bug" is only ever reached by the shrinker probing values just
/// below the primary boundary. Hypothesis records origins discovered while
/// shrinking and shrinks them too; the runner must report both.
#[hegel::test(
    derandomize = true,
    test_cases = 300u64,
    verbosity = hegel::Verbosity::Quiet,
    database = None
)]
#[should_panic(expected = "Property-based test failed with 2 distinct failures.")]
fn test_bug_discovered_during_shrink_is_reported(tc: TestCase) {
    let x: u64 = tc.draw(gs::integers::<u64>());
    if x >= 1000 {
        panic!("primary bug: {}", x);
    }
    if x >= 998 {
        panic!("shadow bug: {}", x);
    }
}

//! Ported from hypothesis-python/tests/cover/test_verbosity.py
//!
//! test_prints_initial_attempts_on_find is omitted: it uses hypothesis.find(),
//! a public API with no hegel-rust counterpart.

use std::sync::OnceLock;

use crate::common::project::TempRustProject;
use hegel::generators as gs;
use hegel::{Hegel, Settings, Verbosity};

const VERBOSE_PASSING_CODE: &str = r#"
use hegel::{Hegel, Settings, Verbosity};
use hegel::generators as gs;

fn main() {
    Hegel::new(|tc| {
        let _: bool = tc.draw(gs::booleans());
    })
    .settings(Settings::new().verbosity(Verbosity::Verbose))
    .run();
}
"#;

fn verbose_passing_project() -> &'static TempRustProject {
    static PROJECT: OnceLock<TempRustProject> = OnceLock::new();
    PROJECT.get_or_init(|| TempRustProject::new().main_file(VERBOSE_PASSING_CODE))
}

// Use non-negative bounded integers to avoid overflow in sum. The condition
// sum < 100 is easily falsifiable with any element >= 100.
const VERBOSE_FAILING_CODE: &str = r#"
use hegel::{Hegel, Settings, Verbosity};
use hegel::generators as gs;

fn main() {
    Hegel::new(|tc| {
        let x: Vec<i64> = tc.draw(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(10000)).min_size(1)
        );
        assert!(x.iter().sum::<i64>() < 100);
    })
    .settings(Settings::new().verbosity(Verbosity::Verbose).database(None))
    .run();
}
"#;

fn verbose_failing_project() -> &'static TempRustProject {
    static PROJECT: OnceLock<TempRustProject> = OnceLock::new();
    PROJECT.get_or_init(|| {
        TempRustProject::new()
            .main_file(VERBOSE_FAILING_CODE)
            .expect_failure("assertion failed")
    })
}

const QUIET_FAILING_CODE: &str = r#"
use hegel::{Hegel, Settings, Verbosity};
use hegel::generators as gs;

fn main() {
    Hegel::new(|tc| {
        let x: bool = tc.draw(gs::booleans());
        assert!(x, "x should be true");
    })
    .settings(Settings::new().verbosity(Verbosity::Quiet).database(None))
    .run();
}
"#;

fn quiet_failing_project() -> &'static TempRustProject {
    static PROJECT: OnceLock<TempRustProject> = OnceLock::new();
    PROJECT.get_or_init(|| {
        TempRustProject::new()
            .main_file(QUIET_FAILING_CODE)
            .expect_failure("x should be true")
    })
}

#[test]
fn test_prints_intermediate_in_success() {
    let output = verbose_passing_project().cargo_run(&[]);
    assert!(
        output.stderr.contains("Trying example"),
        "Expected 'Trying example' in stderr:\n{}",
        output.stderr
    );
}

#[test]
fn test_does_not_log_in_quiet_mode() {
    let output = quiet_failing_project().cargo_run(&[]);
    assert!(
        !output.stderr.contains("Trying example"),
        "Unexpected progress output in quiet mode:\n{}",
        output.stderr
    );
}

#[test]
fn test_includes_progress_in_verbose_mode() {
    let output = verbose_failing_project().cargo_run(&[]);
    assert!(
        output.stderr.contains("Trying example: "),
        "Expected 'Trying example: ' in stderr:\n{}",
        output.stderr
    );
}

#[test]
fn test_includes_intermediate_results_in_verbose_mode() {
    let output = verbose_failing_project().cargo_run(&[]);
    let example_lines = output
        .stderr
        .lines()
        .filter(|l| l.contains("example"))
        .count();
    assert!(
        example_lines > 2,
        "Expected more than 2 lines containing 'example', got {}:\n{}",
        example_lines,
        output.stderr
    );
    assert!(
        output.stderr.contains("assertion failed"),
        "Expected assertion failure message in stderr:\n{}",
        output.stderr
    );
}

#[test]
fn test_no_indexerror_in_quiet_mode() {
    // Regression: quiet mode should not crash
    Hegel::new(|tc| {
        let _x: i64 = tc.draw(gs::integers());
    })
    .settings(Settings::new().verbosity(Verbosity::Quiet))
    .run();
}

#[test]
fn test_verbose_run_succeeds_in_process() {
    // Exercises the verbose logging path (the "Trying example" emission in
    // the runner) from inside the test binary, so coverage instrumentation
    // records it. The TempRustProject-based tests above rely on subprocess
    // binaries that are not built with coverage instrumentation.
    Hegel::new(|tc| {
        let _x: bool = tc.draw(gs::booleans());
    })
    .settings(Settings::new().verbosity(Verbosity::Verbose).database(None))
    .run();
}

#[test]
fn test_no_indexerror_in_quiet_mode_report_multiple() {
    // report_multiple_bugs has no hegel-rust equivalent; verify quiet mode
    // doesn't crash unexpectedly on a failing test.
    quiet_failing_project().cargo_run(&[]);
}

#[test]
fn test_no_indexerror_in_quiet_mode_report_one() {
    quiet_failing_project().cargo_run(&[]);
}

//! End-to-end tests for `#[hegel::main]` binaries, driven against the
//! prebuilt fixture binaries in `tests/fixtures/` (see the `[[bin]]` targets
//! in Cargo.toml). `#[hegel::main]`'s compile-time validation lives in the
//! UI tests (`tests/ui/main_no_params.rs`).

mod common;

use common::exec::fixture;
use common::utils::assert_matches_regex;

const BASIC_MAIN: &str = env!("CARGO_BIN_EXE_fixture_basic_main");
const MAIN_SIMPLE: &str = env!("CARGO_BIN_EXE_fixture_main_simple");
const MAIN_FAILING: &str = env!("CARGO_BIN_EXE_fixture_main_failing");
const MAIN_REWRITE: &str = env!("CARGO_BIN_EXE_fixture_main_rewrite");
const MAIN_EXPLICIT: &str = env!("CARGO_BIN_EXE_fixture_main_explicit");

#[test]
fn test_basic_main_runs() {
    let output = fixture(BASIC_MAIN).run();
    let count = output.stderr.matches("ran").count();
    assert_eq!(count, 7, "stderr:\n{}", output.stderr);
}

#[test]
fn test_main_cli_overrides_test_cases() {
    let output = fixture(BASIC_MAIN).args(&["--test-cases", "3"]).run();
    let count = output.stderr.matches("ran").count();
    assert_eq!(count, 3, "stderr:\n{}", output.stderr);
}

#[test]
fn test_main_default_matches_attribute() {
    let output = fixture(BASIC_MAIN).run();
    let count = output.stderr.matches("ran").count();
    assert_eq!(count, 7, "stderr:\n{}", output.stderr);
}

#[test]
fn test_main_unknown_arg_exits_with_error() {
    let output = fixture(BASIC_MAIN)
        .arg("--not-a-real-arg")
        .expect_failure("Unknown argument")
        .run();
    let _ = output;
}

#[test]
fn test_main_help_exits_cleanly() {
    let output = fixture(MAIN_SIMPLE).arg("--help").run();
    assert!(
        output.stdout.contains("Usage:"),
        "stdout did not contain Usage: {}",
        output.stdout
    );
}

#[test]
fn test_main_failing_property_exits_nonzero() {
    fixture(MAIN_FAILING).expect_failure("got nonneg").run();
}

#[test]
fn test_main_draw_name_rewriting() {
    let output = fixture(MAIN_REWRITE).expect_failure("boom").run();
    assert_matches_regex(&output.stderr, r"let my_var = -?\d+;");
}

#[test]
fn test_main_explicit_test_case() {
    fixture(MAIN_EXPLICIT)
        .expect_failure("got explicit value")
        .run();
}

#[test]
fn test_main_verbosity_override() {
    let output = fixture(MAIN_SIMPLE)
        .args(&["--verbosity", "debug", "--test-cases", "1"])
        .run();
    assert!(
        output.stderr.contains("test case #") || output.stderr.contains("Test done."),
        "Expected debug output, got: {}",
        output.stderr
    );
}

#[test]
fn test_main_seed_override() {
    let output = fixture(MAIN_SIMPLE).args(&["--seed", "42"]).run();
    let _ = output;
}

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
fn test_basic_main_runs_exactly_one_test_case() {
    let output = fixture(BASIC_MAIN).run();
    let count = output.stderr.matches("ran").count();
    assert_eq!(count, 1, "stderr:\n{}", output.stderr);
}

#[test]
fn test_main_env_var_cannot_override_the_test_case_count() {
    let output = fixture(BASIC_MAIN).env("HEGEL_TEST_CASES", "50").run();
    let count = output.stderr.matches("ran").count();
    assert_eq!(count, 1, "stderr:\n{}", output.stderr);
}

#[test]
fn test_main_persists_failures_to_the_database() {
    let db_dir = tempfile::TempDir::new().unwrap();
    let db_path = db_dir.path().join("db");
    fixture(MAIN_FAILING)
        .args(&["--database", db_path.to_str().unwrap()])
        .expect_failure("got nonneg")
        .run();
    assert!(db_path.is_dir());
}

#[test]
fn test_main_removed_flags_are_unknown_arguments() {
    for args in [&["--test-cases", "3"][..], &["--single-test-case"][..]] {
        fixture(BASIC_MAIN)
            .args(args)
            .expect_failure("Unknown argument")
            .run();
    }
}

#[test]
fn test_main_unknown_arg_exits_with_error() {
    fixture(BASIC_MAIN)
        .arg("--not-a-real-arg")
        .expect_failure("Unknown argument")
        .run();
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
    let output = fixture(MAIN_SIMPLE).args(&["--verbosity", "debug"]).run();
    assert!(
        output.stderr.contains("test case #") || output.stderr.contains("Test done."),
        "Expected debug output, got: {}",
        output.stderr
    );
}

#[test]
fn test_main_seed_override() {
    fixture(MAIN_SIMPLE).args(&["--seed", "42"]).run();
}

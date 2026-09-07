//! A failing run always ends with a reproduction pointer: the line naming
//! the database directory the example was saved to, or, when nothing was
//! saved, a copy-pasteable `#[hegel::reproduce_failure("…")]` reproducer
//! line (the `print_blob` setting forces the reproducer line on).
//!
//! These lines are written straight to stderr at the catch site, so the
//! tests run an `#[ignore]`d failing fixture test in a subprocess (this same
//! binary, via `exec::self_test`) or a fixture binary, and assert on the
//! combined output. `HEGEL_DATABASE` pins the database state so the
//! assertions hold both locally (database on by default) and in CI
//! (database off by default).

mod common;

use common::exec::{fixture, self_test};
use hegel::TestCase;
use hegel::generators as gs;

/// Marker printed by the reproducer line (see `run_lifecycle::reproducer_line`).
const REPRODUCER_MARKER: &str = "To reproduce this failure";

/// Marker printed by the saved-to-database line (see
/// `run_lifecycle::saved_to_database_line`).
const SAVED_MARKER: &str = "was saved to";

const OUTPUT_FAILING: &str = env!("CARGO_BIN_EXE_fixture_output_failing");

#[hegel::test(print_blob = true)]
#[ignore = "fixture: run via exec::self_test"]
fn print_blob_true_fixture(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers());
    assert!(x < 5, "x was {x}");
}

#[hegel::test]
#[ignore = "fixture: run via exec::self_test"]
fn print_blob_default_fixture(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers());
    assert!(x < 5, "x was {x}");
}

#[test]
fn print_blob_true_prints_reproducer_line() {
    self_test("print_blob_true_fixture")
        .env("HEGEL_DATABASE", "disabled")
        .expect_failure(REPRODUCER_MARKER)
        .run();
}

#[test]
fn print_blob_true_with_database_prints_reproducer_and_saved_lines() {
    let out = self_test("print_blob_true_fixture")
        .env("HEGEL_DATABASE", "print-blob-true-db")
        .expect_failure(REPRODUCER_MARKER)
        .run();
    let combined = format!("{}\n{}", out.stdout, out.stderr);
    assert!(
        combined.contains("The failing example was saved to 'print-blob-true-db'"),
        "expected the saved-to-database line:\n{combined}"
    );
}

#[test]
fn database_save_replaces_reproducer_line_with_saved_line() {
    let out = self_test("print_blob_default_fixture")
        .env("HEGEL_DATABASE", "print-blob-default-db")
        .expect_failure("x was")
        .run();
    let combined = format!("{}\n{}", out.stdout, out.stderr);
    assert!(
        combined.contains("The failing example was saved to 'print-blob-default-db'"),
        "expected the saved-to-database line:\n{combined}"
    );
    assert!(
        combined.contains("Rerunning this test will replay it."),
        "expected the replay pointer:\n{combined}"
    );
    assert!(
        !combined.contains(REPRODUCER_MARKER),
        "reproducer line should be suppressed when the example was saved:\n{combined}"
    );
}

#[test]
fn disabled_database_prints_reproducer_line_by_default() {
    let out = self_test("print_blob_default_fixture")
        .env("HEGEL_DATABASE", "disabled")
        .expect_failure(REPRODUCER_MARKER)
        .run();
    let combined = format!("{}\n{}", out.stdout, out.stderr);
    assert!(
        combined.contains("#[hegel::reproduce_failure(\""),
        "expected the attribute wording:\n{combined}"
    );
    assert!(
        !combined.contains(SAVED_MARKER),
        "saved-to-database line should be absent without a database:\n{combined}"
    );
}

#[test]
fn bare_hegel_run_prints_builder_reproducer_wording() {
    let out = fixture(OUTPUT_FAILING)
        .expect_failure(REPRODUCER_MARKER)
        .run();
    let combined = format!("{}\n{}", out.stdout, out.stderr);
    assert!(
        combined.contains("Hegel::new(...).reproduce_failure(\""),
        "expected the builder wording for a run without a database key:\n{combined}"
    );
    assert!(
        !combined.contains(SAVED_MARKER),
        "a run without a database key saves nothing:\n{combined}"
    );
}

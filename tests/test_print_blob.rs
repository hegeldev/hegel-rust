//! The `print_blob` setting controls whether a failing run prints a
//! copy-pasteable `#[hegel::reproduce_failure("…")]` reproducer line.
//!
//! The reproducer line is written straight to stderr at the catch site, so
//! these run an `#[ignore]`d failing fixture test in a subprocess (this same
//! binary, via `exec::self_test`) and assert the line is present when
//! `print_blob` is set and absent otherwise.

mod common;

use common::exec::self_test;
use hegel::TestCase;
use hegel::generators as gs;

/// Marker printed by the reproducer line (see `run_lifecycle::reproducer_line`).
const REPRODUCER_MARKER: &str = "To reproduce this failure";

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
        .expect_failure(REPRODUCER_MARKER)
        .run();
}

#[test]
fn print_blob_default_suppresses_reproducer_line() {
    let out = self_test("print_blob_default_fixture")
        .expect_failure("x was")
        .run();
    let combined = format!("{}\n{}", out.stdout, out.stderr);
    assert!(
        !combined.contains(REPRODUCER_MARKER),
        "reproducer line should be suppressed without print_blob:\n{combined}"
    );
}

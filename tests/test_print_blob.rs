//! The `print_blob` setting controls whether a failing run prints a
//! copy-pasteable `#[hegel::reproduce_failure("…")]` reproducer line.
//!
//! These run a failing `#[hegel::test]` in a subprocess (so its stderr can be
//! captured) and assert the reproducer line is present when `print_blob` is
//! set and absent otherwise.

mod common;

use common::project::TempRustProject;

/// Marker printed by the reproducer line (see `run_lifecycle::reproducer_line`).
const REPRODUCER_MARKER: &str = "To reproduce this failure";

#[test]
fn print_blob_true_prints_reproducer_line() {
    let code = r#"
#[hegel::test(print_blob = true)]
fn my_test(tc: hegel::TestCase) {
    use hegel::generators as gs;
    let x: i32 = tc.draw(gs::integers());
    assert!(x < 5, "x was {x}");
}
"#;
    TempRustProject::new()
        .main_file("fn main() {}")
        .test_file("repro.rs", code)
        .expect_failure(REPRODUCER_MARKER)
        .cargo_test(&["--test", "repro"]);
}

#[test]
fn print_blob_default_suppresses_reproducer_line() {
    let code = r#"
#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    use hegel::generators as gs;
    let x: i32 = tc.draw(gs::integers());
    assert!(x < 5, "x was {x}");
}
"#;
    let out = TempRustProject::new()
        .main_file("fn main() {}")
        .test_file("repro.rs", code)
        .invoke()
        .expect_failure("x was")
        .cargo_test(&["--test", "repro"]);
    let combined = format!("{}\n{}", out.stdout, out.stderr);
    assert!(
        !combined.contains(REPRODUCER_MARKER),
        "reproducer line should be suppressed without print_blob:\n{combined}"
    );
}

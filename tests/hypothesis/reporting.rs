//! Ported from hypothesis-python/tests/cover/test_reporting.py
//!
//! Individually-skipped tests:
//!
//! - `test_does_not_print_debug_in_verbose`,
//!   `test_does_print_debug_in_debug`,
//!   `test_does_print_verbose_in_debug` — exercise
//!   `hypothesis.reporting.debug_report` / `verbose_report`, public APIs
//!   for verbosity-gated user logging that hegel-rust does not expose.
//!   The closest analog, `tc.note()`, is verbosity-independent and only
//!   fires on the final failing-test replay.
//!
//! - `test_can_report_when_system_locale_is_ascii` — relies on Python
//!   `monkeypatch.setattr(sys, "stdout", ...)` and `os.pipe()` to swap
//!   the process stdout for an ASCII-only stream. Both are
//!   Python-specific facilities with no Rust counterpart.

use std::sync::OnceLock;

use crate::common::project::TempRustProject;

const FAILING_TEST_CODE: &str = r#"
use hegel::{Hegel, Settings};
use hegel::generators as gs;

fn main() {
    Hegel::new(|tc| {
        let _x: i64 = tc.draw(gs::integers());
        panic!("intentional failure");
    })
    .settings(Settings::new().database(None))
    .run();
}
"#;

fn failing_project() -> &'static TempRustProject {
    static PROJECT: OnceLock<TempRustProject> = OnceLock::new();
    PROJECT.get_or_init(|| {
        TempRustProject::new()
            .main_file(FAILING_TEST_CODE)
            .expect_failure("intentional failure")
    })
}

#[test]
fn test_prints_output_by_default() {
    // Hypothesis prints "Falsifying example: test_int(x=...)" by default.
    // hegel-rust's equivalent is the per-draw `let draw_N = ...;`
    // assignment line emitted during the final replay of the shrunk
    // failing case — the same information in a different format.
    let output = failing_project().cargo_run(&[]);
    assert!(
        output.stderr.contains("let draw_1 = "),
        "Expected 'let draw_1 = ' in stderr (default failing-example output):\n{}",
        output.stderr
    );
}

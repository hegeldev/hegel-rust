//! Ported from hypothesis-python/tests/cover/test_debug_information.py

use crate::common::project::TempRustProject;
use std::sync::OnceLock;

const DEBUG_FAILING_CODE: &str = r#"
use hegel::{Hegel, Settings, Verbosity};
use hegel::generators as gs;

fn main() {
    Hegel::new(|tc| {
        let i: i64 = tc.draw(gs::integers::<i64>());
        assert!(i < 10);
    })
    .settings(Settings::new()
        .verbosity(Verbosity::Debug)
        .test_cases(1000)
        .database(None))
    .run();
}
"#;

fn debug_failing_project() -> &'static TempRustProject {
    static PROJECT: OnceLock<TempRustProject> = OnceLock::new();
    PROJECT.get_or_init(|| {
        TempRustProject::new()
            .main_file(DEBUG_FAILING_CODE)
            .expect_failure("assertion failed")
    })
}

#[test]
fn test_reports_passes() {
    let output = debug_failing_project().cargo_run(&[]);
    let stderr = &output.stderr;

    #[cfg(feature = "native")]
    {
        assert!(
            stderr.contains("Shrinking:"),
            "Expected shrinking debug output in stderr:\n{}",
            stderr
        );
        assert!(
            stderr.contains("Shrinking complete:"),
            "Expected shrinking-complete debug output in stderr:\n{}",
            stderr
        );
    }
    #[cfg(not(feature = "native"))]
    {
        assert!(
            stderr.contains("Test done."),
            "Expected 'Test done.' in debug output:\n{}",
            stderr
        );
    }
}

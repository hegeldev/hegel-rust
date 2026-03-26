//! Tests for server error handling paths using HEGEL_PROTOCOL_TEST_MODE.
//!
//! Each test runs in a separate subprocess (via TempRustProject) because the
//! hegel server is a process-scoped static — HEGEL_PROTOCOL_TEST_MODE must
//! be set before the first Hegel test runs.

mod common;

use common::project::TempRustProject;

const SIMPLE_TEST: &str = r#"
fn main() {
    hegel::hegel(|tc| {
        let _: bool = tc.draw(hegel::generators::booleans());
    });
}
"#;

const SPAN_TEST: &str = r#"
use hegel::generators::Generator;

fn main() {
    hegel::hegel(|tc| {
        let _: String = tc.draw(
            hegel::generators::integers::<usize>()
                .min_value(1)
                .max_value(3)
                .flat_map(|n| hegel::generators::text().min_size(n).max_size(n)),
        );
    });
}
"#;

#[test]
fn test_stop_test_on_start_span_handled() {
    TempRustProject::new()
        .main_file(SPAN_TEST)
        .env("HEGEL_PROTOCOL_TEST_MODE", "stop_test_on_start_span")
        .cargo_run(&[]);
}

#[test]
fn test_health_check_failure_reported() {
    TempRustProject::new()
        .main_file(SIMPLE_TEST)
        .env("HEGEL_PROTOCOL_TEST_MODE", "health_check_failure")
        .expect_failure("Health check failure")
        .cargo_run(&[]);
}

#[test]
fn test_server_error_in_results_reported() {
    TempRustProject::new()
        .main_file(SIMPLE_TEST)
        .env("HEGEL_PROTOCOL_TEST_MODE", "server_error_in_results")
        .expect_failure("Server error")
        .cargo_run(&[]);
}

#[test]
fn test_flaky_replay_handled() {
    // FlakyReplay on generate — client should handle gracefully
    TempRustProject::new()
        .main_file(SIMPLE_TEST)
        .env("HEGEL_PROTOCOL_TEST_MODE", "flaky_replay")
        .cargo_run(&[]);
}

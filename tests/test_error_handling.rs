//! Tests for server error handling paths using HEGEL_PROTOCOL_TEST_MODE.
//!
//! Each test runs in a separate subprocess (via TempRustProject) because the
//! hegel server is a process-scoped static — HEGEL_PROTOCOL_TEST_MODE must
//! be set before the first Hegel test runs.

mod common;

use common::project::TempRustProject;

fn local_hegel_binary() -> String {
    let local = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), ".hegel/venv/bin/hegel");
    if std::path::Path::new(&local).exists() {
        local
    } else {
        // In CI, hegel is installed system-wide via pip
        String::from_utf8(
            std::process::Command::new("which")
                .arg("hegel")
                .output()
                .expect("hegel not found on PATH")
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string()
    }
}

fn error_test(mode: &str) -> TempRustProject {
    TempRustProject::new()
        .env("HEGEL_SERVER_COMMAND", &local_hegel_binary())
        .env("HEGEL_PROTOCOL_TEST_MODE", mode)
}

/// Check if hegel-core has the new test modes from hegeldev/hegel-core#68.
/// The hegel-core sibling project exists with the new modes when developing
/// locally; in CI, hegel-core is installed from the released version.
fn has_new_test_modes() -> bool {
    // Check for the sibling hegel-core checkout with the new test_server code
    let sibling = format!(
        "{}/../hegel-core/src/hegel/test_server.py",
        env!("CARGO_MANIFEST_DIR")
    );
    if let Ok(content) = std::fs::read_to_string(&sibling) {
        content.contains("failed_no_reason")
    } else {
        false
    }
}

macro_rules! requires_new_modes {
    () => {
        if !has_new_test_modes() {
            eprintln!(
                "Skipping: hegel-core test modes not available (need hegeldev/hegel-core#68)"
            );
            return;
        }
    };
}

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

const COLLECTION_TEST: &str = r#"
use hegel::generators::Generator;

fn main() {
    hegel::hegel(|tc| {
        // Use flat_map to force non-basic path which uses Collection protocol
        let _: Vec<String> = tc.draw(
            hegel::generators::vecs(
                hegel::generators::integers::<usize>()
                    .min_value(1)
                    .max_value(3)
                    .flat_map(|n| hegel::generators::text().min_size(n).max_size(n)),
            )
            .min_size(1)
            .max_size(3),
        );
    });
}
"#;

const POOL_ADD_TEST: &str = r#"
fn main() {
    hegel::hegel(|tc| {
        let mut pool = hegel::stateful::variables::<bool>(&tc);
        let val: bool = tc.draw(hegel::generators::booleans());
        pool.add(val);
    });
}
"#;

const NEW_POOL_TEST: &str = r#"
fn main() {
    hegel::hegel(|tc| {
        let _pool = hegel::stateful::variables::<i32>(&tc);
    });
}
"#;

const ANTITHESIS_TEST: &str = r#"
fn main() {
    // Set ANTITHESIS_OUTPUT_DIR to trigger the antithesis code path
    // but don't enable the antithesis feature — this should panic
    std::env::set_var("ANTITHESIS_OUTPUT_DIR", "/tmp");
    hegel::hegel(|tc| {
        let _: bool = tc.draw(hegel::generators::booleans());
    });
}
"#;

const PROTOCOL_DEBUG_TEST: &str = r#"
fn main() {
    hegel::hegel(|tc| {
        let _: bool = tc.draw(hegel::generators::booleans());
    });
}
"#;

// === StopTest modes (released in hegel-core) ===

#[test]
fn test_stop_test_on_new_collection() {
    error_test("stop_test_on_new_collection")
        .main_file(COLLECTION_TEST)
        .cargo_run(&[]);
}

#[test]
fn test_stop_test_on_collection_more() {
    error_test("stop_test_on_collection_more")
        .main_file(COLLECTION_TEST)
        .cargo_run(&[]);
}

// === StopTest modes (require hegeldev/hegel-core#68) ===

#[test]
fn test_stop_test_on_start_span() {
    requires_new_modes!();
    error_test("stop_test_on_start_span")
        .main_file(SPAN_TEST)
        .cargo_run(&[]);
}

#[test]
fn test_stop_test_on_pool_add() {
    requires_new_modes!();
    error_test("stop_test_on_pool_add")
        .main_file(POOL_ADD_TEST)
        .cargo_run(&[]);
}

#[test]
fn test_stop_test_on_new_pool() {
    requires_new_modes!();
    error_test("stop_test_on_new_pool")
        .main_file(NEW_POOL_TEST)
        .cargo_run(&[]);
}

// === Server error modes (require hegeldev/hegel-core#68) ===

#[test]
fn test_health_check_failure() {
    requires_new_modes!();
    error_test("health_check_failure")
        .main_file(SIMPLE_TEST)
        .expect_failure("Health check failure")
        .cargo_run(&[]);
}

#[test]
fn test_server_error_in_results() {
    requires_new_modes!();
    error_test("server_error_in_results")
        .main_file(SIMPLE_TEST)
        .expect_failure("Server error")
        .cargo_run(&[]);
}

#[test]
fn test_failed_no_reason() {
    requires_new_modes!();
    error_test("failed_no_reason")
        .main_file(SIMPLE_TEST)
        .expect_failure("Property test failed: unknown")
        .cargo_run(&[]);
}

#[test]
fn test_flaky_replay() {
    requires_new_modes!();
    error_test("flaky_replay")
        .main_file(SIMPLE_TEST)
        .cargo_run(&[]);
}

#[test]
fn test_server_crash() {
    requires_new_modes!();
    error_test("server_crash")
        .main_file(SIMPLE_TEST)
        .expect_failure("hegel server process exited")
        .cargo_run(&[]);
}

// === Communication error (non-StopTest error from server) ===

#[test]
fn test_error_response_causes_communication_error() {
    // error_response mode sends a RequestError on generate.
    // The error message doesn't match StopTest/FlakyReplay patterns,
    // so it should hit the CommunicationError panic path.
    error_test("error_response")
        .main_file(SIMPLE_TEST)
        .expect_failure("Property test failed")
        .cargo_run(&[]);
}

// === Antithesis paths ===

#[test]
fn test_antithesis_without_feature_panics() {
    error_test("empty_test")
        .main_file(ANTITHESIS_TEST)
        .expect_failure("antithesis")
        .cargo_run(&[]);
}

// === Antithesis emit on test failure ===

#[test]
fn test_antithesis_emit_on_failure() {
    let output_dir = tempfile::TempDir::new().unwrap();
    let code = r#"
#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let _: bool = tc.draw(hegel::generators::booleans());
    panic!("intentional-failure-for-antithesis");
}

fn main() {}
"#;
    TempRustProject::new()
        .test_file("test.rs", code)
        .main_file("fn main() {}")
        .feature("antithesis")
        .env("ANTITHESIS_OUTPUT_DIR", output_dir.path().to_str().unwrap())
        .expect_failure("Property test failed")
        .cargo_test(&[]);

    let jsonl_path = output_dir.path().join("sdk.jsonl");
    assert!(
        jsonl_path.exists(),
        "Antithesis JSONL should be written on test failure"
    );
}

// === Protocol debug ===

#[test]
fn test_protocol_debug_mode() {
    // Exercise the HEGEL_PROTOCOL_DEBUG=1 code path
    error_test("stop_test_on_generate")
        .main_file(PROTOCOL_DEBUG_TEST)
        .env("HEGEL_PROTOCOL_DEBUG", "1")
        .cargo_run(&[]);
}

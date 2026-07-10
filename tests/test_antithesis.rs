//! Antithesis integration behaviour, driven end-to-end by re-executing this
//! test binary (`exec::self_test`) with `ANTITHESIS_OUTPUT_DIR` set: the SDK
//! reads the variable at startup and writes `sdk.jsonl` into it, so a real
//! subprocess with a controlled environment is required.
//!
//! The feature-missing cases are compiled only without the `antithesis`
//! feature (they assert what happens when the feature is absent); the plain
//! `cargo test` CI job runs them. The `sdk.jsonl` content test is compiled
//! only with the feature.

#![cfg(not(windows))]

mod common;

use common::exec::self_test;
use hegel::generators as gs;
use tempfile::TempDir;

#[cfg(feature = "antithesis")]
#[hegel::test]
#[ignore = "fixture: run via exec::self_test"]
fn antithesis_jsonl_fixture(tc: hegel::TestCase) {
    let _ = tc.draw(gs::booleans());
}

/// The source line of the `#[hegel::test]` attribute on
/// `antithesis_jsonl_fixture`, which the SDK reports as the assertion
/// location's `begin_line`. Scanned from this file's own source so the
/// assertion doesn't break when the file is edited.
#[cfg(feature = "antithesis")]
fn jsonl_fixture_begin_line() -> u64 {
    let lines: Vec<&str> = include_str!("test_antithesis.rs").lines().collect();
    let fn_line = lines
        .iter()
        .position(|l| l.starts_with("fn antithesis_jsonl_fixture"))
        .expect("fixture fn not found in source")
        + 1;
    let attr_line = lines[..fn_line - 1]
        .iter()
        .rposition(|l| l.trim() == "#[hegel::test]")
        .expect("fixture #[hegel::test] attribute not found in source")
        + 1;
    attr_line as u64
}

#[cfg(feature = "antithesis")]
#[test]
fn test_antithesis_jsonl_written_when_env_set() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().to_str().unwrap().to_string();

    self_test("antithesis_jsonl_fixture")
        .env("ANTITHESIS_OUTPUT_DIR", &output_path)
        .run();

    let jsonl_path = output_dir.path().join("sdk.jsonl");
    assert!(jsonl_path.exists());

    let contents = std::fs::read_to_string(&jsonl_path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 2, "Got {} lines", lines.len());

    let declaration: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    let evaluation: serde_json::Value = serde_json::from_str(lines[1]).unwrap();

    let expected_id = "test_antithesis::antithesis_jsonl_fixture passes properties";
    let expected_location = serde_json::json!({
        "function": "antithesis_jsonl_fixture",
        "file": "tests/test_antithesis.rs",
        "class": "test_antithesis",
        "begin_line": jsonl_fixture_begin_line(),
        "begin_column": 0,
    });

    assert_eq!(
        declaration,
        serde_json::json!({
            "antithesis_assert": {
                "hit": false,
                "must_hit": true,
                "assert_type": "always",
                "display_type": "Always",
                "condition": false,
                "id": expected_id,
                "message": expected_id,
                "location": expected_location,
            }
        })
    );

    assert_eq!(
        evaluation,
        serde_json::json!({
            "antithesis_assert": {
                "hit": true,
                "must_hit": true,
                "assert_type": "always",
                "display_type": "Always",
                "condition": true,
                "id": expected_id,
                "message": expected_id,
                "location": expected_location,
            }
        })
    );
}

#[cfg(not(feature = "antithesis"))]
#[hegel::test]
#[ignore = "fixture: run via exec::self_test"]
fn antithesis_no_feature_fixture(tc: hegel::TestCase) {
    let _ = tc.draw(gs::booleans());
}

#[cfg(not(feature = "antithesis"))]
#[test]
fn test_antithesis_panics_without_feature() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().to_str().unwrap().to_string();

    self_test("antithesis_no_feature_fixture")
        .env("ANTITHESIS_OUTPUT_DIR", &output_path)
        .expect_failure("antithesis")
        .run();
}

#[cfg(not(feature = "antithesis"))]
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn antithesis_body_marker_fixture() {
    hegel::Hegel::new(|tc| {
        println!("BODY-RAN");
        let _: bool = tc.draw(gs::booleans());
    })
    .settings(hegel::Settings::new().database(None))
    .run();
}

/// Running under Antithesis without the `antithesis` feature is a
/// configuration error, and must fail *before* any test case runs — not
/// after a full (potentially long) property run has completed.
#[cfg(not(feature = "antithesis"))]
#[test]
fn test_missing_antithesis_feature_fails_before_running_any_test_case() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().to_str().unwrap().to_string();

    let output = self_test("antithesis_body_marker_fixture")
        .env("ANTITHESIS_OUTPUT_DIR", &output_path)
        .expect_failure("requires the `antithesis` feature")
        .run();
    assert!(
        !output.stdout.contains("BODY-RAN"),
        "the configuration error must fire before any test case runs, got:\n{}",
        output.stdout
    );
}

#[test]
#[ignore = "fixture: run via exec::self_test"]
fn antithesis_plain_run_fixture() {
    hegel::Hegel::new(|tc| {
        let _: bool = tc.draw(gs::booleans());
    })
    .settings(hegel::Settings::new().database(None))
    .run();
}

/// `ANTITHESIS_OUTPUT_DIR` pointing at a nonexistent path is a launch
/// configuration error, reported as a plain panic (the directory check runs
/// before — and regardless of — the feature check).
#[test]
fn test_nonexistent_antithesis_output_dir_panics() {
    self_test("antithesis_plain_run_fixture")
        .env("ANTITHESIS_OUTPUT_DIR", "/nonexistent/antithesis-output")
        .expect_failure("to exist when running inside of Antithesis")
        .run();
}

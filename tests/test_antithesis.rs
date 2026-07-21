//! Antithesis integration behaviour, driven end-to-end by re-executing this
//! test binary (`exec::self_test`) with `ANTITHESIS_OUTPUT_DIR` set: the
//! integration reads the variable at runtime and writes `sdk.jsonl` into it,
//! so a real subprocess with a controlled environment is required.

#![cfg(not(windows))]

mod common;

use common::exec::self_test;
use hegel::generators as gs;
use hegel::{HealthCheck, Mode, TestCase};
use std::path::Path;
use tempfile::TempDir;

/// Parse every line of `dir`'s `sdk.jsonl` as JSON. Panics on a missing file
/// or an unparseable line — every emitted line must be valid single-line JSON.
fn sdk_jsonl_lines(dir: &Path) -> Vec<serde_json::Value> {
    let contents = std::fs::read_to_string(dir.join("sdk.jsonl")).unwrap();
    contents
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[hegel::test]
#[ignore = "fixture: run via exec::self_test"]
fn antithesis_jsonl_fixture(tc: hegel::TestCase) {
    let _ = tc.draw(gs::booleans());
}

/// The source line of the `#[hegel::test]` attribute on
/// `antithesis_jsonl_fixture`, which the integration reports as the assertion
/// location's `begin_line`. Scanned from this file's own source so the
/// assertion doesn't break when the file is edited.
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

#[test]
fn test_antithesis_jsonl_written_when_env_set() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().to_str().unwrap().to_string();

    self_test("antithesis_jsonl_fixture")
        .env("ANTITHESIS_OUTPUT_DIR", &output_path)
        .run();

    let lines = sdk_jsonl_lines(output_dir.path());
    assert_eq!(lines.len(), 2, "Got {} lines", lines.len());

    let expected_id = "test_antithesis::antithesis_jsonl_fixture passes properties";
    let expected_location = serde_json::json!({
        "function": "antithesis_jsonl_fixture",
        "file": "tests/test_antithesis.rs",
        "class": "test_antithesis",
        "begin_line": jsonl_fixture_begin_line(),
        "begin_column": 0,
    });

    assert_eq!(
        lines[0],
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
        lines[1],
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

/// `ANTITHESIS_OUTPUT_DIR` pointing at a nonexistent path is a launch
/// configuration error, reported as a plain panic *before* any test case
/// runs — not after a full (potentially long) property run has completed.
#[test]
fn test_nonexistent_antithesis_output_dir_panics_before_any_test_case() {
    let output = self_test("antithesis_body_marker_fixture")
        .env("ANTITHESIS_OUTPUT_DIR", "/nonexistent/antithesis-output")
        .expect_failure("to exist when running inside of Antithesis")
        .run();
    assert!(
        !output.stdout.contains("BODY-RAN"),
        "the configuration error must fire before any test case runs, got:\n{}",
        output.stdout
    );
}

#[test]
#[ignore = "fixture: run via exec::self_test"]
fn antithesis_stateful_fixture() {
    struct Machine;

    #[hegel::state_machine]
    impl Machine {
        #[rule]
        fn noop(&mut self, _: TestCase) {}
    }

    hegel::Hegel::new(|tc| hegel::stateful::run(Machine, tc))
        .settings(hegel::Settings::new().database(None).test_cases(3))
        .run();
}

/// Every stateful rule draw is preceded by a `hegel_strategy_state` event, so
/// the moment right before the rule choice is distinguishable to Antithesis
/// as a strategy state. Steps count up from 1 within each test case.
#[test]
fn test_stateful_rule_draws_emit_strategy_state_events() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().to_str().unwrap().to_string();

    self_test("antithesis_stateful_fixture")
        .env("ANTITHESIS_OUTPUT_DIR", &output_path)
        .run();

    let lines = sdk_jsonl_lines(output_dir.path());
    assert!(!lines.is_empty());
    let steps: Vec<i64> = lines
        .iter()
        .map(|line| {
            line.get("hegel_strategy_state")
                .unwrap_or_else(|| panic!("unexpected sdk.jsonl line: {line}"))
                .get("step")
                .unwrap()
                .as_i64()
                .unwrap()
        })
        .collect();

    assert_eq!(steps[0], 1);
    let mut prev = 0;
    for step in steps {
        assert!(
            step == 1 || step == prev + 1,
            "steps must count up from 1 within each test case, got {step} after {prev}"
        );
        prev = step;
    }
}

#[test]
#[ignore = "fixture: run via exec::self_test"]
fn antithesis_single_invalid_fixture() {
    hegel::Hegel::new(|tc| {
        let _ = tc.draw(gs::booleans());
        tc.assume(false);
    })
    .settings(
        hegel::Settings::new()
            .mode(Mode::SingleTestCase)
            .database(None),
    )
    .run();
}

/// In `Mode::SingleTestCase`, a test case marked invalid (a failed
/// assumption) emits a soft terminate, telling Antithesis this branch is not
/// worth continuing.
#[test]
fn test_single_test_case_invalid_emits_soft_terminate() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().to_str().unwrap().to_string();

    self_test("antithesis_single_invalid_fixture")
        .env("ANTITHESIS_OUTPUT_DIR", &output_path)
        .run();

    let lines = sdk_jsonl_lines(output_dir.path());
    assert_eq!(
        lines,
        vec![serde_json::json!({
            "hegel_soft_terminate": {"reason": "test_case_invalid"}
        })]
    );
}

#[test]
#[ignore = "fixture: run via exec::self_test"]
fn antithesis_single_valid_fixture() {
    hegel::Hegel::new(|tc| {
        let _ = tc.draw(gs::booleans());
    })
    .settings(
        hegel::Settings::new()
            .mode(Mode::SingleTestCase)
            .database(None),
    )
    .run();
}

/// A valid single test case emits nothing: no soft terminate, and (without a
/// test location) no assertion either — the sdk.jsonl file is never created.
#[test]
fn test_single_test_case_valid_emits_no_soft_terminate() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().to_str().unwrap().to_string();

    self_test("antithesis_single_valid_fixture")
        .env("ANTITHESIS_OUTPUT_DIR", &output_path)
        .run();

    assert!(!output_dir.path().join("sdk.jsonl").exists());
}

#[test]
#[ignore = "fixture: run via exec::self_test"]
fn antithesis_test_run_with_rejections_fixture() {
    hegel::Hegel::new(|tc| {
        let b = tc.draw(gs::booleans());
        tc.assume(b);
    })
    .settings(
        hegel::Settings::new()
            .database(None)
            .test_cases(20)
            .suppress_health_check([HealthCheck::FilterTooMuch]),
    )
    .run();
}

/// Invalid test cases in an ordinary test run (`Mode::TestRun`) must *not*
/// emit soft terminates — rejection sampling is routine there, and the run
/// carries on generating. Soft terminate is a single-test-case behaviour.
#[test]
fn test_test_run_mode_invalid_cases_emit_no_soft_terminate() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().to_str().unwrap().to_string();

    self_test("antithesis_test_run_with_rejections_fixture")
        .env("ANTITHESIS_OUTPUT_DIR", &output_path)
        .run();

    assert!(!output_dir.path().join("sdk.jsonl").exists());
}

mod common;

use common::project::TempRustProject;
use tempfile::TempDir;

#[test]
fn test_antithesis_jsonl_written_when_env_set() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().to_str().unwrap().to_string();

    let code = r#"
use hegel::generators;

#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let _ = tc.draw(generators::booleans());
}
"#;

    let output = TempRustProject::new()
        .test_file(code)
        .feature("antithesis")
        .env("ANTITHESIS_OUTPUT_DIR", &output_path)
        .run();

    assert!(
        output.status.success(),
        "Subprocess failed: {}",
        output.stderr
    );

    let jsonl_path = output_dir.path().join("sdk.jsonl");
    assert!(jsonl_path.exists());

    let contents = std::fs::read_to_string(&jsonl_path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 2, "Got {} lines", lines.len());

    let declaration: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    let evaluation: serde_json::Value = serde_json::from_str(lines[1]).unwrap();

    let expected_id = "test::my_test passes properties";
    let expected_location = serde_json::json!({
        "function": "my_test",
        "file": "tests/test.rs",
        "class": "test",
        "begin_line": 4,
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
                "details": null
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
                "details": null
            }
        })
    );
}

#[test]
fn test_antithesis_failing_test_includes_details() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().to_str().unwrap().to_string();

    let code = r#"
use hegel::generators;

#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let _x: bool = tc.draw(generators::booleans());
    let _y: bool = tc.draw(generators::booleans());
    panic!("always fails");
}
"#;

    let output = TempRustProject::new()
        .test_file(code)
        .feature("antithesis")
        .env("ANTITHESIS_OUTPUT_DIR", &output_path)
        .run();

    assert!(
        !output.status.success(),
        "Expected test to fail but it passed"
    );

    let jsonl_path = output_dir.path().join("sdk.jsonl");
    assert!(jsonl_path.exists());

    let contents = std::fs::read_to_string(&jsonl_path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 2, "Got {} lines", lines.len());

    let evaluation: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    let details = &evaluation["antithesis_assert"]["details"];

    assert!(
        !details.is_null(),
        "Expected details to be present on failing test"
    );

    let failing_inputs = details["failing_inputs"].as_array().unwrap();
    assert_eq!(
        failing_inputs.len(),
        2,
        "Expected 2 failing inputs, got {:?}",
        failing_inputs
    );

    assert!(
        details["seed"].is_string(),
        "Expected seed to be a string, got {:?}",
        details["seed"]
    );
}

#[test]
fn test_antithesis_panics_without_feature() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().to_str().unwrap().to_string();

    let code = r#"
use hegel::generators;

#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let _ = tc.draw(generators::booleans());
}
"#;

    let output = TempRustProject::new()
        .test_file(code)
        .env("ANTITHESIS_OUTPUT_DIR", &output_path)
        .run();

    assert!(!output.status.success());
    assert!(
        output.stdout.contains("antithesis"),
        "\nstderr: {}\nstdout: {}",
        output.stderr,
        output.stdout
    );
}

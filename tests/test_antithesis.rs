// Antithesis only supports Linux.
#![cfg(not(windows))]
// Antithesis integration (JSONL output and the panic-without-feature guard) is currently
// only wired through the server backend path. These tests are gated until Antithesis
// support is added to the native backend.
#![cfg(not(feature = "native"))]
mod common;

use common::project::TempRustProject;
use tempfile::TempDir;

#[test]
fn test_antithesis_jsonl_written_when_env_set() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().to_str().unwrap().to_string();

    let code = r#"
use hegel::generators as gs;

#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let _ = tc.draw(gs::booleans());
}
"#;

    TempRustProject::new()
        .test_file("test.rs", code)
        .feature("antithesis")
        .env("ANTITHESIS_OUTPUT_DIR", &output_path)
        .cargo_test(&[]);

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

#[test]
fn test_antithesis_panics_without_feature() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().to_str().unwrap().to_string();

    let code = r#"
use hegel::generators as gs;

#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let _ = tc.draw(gs::booleans());
}
"#;

    TempRustProject::new()
        .test_file("test.rs", code)
        .env("ANTITHESIS_OUTPUT_DIR", &output_path)
        .expect_failure("antithesis")
        .cargo_test(&[]);
}

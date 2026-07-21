use super::*;

fn location() -> TestLocation {
    TestLocation {
        function: "my_test".to_string(),
        file: "tests/my_file.rs".to_string(),
        class: "my_module".to_string(),
        begin_line: 42,
    }
}

#[test]
fn check_antithesis_output_dir_accepts_an_existing_directory() {
    let dir = tempfile::TempDir::new().unwrap();
    check_antithesis_output_dir(dir.path().to_str().unwrap());
}

#[test]
fn check_antithesis_output_dir_panics_on_a_missing_directory() {
    let result =
        std::panic::catch_unwind(|| check_antithesis_output_dir("/nonexistent/antithesis-output"));
    let msg = result
        .unwrap_err()
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(
        msg.contains("to exist when running inside of Antithesis"),
        "{msg}"
    );
}

#[test]
fn is_running_in_antithesis_is_false_without_the_env_var() {
    assert!(!is_running_in_antithesis());
}

#[test]
fn validate_launch_configuration_is_a_no_op_outside_antithesis() {
    validate_launch_configuration();
}

#[test]
fn json_string_round_trips_through_a_json_parser() {
    for s in [
        "",
        "plain text",
        "quotes \" and backslashes \\",
        "newline\n tab\t carriage return\r",
        "low control chars \u{1}\u{1f}",
        "Ünïcödé ✓ 🦀",
    ] {
        let encoded = json_string(s);
        let parsed: String = serde_json::from_str(&encoded).unwrap();
        assert_eq!(parsed, s, "round-tripping {s:?} via {encoded}");
    }
}

#[test]
fn json_string_uses_standard_escapes() {
    assert_eq!(json_string("a\"b\\c\nd\re\tf"), r#""a\"b\\c\nd\re\tf""#);
    assert_eq!(json_string("\u{1}"), r#""\u0001""#);
}

#[test]
fn assertion_line_produces_the_documented_shape() {
    let expected_id = "my_module::my_test passes properties";
    let expected_location = serde_json::json!({
        "class": "my_module",
        "function": "my_test",
        "file": "tests/my_file.rs",
        "begin_line": 42,
        "begin_column": 0,
    });

    let declaration: serde_json::Value =
        serde_json::from_str(&assertion_line(&location(), false, false)).unwrap();
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

    let evaluation: serde_json::Value =
        serde_json::from_str(&assertion_line(&location(), true, true)).unwrap();
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
fn assertion_line_escapes_location_fields() {
    let loc = TestLocation {
        function: "weird\"name".to_string(),
        file: "C:\\windows\\path.rs".to_string(),
        class: "module".to_string(),
        begin_line: 1,
    };
    let parsed: serde_json::Value =
        serde_json::from_str(&assertion_line(&loc, true, true)).unwrap();
    let location = &parsed["antithesis_assert"]["location"];
    assert_eq!(location["function"], "weird\"name");
    assert_eq!(location["file"], "C:\\windows\\path.rs");
}

#[test]
fn strategy_state_line_produces_the_documented_shape() {
    let parsed: serde_json::Value = serde_json::from_str(&strategy_state_line(7)).unwrap();
    assert_eq!(
        parsed,
        serde_json::json!({"hegel_strategy_state": {"step": 7}})
    );
}

#[test]
fn soft_terminate_line_produces_the_documented_shape() {
    let parsed: serde_json::Value =
        serde_json::from_str(&soft_terminate_line("test_case_invalid")).unwrap();
    assert_eq!(
        parsed,
        serde_json::json!({"hegel_soft_terminate": {"reason": "test_case_invalid"}})
    );
}

#[test]
fn emit_lines_to_appends_to_sdk_jsonl() {
    let dir = tempfile::TempDir::new().unwrap();
    let output_dir = dir.path().to_str().unwrap().to_string();
    emit_lines_to(Some(output_dir.clone()), &["{\"first\":1}".to_string()]);
    emit_lines_to(
        Some(output_dir),
        &["{\"second\":2}".to_string(), "{\"third\":3}".to_string()],
    );

    let contents = std::fs::read_to_string(dir.path().join("sdk.jsonl")).unwrap();
    assert_eq!(contents, "{\"first\":1}\n{\"second\":2}\n{\"third\":3}\n");
}

#[test]
fn emit_lines_to_does_nothing_without_an_output_dir() {
    emit_lines_to(None, &["{}".to_string()]);
}

#[test]
fn append_sdk_jsonl_panics_when_the_directory_is_missing() {
    let result = std::panic::catch_unwind(|| {
        append_sdk_jsonl("/nonexistent/antithesis-output", &["{}".to_string()])
    });
    let msg = result
        .unwrap_err()
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(msg.contains("failed to open"), "{msg}");
}

#[test]
fn emitters_are_no_ops_outside_antithesis() {
    emit_assertion(&location(), true);
    emit_strategy_state(1);
    emit_soft_terminate("test_case_invalid");
}

use super::*;

#[test]
fn json_string_output_parses_back_to_the_original() {
    for s in [
        "plain",
        "with \"quotes\" and \\backslashes\\",
        "newline\ntab\tcarriage\rreturn",
        "control\u{1}char",
        "unicode: héllo ☃",
        "",
    ] {
        let encoded = json_string(s);
        let decoded: String = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, s, "{encoded}");
    }
}

#[test]
fn assertion_json_matches_the_antithesis_sdk_shape() {
    let location = TestLocation {
        function: "my_test".to_string(),
        file: "tests/my_test.rs".to_string(),
        class: "my_module".to_string(),
        begin_line: 42,
    };
    for (hit, condition) in [(false, false), (true, false), (true, true)] {
        let parsed: serde_json::Value =
            serde_json::from_str(&assertion_json(&location, hit, condition)).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!({
                "antithesis_assert": {
                    "hit": hit,
                    "must_hit": true,
                    "assert_type": "always",
                    "display_type": "Always",
                    "condition": condition,
                    "id": "my_module::my_test passes properties",
                    "message": "my_module::my_test passes properties",
                    "location": {
                        "class": "my_module",
                        "function": "my_test",
                        "file": "tests/my_test.rs",
                        "begin_line": 42,
                        "begin_column": 0,
                    }
                }
            })
        );
    }
}

#[test]
fn check_antithesis_output_dir_accepts_an_existing_directory() {
    let dir = tempfile::TempDir::new().unwrap();
    assert!(check_antithesis_output_dir(dir.path().to_str().unwrap()));
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

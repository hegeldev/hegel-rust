use super::*;
use alloc::string::ToString;
use alloc::vec::Vec;
use std::sync::{Arc, Mutex};

#[test]
fn existing_output_dir_is_accepted() {
    let dir = tempfile::TempDir::new().unwrap();
    check_antithesis_output_dir(dir.path().to_str().unwrap()).unwrap();
}

#[test]
fn missing_output_dir_is_a_usage_error() {
    let err =
        check_antithesis_output_dir("/no/such/antithesis/output/dir/for/hegel/tests").unwrap_err();
    assert!(matches!(err, crate::backend::RunError::UsageError(_)));
    let msg = err.to_string();
    assert!(msg.contains("ANTITHESIS_OUTPUT_DIR"), "got: {msg}");
}

/// A fake environment holding only `ANTITHESIS_OUTPUT_DIR`.
fn env_with_output_dir(dir: &str) -> impl Fn(&str) -> Option<String> {
    let dir = dir.to_string();
    move |key| (key == "ANTITHESIS_OUTPUT_DIR").then(|| dir.clone())
}

#[test]
fn not_in_antithesis_when_the_variable_is_unset() {
    check_environment_from(|_| None).unwrap();
    assert!(!antithesis_env_var_set_from(|_| None));
    assert!(antithesis_output_dir_from(|_| None).is_none());
}

#[test]
fn in_antithesis_when_the_variable_names_an_existing_directory() {
    let dir = tempfile::TempDir::new().unwrap();
    let env = env_with_output_dir(dir.path().to_str().unwrap());
    check_environment_from(&env).unwrap();
    assert_eq!(antithesis_env_var_set_from(&env), !cfg!(windows));
    assert_eq!(antithesis_output_dir_from(&env).is_some(), !cfg!(windows));
}

#[test]
fn other_variables_do_not_count() {
    let env = |key: &str| (key == "ANTITHESIS_OUTPUT").then(|| "/tmp".to_string());
    check_environment_from(env).unwrap();
    assert!(!antithesis_env_var_set_from(env));
}

#[cfg(not(windows))]
#[test]
fn a_missing_directory_is_a_usage_error_via_the_environment() {
    let env = env_with_output_dir("/no/such/antithesis/output/dir/for/hegel/tests");
    let err = check_environment_from(env).unwrap_err();
    assert!(matches!(err, crate::backend::RunError::UsageError(_)));
}

fn location() -> TestLocation {
    TestLocation {
        function: "my_property".to_string(),
        class: "my_crate::tests".to_string(),
        file: "tests/my_tests.rs".to_string(),
        begin_line: 42,
    }
}

/// The JSON object Antithesis's SDKs write for one assertion event.
fn expected_event(location: &TestLocation, hit: bool, condition: bool) -> serde_json::Value {
    let id = format!(
        "{}::{} passes properties",
        location.class, location.function
    );
    serde_json::json!({
        "antithesis_assert": {
            "hit": hit,
            "must_hit": true,
            "assert_type": "always",
            "display_type": "Always",
            "condition": condition,
            "id": id,
            "message": id,
            "location": {
                "class": location.class,
                "function": location.function,
                "file": location.file,
                "begin_line": location.begin_line,
                "begin_column": 0,
            },
        }
    })
}

fn parse_lines(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn assertion_lines_declare_then_evaluate_the_assertion() {
    for passed in [true, false] {
        let text = assertion_lines(&location(), passed);
        assert!(text.ends_with('\n'));
        assert_eq!(
            parse_lines(&text),
            [
                expected_event(&location(), false, false),
                expected_event(&location(), true, passed),
            ]
        );
    }
}

#[test]
fn assertion_lines_escape_the_location_strings() {
    let awkward = TestLocation {
        function: "quotes \" and \\ backslashes".to_string(),
        class: "tabs\tnewlines\nreturns\r".to_string(),
        file: "control \u{1} chars \u{1f} and unicode é 日本 \u{1F600}".to_string(),
        begin_line: u32::MAX,
    };
    assert_eq!(
        parse_lines(&assertion_lines(&awkward, true)),
        [
            expected_event(&awkward, false, false),
            expected_event(&awkward, true, true),
        ]
    );
}

#[test]
fn json_string_round_trips_every_ascii_character() {
    let all: String = (0u8..128).map(char::from).collect();
    let decoded: String = serde_json::from_str(&json_string(&all)).unwrap();
    assert_eq!(decoded, all);
}

/// An output whose lines are collected into the returned buffer.
fn capturing_output() -> (Output, Arc<Mutex<Vec<String>>>) {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&lines);
    let output = Output::callback(move |line: &str| sink.lock().unwrap().push(line.to_string()));
    (output, lines)
}

#[test]
fn nothing_is_reported_outside_antithesis() {
    let (output, lines) = capturing_output();
    report_with(|_| None, &location(), false, &output);
    assert!(lines.lock().unwrap().is_empty());
}

#[cfg(not(windows))]
#[test]
fn reports_append_to_the_sdk_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let env = env_with_output_dir(dir.path().to_str().unwrap());
    let (output, lines) = capturing_output();
    report_with(&env, &location(), true, &output);
    report_with(&env, &location(), false, &output);
    assert!(lines.lock().unwrap().is_empty());
    let text = std::fs::read_to_string(dir.path().join("sdk.jsonl")).unwrap();
    assert_eq!(
        parse_lines(&text),
        [
            expected_event(&location(), false, false),
            expected_event(&location(), true, true),
            expected_event(&location(), false, false),
            expected_event(&location(), true, false),
        ]
    );
}

#[cfg(not(windows))]
#[test]
fn an_unwritable_sdk_file_is_announced_on_the_output() {
    let env = env_with_output_dir("/no/such/antithesis/output/dir/for/hegel/tests");
    let (output, lines) = capturing_output();
    report_with(env, &location(), true, &output);
    let lines = lines.lock().unwrap();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("my_crate::tests::my_property passes properties")
            && lines[0].contains("/no/such/antithesis/output/dir/for/hegel/tests/sdk.jsonl"),
        "{}",
        lines[0]
    );
}

#[test]
fn a_reporter_outside_antithesis_is_silent() {
    let (output, lines) = capturing_output();
    Reporter::new(location(), output).report(false);
    assert!(lines.lock().unwrap().is_empty());
}

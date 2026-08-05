use super::*;

#[test]
fn default_is_new() {
    let d = Settings::default();
    let n = Settings::new();
    assert_eq!(d.test_cases, n.test_cases);
    assert_eq!(d.mode, n.mode);
}

#[test]
fn resolved_backend_picks_urandom_under_antithesis() {
    assert_eq!(
        Settings::new()
            .backend(Backend::Default)
            .resolved_backend(true),
        Backend::Default
    );
    assert_eq!(Settings::new().resolved_backend(true), Backend::Urandom);
    assert_eq!(Settings::new().resolved_backend(false), Backend::Default);
}

#[test]
fn suppress_health_check_replaces() {
    let s = Settings::new()
        .suppress_health_check([HealthCheck::TooSlow])
        .suppress_health_check([HealthCheck::FilterTooMuch]);
    assert_eq!(s.suppress_health_check, vec![HealthCheck::FilterTooMuch]);
    let s = s.suppress_health_check([]);
    assert_eq!(s.suppress_health_check, vec![]);
}

#[test]
fn settings_in_ci_disable_the_database_and_derandomize() {
    let settings = Settings::for_ci(true);
    assert!(matches!(settings.database, Database::Disabled));
    assert!(settings.derandomize);
}

#[test]
fn settings_outside_ci_leave_the_database_unset_and_randomized() {
    let settings = Settings::for_ci(false);
    assert!(matches!(settings.database, Database::Unset));
    assert!(!settings.derandomize);
}

#[test]
fn is_in_ci_from_is_false_when_no_variable_is_set() {
    assert!(!is_in_ci_from(|_| None));
}

#[test]
fn is_in_ci_from_detects_presence_variables_even_when_empty() {
    assert!(is_in_ci_from(|key| (key == "CI").then(String::new)));
    assert!(is_in_ci_from(|key| (key == "GITLAB_CI").then(String::new)));
}

#[test]
fn is_in_ci_from_requires_the_expected_value_for_value_variables() {
    assert!(is_in_ci_from(
        |key| (key == "TF_BUILD").then(|| "true".to_string())
    ));
    assert!(!is_in_ci_from(
        |key| (key == "TF_BUILD").then(|| "false".to_string())
    ));
    assert!(!is_in_ci_from(
        |key| (key == "GITHUB_ACTIONS").then(String::new)
    ));
}

#[test]
fn output_debug_names_the_destination() {
    assert_eq!(format!("{:?}", Output::stderr()), "Output(stderr)");
    assert_eq!(
        format!("{:?}", Output::callback(|_| {})),
        "Output(callback)"
    );
}

#[test]
fn output_line_routes_to_the_callback_when_set() {
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = std::sync::Arc::clone(&lines);
    let out = Output::callback(move |line| sink.lock().unwrap().push(line.to_string()));
    out.line("routed");
    assert_eq!(lines.lock().unwrap().as_slice(), ["routed".to_string()]);
    Output::stderr().line("this line goes to the test harness's stderr");
}

#[test]
fn settings_default_to_stderr_output_and_carry_a_configured_one() {
    assert_eq!(format!("{:?}", Settings::new().output), "Output(stderr)");
    let s = Settings::new().output(Output::callback(|_| {}));
    assert_eq!(format!("{:?}", s.output), "Output(callback)");
}

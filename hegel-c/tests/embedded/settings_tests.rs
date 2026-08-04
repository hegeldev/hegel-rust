use super::*;

static CI_ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
fn new_disables_database_in_ci() {
    let _guard = CI_ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let key = "TEAMCITY_VERSION";
    let had_key = std::env::var_os(key);
    unsafe {
        std::env::set_var(key, "1");
    }
    let settings = Settings::new();
    match had_key {
        Some(v) => unsafe { std::env::set_var(key, v) },
        None => unsafe { std::env::remove_var(key) },
    }
    assert!(matches!(settings.database, Database::Disabled));
    assert!(settings.derandomize);
}

#[test]
fn nondeterministic_defaults_off_and_is_settable() {
    let settings = Settings::new();
    assert!(!settings.nondeterministic);
    assert!(settings.nondeterministic(true).nondeterministic);
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

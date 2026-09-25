use super::*;
use alloc::format;
use alloc::string::ToString;

#[test]
fn default_is_new() {
    let d = Settings::default();
    let n = Settings::new();
    assert_eq!(d.test_cases, n.test_cases);
}

#[test]
fn backend_defaults_to_the_prng_and_is_settable() {
    for in_antithesis in [false, true] {
        assert_eq!(Settings::base(in_antithesis).backend, Backend::Default);
    }
    let s = Settings::new().backend(Backend::Urandom);
    assert_eq!(s.backend, Backend::Urandom);
    assert_eq!(s.backend(Backend::Default).backend, Backend::Default);
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
fn base_settings_are_environment_independent_except_for_antithesis() {
    let settings = Settings::base(false);
    assert!(matches!(settings.database, Database::Unset));
    assert!(!settings.derandomize);
    assert!(settings.print_blob);
    assert!(!settings.report_multiple_failures);
}

const ALL_HEALTH_CHECKS: [HealthCheck; 4] = [
    HealthCheck::FilterTooMuch,
    HealthCheck::TooSlow,
    HealthCheck::TestCasesTooLarge,
    HealthCheck::LargeInitialTestCase,
];

#[test]
fn health_checks_run_unless_suppressed_explicitly() {
    for in_antithesis in [false, true] {
        let settings = Settings::base(in_antithesis);
        for check in ALL_HEALTH_CHECKS {
            assert!(!settings.health_check_suppressed(check), "{check:?}");
        }
    }
    let settings = Settings::base(false);
    let settings = settings.suppress_health_check([HealthCheck::TooSlow]);
    assert!(settings.health_check_suppressed(HealthCheck::TooSlow));
    assert!(!settings.health_check_suppressed(HealthCheck::FilterTooMuch));
}

#[test]
fn print_blob_defaults_on_and_is_settable() {
    assert!(Settings::base(false).print_blob);
    assert!(!Settings::base(false).print_blob(false).print_blob);
}

fn only(key: &'static str, value: &'static str) -> impl Fn(&str) -> Option<String> {
    move |k| (k == key).then(|| value.to_string())
}

fn with_env(settings: Settings, key: &'static str, value: &'static str) -> Settings {
    settings.with_env_overrides_from(only(key, value)).unwrap()
}

fn env_error(key: &'static str, value: &'static str) -> String {
    Settings::base(false)
        .with_env_overrides_from(only(key, value))
        .unwrap_err()
}

#[test]
fn env_overrides_leave_settings_alone_when_nothing_is_set() {
    let s = Settings::base(false)
        .test_cases(5)
        .with_env_overrides_from(|_| None)
        .unwrap();
    assert_eq!(s.test_cases, 5);
}

#[test]
fn env_override_replaces_test_cases() {
    let s = with_env(
        Settings::base(false).test_cases(5),
        "HEGEL_TEST_CASES",
        "17",
    );
    assert_eq!(s.test_cases, 17);
}

#[test]
fn env_override_test_cases_empty_is_ignored() {
    let s = with_env(Settings::base(false).test_cases(5), "HEGEL_TEST_CASES", "");
    assert_eq!(s.test_cases, 5);
}

#[test]
fn env_override_test_cases_rejects_non_numeric_and_zero() {
    assert_eq!(
        env_error("HEGEL_TEST_CASES", "lots"),
        "HEGEL_TEST_CASES must be a positive integer, got \"lots\""
    );
    assert_eq!(
        env_error("HEGEL_TEST_CASES", "0"),
        "HEGEL_TEST_CASES must be a positive integer, got \"0\""
    );
}

#[test]
fn env_override_database_disabled_keyword_and_path() {
    let custom = || Settings::base(false).database(Some("custom".to_string()));
    assert_eq!(
        with_env(custom(), "HEGEL_DATABASE", "disabled").database,
        Database::Disabled
    );
    assert_eq!(
        with_env(
            Settings::base(false).database(None),
            "HEGEL_DATABASE",
            "my-db"
        )
        .database,
        Database::Path("my-db".to_string())
    );
    assert_eq!(
        with_env(custom(), "HEGEL_DATABASE", "").database,
        Database::Path("custom".to_string())
    );
}

#[test]
fn env_override_statistics_turns_reporting_on_unless_zero_or_empty() {
    assert!(with_env(Settings::base(false), "HEGEL_STATISTICS", "1").show_statistics);
    assert!(!with_env(Settings::base(false), "HEGEL_STATISTICS", "0").show_statistics);
    assert!(!with_env(Settings::base(false), "HEGEL_STATISTICS", "").show_statistics);
    assert!(
        with_env(
            Settings::base(false).show_statistics(true),
            "HEGEL_STATISTICS",
            "0"
        )
        .show_statistics,
        "0 does not turn the statistics off"
    );
}

#[test]
fn env_override_seed_replaces_clears_or_keeps_a_fixed_seed() {
    let seeded = || Settings::base(false).seed(Some(4242));
    assert_eq!(with_env(seeded(), "HEGEL_SEED", "7").seed, Some(7));
    assert_eq!(with_env(seeded(), "HEGEL_SEED", "none").seed, None);
    assert_eq!(with_env(seeded(), "HEGEL_SEED", "").seed, Some(4242));
    assert_eq!(
        env_error("HEGEL_SEED", "-1"),
        "HEGEL_SEED must be an integer or 'none', got \"-1\""
    );
}

#[test]
fn env_override_derandomize_accepts_the_boolean_vocabulary() {
    for value in ["true", "1", "yes"] {
        assert!(
            with_env(Settings::base(false), "HEGEL_DERANDOMIZE", value).derandomize,
            "{value:?}"
        );
    }
    for value in ["false", "0", "no"] {
        assert!(
            !with_env(
                Settings::base(false).derandomize(true),
                "HEGEL_DERANDOMIZE",
                value
            )
            .derandomize,
            "{value:?}"
        );
    }
    assert!(
        with_env(
            Settings::base(false).derandomize(true),
            "HEGEL_DERANDOMIZE",
            ""
        )
        .derandomize
    );
    assert_eq!(
        env_error("HEGEL_DERANDOMIZE", "maybe"),
        "HEGEL_DERANDOMIZE must be true or false, got \"maybe\""
    );
}

#[test]
fn env_override_print_blob_replaces_the_setting() {
    assert!(
        with_env(
            Settings::base(false).print_blob(false),
            "HEGEL_PRINT_BLOB",
            "true"
        )
        .print_blob
    );
    assert!(!with_env(Settings::base(false), "HEGEL_PRINT_BLOB", "false").print_blob);
    assert!(with_env(Settings::base(false), "HEGEL_PRINT_BLOB", "").print_blob);
    assert_eq!(
        env_error("HEGEL_PRINT_BLOB", "on"),
        "HEGEL_PRINT_BLOB must be true or false, got \"on\""
    );
}

#[test]
fn env_override_nondeterminism_strictness_accepts_quiet_warn_and_error() {
    for (value, strictness) in [
        ("quiet", NondeterminismStrictness::Quiet),
        ("warn", NondeterminismStrictness::Warn),
        ("error", NondeterminismStrictness::Error),
    ] {
        assert_eq!(
            with_env(
                Settings::base(false).nondeterminism_strictness(NondeterminismStrictness::Warn),
                "HEGEL_NONDETERMINISM_STRICTNESS",
                value
            )
            .nondeterminism_strictness,
            strictness,
            "{value:?}"
        );
    }
    assert_eq!(
        with_env(
            Settings::base(false).nondeterminism_strictness(NondeterminismStrictness::Error),
            "HEGEL_NONDETERMINISM_STRICTNESS",
            ""
        )
        .nondeterminism_strictness,
        NondeterminismStrictness::Error
    );
    assert_eq!(
        env_error("HEGEL_NONDETERMINISM_STRICTNESS", "strict"),
        "HEGEL_NONDETERMINISM_STRICTNESS must be quiet, warn or error, got \"strict\""
    );
}

#[test]
fn env_overrides_report_the_first_malformed_variable() {
    let env = |key: &str| match key {
        "HEGEL_TEST_CASES" => Some("17".to_string()),
        "HEGEL_SEED" => Some("x".to_string()),
        "HEGEL_PRINT_BLOB" => Some("on".to_string()),
        _ => None,
    };
    assert_eq!(
        Settings::base(false)
            .with_env_overrides_from(env)
            .unwrap_err(),
        "HEGEL_SEED must be an integer or 'none', got \"x\""
    );
}

#[test]
fn parse_bool_covers_the_shared_vocabulary() {
    for value in ["true", "1", "yes"] {
        assert_eq!(parse_bool(value), Some(true), "{value:?}");
    }
    for value in ["false", "0", "no"] {
        assert_eq!(parse_bool(value), Some(false), "{value:?}");
    }
    for value in ["", "TRUE", "on", "2"] {
        assert_eq!(parse_bool(value), None, "{value:?}");
    }
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

#[test]
fn nondeterminism_strictness_defaults_to_quiet_and_builds() {
    let s = Settings::new();
    assert_eq!(s.nondeterminism_strictness, NondeterminismStrictness::Quiet);
    assert!(!s.nd_force);
    let s = Settings::new().nondeterminism_strictness(NondeterminismStrictness::Error);
    assert_eq!(s.nondeterminism_strictness, NondeterminismStrictness::Error);
}

#[test]
fn choice_bound_defaults_to_buffer_size_and_can_be_removed() {
    let s = Settings::new();
    assert!(!s.unbounded_choices);
    assert_eq!(crate::native::core::BUFFER_SIZE, 1 << 20);
    assert_eq!(s.choice_bound(), crate::native::core::BUFFER_SIZE);
    let s = s.unbounded_choices(true);
    assert_eq!(s.choice_bound(), usize::MAX);
    let s = s.unbounded_choices(false);
    assert_eq!(s.choice_bound(), crate::native::core::BUFFER_SIZE);
}

#[test]
fn suppressing_test_cases_too_large_removes_the_choice_bound() {
    let s = Settings::new().suppress_health_check([HealthCheck::TestCasesTooLarge]);
    assert_eq!(s.choice_bound(), usize::MAX);
    assert_eq!(
        Settings::base(true).choice_bound(),
        crate::native::core::BUFFER_SIZE
    );
    let s = Settings::new().suppress_health_check([HealthCheck::TooSlow]);
    assert_eq!(s.choice_bound(), crate::native::core::BUFFER_SIZE);
}

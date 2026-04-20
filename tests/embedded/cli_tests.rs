use super::*;
use crate::runner::{Database, Settings, Verbosity};

fn s(strs: &[&str]) -> Vec<String> {
    std::iter::once("prog")
        .chain(strs.iter().copied())
        .map(String::from)
        .collect()
}

fn apply(args: &[&str]) -> Settings {
    try_apply_cli_args(Settings::new(), s(args)).unwrap_or_else(|e| match e {
        CliError::Help(_) => panic!("unexpected help"),
        CliError::Parse(msg) => panic!("parse error: {msg}"),
    })
}

#[test]
fn test_no_args_returns_default() {
    let defaults = Settings::new();
    let parsed = apply(&[]);
    assert_eq!(parsed.test_cases, defaults.test_cases);
    assert_eq!(parsed.verbosity, defaults.verbosity);
    assert_eq!(parsed.seed, defaults.seed);
}

#[test]
fn test_test_cases_override() {
    let parsed = apply(&["--test-cases", "500"]);
    assert_eq!(parsed.test_cases, 500);
}

#[test]
fn test_seed_override() {
    let parsed = apply(&["--seed", "42"]);
    assert_eq!(parsed.seed, Some(42));
}

#[test]
fn test_seed_none() {
    let parsed = apply(&["--seed", "none"]);
    assert_eq!(parsed.seed, None);
}

#[test]
fn test_verbosity_override() {
    let parsed = apply(&["--verbosity", "quiet"]);
    assert_eq!(parsed.verbosity, Verbosity::Quiet);

    let parsed = apply(&["--verbosity", "verbose"]);
    assert_eq!(parsed.verbosity, Verbosity::Verbose);

    let parsed = apply(&["--verbosity", "debug"]);
    assert_eq!(parsed.verbosity, Verbosity::Debug);

    let parsed = apply(&["--verbosity", "normal"]);
    assert_eq!(parsed.verbosity, Verbosity::Normal);
}

#[test]
fn test_derandomize_override() {
    let parsed = apply(&["--derandomize", "true"]);
    assert!(parsed.derandomize);
    let parsed = apply(&["--derandomize", "false"]);
    assert!(!parsed.derandomize);
}

#[test]
fn test_database_path() {
    let parsed = apply(&["--database", "/tmp/example"]);
    assert_eq!(parsed.database, Database::Path("/tmp/example".to_string()));
}

#[test]
fn test_database_disabled() {
    let parsed = apply(&["--database", "disabled"]);
    assert_eq!(parsed.database, Database::Disabled);
}

#[test]
fn test_suppress_health_check_single() {
    let parsed = apply(&["--suppress-health-check", "too_slow"]);
    assert_eq!(
        parsed.suppress_health_check,
        vec![crate::runner::HealthCheck::TooSlow]
    );
}

#[test]
fn test_suppress_health_check_multiple() {
    let parsed = apply(&["--suppress-health-check", "too_slow,filter_too_much"]);
    assert_eq!(
        parsed.suppress_health_check,
        vec![
            crate::runner::HealthCheck::TooSlow,
            crate::runner::HealthCheck::FilterTooMuch
        ]
    );
}

#[test]
fn test_suppress_health_check_all() {
    let parsed = apply(&["--suppress-health-check", "all"]);
    assert_eq!(parsed.suppress_health_check.len(), 4);
}

#[test]
fn test_multiple_flags() {
    let parsed = apply(&["--test-cases", "7", "--seed", "9", "--verbosity", "quiet"]);
    assert_eq!(parsed.test_cases, 7);
    assert_eq!(parsed.seed, Some(9));
    assert_eq!(parsed.verbosity, Verbosity::Quiet);
}

#[test]
fn test_unknown_arg_error() {
    let err = try_apply_cli_args(Settings::new(), s(&["--nope"])).unwrap_err();
    match err {
        CliError::Parse(msg) => assert!(msg.contains("Unknown argument")),
        _ => panic!("wrong error kind"),
    }
}

#[test]
fn test_missing_value_error() {
    let err = try_apply_cli_args(Settings::new(), s(&["--test-cases"])).unwrap_err();
    match err {
        CliError::Parse(msg) => assert!(msg.contains("requires a value")),
        _ => panic!("wrong error kind"),
    }
}

#[test]
fn test_invalid_value_error() {
    let err = try_apply_cli_args(Settings::new(), s(&["--test-cases", "abc"])).unwrap_err();
    match err {
        CliError::Parse(msg) => assert!(msg.contains("non-negative integer")),
        _ => panic!("wrong error kind"),
    }
}

#[test]
fn test_help_returns_help_error() {
    let err = try_apply_cli_args(Settings::new(), s(&["--help"])).unwrap_err();
    match err {
        CliError::Help(msg) => assert!(msg.contains("Usage:")),
        _ => panic!("wrong error kind"),
    }
}

#[test]
fn test_short_help_returns_help_error() {
    let err = try_apply_cli_args(Settings::new(), s(&["-h"])).unwrap_err();
    matches!(err, CliError::Help(_));
}

#[test]
fn test_default_preserved_when_not_overridden() {
    let parsed = try_apply_cli_args(Settings::new().test_cases(42), s(&[])).unwrap();
    assert_eq!(parsed.test_cases, 42);
}

#[test]
fn test_explicit_override_wins_over_default() {
    let parsed =
        try_apply_cli_args(Settings::new().test_cases(42), s(&["--test-cases", "10"])).unwrap();
    assert_eq!(parsed.test_cases, 10);
}

#[test]
fn test_invalid_verbosity_error() {
    let err = try_apply_cli_args(Settings::new(), s(&["--verbosity", "loud"])).unwrap_err();
    match err {
        CliError::Parse(msg) => assert!(msg.contains("quiet|normal|verbose|debug")),
        _ => panic!("wrong error kind"),
    }
}

#[test]
fn test_invalid_bool_error() {
    let err = try_apply_cli_args(Settings::new(), s(&["--derandomize", "maybe"])).unwrap_err();
    match err {
        CliError::Parse(msg) => assert!(msg.contains("true|false")),
        _ => panic!("wrong error kind"),
    }
}

#[test]
fn test_invalid_health_check_error() {
    let err = try_apply_cli_args(Settings::new(), s(&["--suppress-health-check", "bad_name"]))
        .unwrap_err();
    match err {
        CliError::Parse(msg) => assert!(msg.contains("does not recognise")),
        _ => panic!("wrong error kind"),
    }
}

#[test]
fn test_bool_aliases() {
    let parsed = apply(&["--derandomize", "1"]);
    assert!(parsed.derandomize);
    let parsed = apply(&["--derandomize", "yes"]);
    assert!(parsed.derandomize);
    let parsed = apply(&["--derandomize", "0"]);
    assert!(!parsed.derandomize);
    let parsed = apply(&["--derandomize", "no"]);
    assert!(!parsed.derandomize);
}

#[test]
fn test_invalid_seed_error() {
    let err = try_apply_cli_args(Settings::new(), s(&["--seed", "abc"])).unwrap_err();
    match err {
        CliError::Parse(msg) => assert!(msg.contains("integer or 'none'")),
        _ => panic!("wrong error kind"),
    }
}

#[test]
fn test_apply_cli_args_success() {
    match apply_cli_args(Settings::new(), s(&["--test-cases", "13"])) {
        CliOutcome::Success(settings) => assert_eq!(settings.test_cases, 13),
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn test_apply_cli_args_help() {
    match apply_cli_args(Settings::new(), s(&["--help"])) {
        CliOutcome::Help(msg) => assert!(msg.contains("Usage:")),
        other => panic!("expected Help, got {other:?}"),
    }
}

#[test]
fn test_apply_cli_args_parse_error() {
    match apply_cli_args(Settings::new(), s(&["--not-a-flag"])) {
        CliOutcome::ParseError(msg) => {
            assert!(msg.contains("Unknown argument"));
            assert!(msg.contains("Usage:"));
        }
        other => panic!("expected ParseError, got {other:?}"),
    }
}

mod common;

use common::exec::self_test;
use hegel::generators as gs;

#[test]
fn test_default_runs_100_test_cases() {
    let mut count = 0;

    hegel::hegel(|tc| {
        let _ = tc.draw(gs::integers::<i32>());
        count += 1;
    });

    assert_eq!(count, 100);
}

#[test]
fn test_settings_default_trait() {
    let settings = hegel::Settings::default();
    let mut count = 0;

    hegel::Hegel::new(|tc| {
        let _ = tc.draw(gs::integers::<i32>());
        count += 1;
    })
    .settings(settings)
    .run();

    assert_eq!(count, 100);
}

#[test]
fn test_settings_verbosity() {
    let mut count = 0;

    hegel::Hegel::new(|tc| {
        let _ = tc.draw(gs::integers::<i32>());
        count += 1;
    })
    .settings(
        hegel::Settings::new()
            .verbosity(hegel::Verbosity::Quiet)
            .test_cases(10),
    )
    .run();

    assert_eq!(count, 10);
}

/// Fixture for `test_hegel_test_cases_env_overrides_settings`, run via
/// self-exec with `HEGEL_TEST_CASES=7`: the environment variable must win
/// over the explicit `test_cases(100)` in the settings.
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn env_test_cases_fixture() {
    let mut count = 0;
    hegel::Hegel::new(|tc| {
        tc.draw(gs::integers::<i32>());
        count += 1;
    })
    .settings(hegel::Settings::new().test_cases(100).database(None))
    .run();
    assert_eq!(count, 7);
}

#[test]
fn test_hegel_test_cases_env_overrides_settings() {
    self_test("env_test_cases_fixture")
        .env("HEGEL_TEST_CASES", "7")
        .run();
}

fn run_failing_test_with_default_database(key: &str) {
    let result = std::panic::catch_unwind(|| {
        hegel::Hegel::new(|tc: hegel::TestCase| {
            tc.draw(gs::integers::<i32>());
            panic!("stored failure");
        })
        .__database_key(key.to_string())
        .settings(hegel::Settings::new().test_cases(1))
        .run();
    });
    assert!(result.is_err());
}

/// Fixture for `test_hegel_database_env_relocates_database`, run via
/// self-exec with `HEGEL_DATABASE=env-relocated-db`: the failing example
/// must be stored under that directory instead of the default `.hegel/`.
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn env_database_path_fixture() {
    run_failing_test_with_default_database("env_database_path_fixture");
    assert!(std::path::Path::new("env-relocated-db").is_dir());
}

#[test]
fn test_hegel_database_env_relocates_database() {
    self_test("env_database_path_fixture")
        .env("HEGEL_DATABASE", "env-relocated-db")
        .run();
}

/// Fixture for `test_hegel_database_env_disables_database`, run via
/// self-exec with `HEGEL_DATABASE=disabled`: no default `.hegel/` database
/// may be created even though the settings leave the database unset.
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn env_database_disabled_fixture() {
    run_failing_test_with_default_database("env_database_disabled_fixture");
    assert!(!std::path::Path::new(".hegel").exists());
}

#[test]
fn test_hegel_database_env_disables_database() {
    self_test("env_database_disabled_fixture")
        .env("HEGEL_DATABASE", "disabled")
        .run();
}

#[test]
fn test_settings_verbosity_debug() {
    let mut count = 0;

    hegel::Hegel::new(|tc| {
        let _ = tc.draw(gs::booleans());
        count += 1;
    })
    .settings(
        hegel::Settings::new()
            .verbosity(hegel::Verbosity::Debug)
            .test_cases(1),
    )
    .run();

    assert_eq!(count, 1);
}

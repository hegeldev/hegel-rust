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

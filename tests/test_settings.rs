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

/// Fixture for `test_hegel_seed_env_overrides_a_fixed_seed`, run via
/// self-exec: prints the sequence a compiled-in seed of 4242 draws, so the
/// parent can tell whether `HEGEL_SEED` replaced that seed.
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn env_seed_fixture() {
    hegel::Hegel::new(|tc| {
        println!("DRAW:{}", tc.draw(gs::integers::<u64>()));
    })
    .settings(
        hegel::Settings::new()
            .seed(Some(4242))
            .derandomize(false)
            .test_cases(32)
            .database(None),
    )
    .run();
}

fn seed_fixture_draws(seed: Option<&str>) -> Vec<String> {
    let mut cmd = self_test("env_seed_fixture").env_remove("HEGEL_SEED");
    if let Some(seed) = seed {
        cmd = cmd.env("HEGEL_SEED", seed);
    }
    cmd.run()
        .stdout
        .lines()
        .filter(|line| line.starts_with("DRAW:"))
        .map(str::to_string)
        .collect()
}

#[test]
fn test_hegel_seed_env_overrides_a_fixed_seed() {
    let compiled_in = seed_fixture_draws(None);
    let seven = seed_fixture_draws(Some("7"));
    let seven_again = seed_fixture_draws(Some("7"));
    let nine = seed_fixture_draws(Some("9"));
    assert_eq!(seven.len(), 32);
    assert_eq!(seven, seven_again);
    assert_ne!(seven, compiled_in);
    assert_ne!(seven, nine);
}

/// Fixture for `test_hegel_print_blob_env_overrides_an_explicit_setting`,
/// run via self-exec: a failing property whose settings turn `print_blob`
/// off, so a reproducer line in its output can only come from
/// `HEGEL_PRINT_BLOB=true`.
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn env_print_blob_fixture() {
    hegel::Hegel::new(|tc: hegel::TestCase| {
        let n = tc.draw(gs::integers::<u8>().min_value(1));
        assert_eq!(n, 0);
    })
    .settings(
        hegel::Settings::new()
            .test_cases(1)
            .derandomize(true)
            .print_blob(false)
            .database(None),
    )
    .run();
}

#[test]
fn test_hegel_print_blob_env_overrides_an_explicit_setting() {
    let without = self_test("env_print_blob_fixture")
        .env_remove("HEGEL_PRINT_BLOB")
        .expect_failure("assertion `left == right` failed")
        .run();
    assert!(
        !without.stderr.contains("reproduce_failure("),
        "unexpected reproducer line:\n{}",
        without.stderr
    );
    self_test("env_print_blob_fixture")
        .env("HEGEL_PRINT_BLOB", "true")
        .expect_failure(r#"#\[hegel::reproduce_failure\("[A-Za-z0-9+/=_-]+"\)\]"#)
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

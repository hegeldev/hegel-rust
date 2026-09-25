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

/// Fixture for `test_hegel_test_cases_env_sets_the_starting_value`, run
/// via self-exec with `HEGEL_TEST_CASES=7`: settings that leave
/// `test_cases` to the profile pick the variable's value up.
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn env_test_cases_fixture() {
    let mut count = 0;
    hegel::Hegel::new(|tc| {
        tc.draw(gs::integers::<i32>());
        count += 1;
    })
    .settings(hegel::Settings::new().database(None))
    .run();
    assert_eq!(count, 7);
}

#[test]
fn test_hegel_test_cases_env_sets_the_starting_value() {
    self_test("env_test_cases_fixture")
        .env("HEGEL_TEST_CASES", "7")
        .run();
}

/// Fixture for `test_hegel_test_cases_compiled_in_beats_the_env`, run via
/// self-exec with `HEGEL_TEST_CASES=7`: an explicit `test_cases(100)` is
/// compiled in and keeps its value.
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn env_test_cases_compiled_in_fixture() {
    let mut count = 0;
    hegel::Hegel::new(|tc| {
        tc.draw(gs::integers::<i32>());
        count += 1;
    })
    .settings(hegel::Settings::new().test_cases(100).database(None))
    .run();
    assert_eq!(count, 100);
}

#[test]
fn test_hegel_test_cases_compiled_in_beats_the_env() {
    self_test("env_test_cases_compiled_in_fixture")
        .env("HEGEL_TEST_CASES", "7")
        .run();
}

/// Fixture for `test_hegel_malformed_env_var_fails_settings_creation`, run
/// via self-exec with `HEGEL_TEST_CASES=lots`: creating the settings fails
/// with the engine's message naming the variable.
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn env_malformed_fixture() {
    hegel::Settings::new();
}

#[test]
fn test_hegel_malformed_env_var_fails_settings_creation() {
    self_test("env_malformed_fixture")
        .env("HEGEL_TEST_CASES", "lots")
        .expect_failure(r#"HEGEL_TEST_CASES must be a positive integer, got "lots""#)
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

fn print_draws(settings: hegel::Settings) {
    hegel::Hegel::new(|tc| {
        println!("DRAW:{}", tc.draw(gs::integers::<u64>()));
    })
    .settings(settings.test_cases(32).database(None))
    .run();
}

/// Fixture for `test_hegel_seed_env_replaces_the_derived_seed`, run via
/// self-exec: prints the sequence a derandomized test with no fixed seed
/// draws, so the parent can tell whether `HEGEL_SEED` replaced the seed.
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn env_seed_fixture() {
    print_draws(hegel::Settings::new().derandomize(true));
}

/// Fixture for `test_hegel_seed_compiled_in_beats_the_env`, run via
/// self-exec: prints the sequence a compiled-in seed of 4242 draws, which
/// `HEGEL_SEED` must leave alone.
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn env_seed_compiled_in_fixture() {
    print_draws(hegel::Settings::new().seed(Some(4242)).derandomize(false));
}

fn seed_fixture_draws(fixture: &str, seed: Option<&str>) -> Vec<String> {
    let mut cmd = self_test(fixture).env_remove("HEGEL_SEED");
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
fn test_hegel_seed_env_replaces_the_derived_seed() {
    let derived = seed_fixture_draws("env_seed_fixture", None);
    let seven = seed_fixture_draws("env_seed_fixture", Some("7"));
    let seven_again = seed_fixture_draws("env_seed_fixture", Some("7"));
    let nine = seed_fixture_draws("env_seed_fixture", Some("9"));
    assert_eq!(seven.len(), 32);
    assert_eq!(seven, seven_again);
    assert_ne!(seven, derived);
    assert_ne!(seven, nine);
}

#[test]
fn test_hegel_seed_compiled_in_beats_the_env() {
    let compiled_in = seed_fixture_draws("env_seed_compiled_in_fixture", None);
    let seven = seed_fixture_draws("env_seed_compiled_in_fixture", Some("7"));
    assert_eq!(compiled_in.len(), 32);
    assert_eq!(seven, compiled_in);
}

fn failing_property_with(settings: hegel::Settings) {
    hegel::Hegel::new(|tc: hegel::TestCase| {
        let n = tc.draw(gs::integers::<u8>().min_value(1));
        assert_eq!(n, 0);
    })
    .settings(settings.test_cases(1).derandomize(true).database(None))
    .run();
}

/// Fixture for `test_hegel_print_blob_env_turns_the_reproducer_off`, run
/// via self-exec: a failing property whose settings leave `print_blob` to
/// the profile, where it is on, so `HEGEL_PRINT_BLOB=false` is the only
/// thing that can remove the reproducer line from its output.
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn env_print_blob_fixture() {
    failing_property_with(hegel::Settings::new());
}

/// Fixture for `test_hegel_print_blob_compiled_in_beats_the_env`, run via
/// self-exec: the same failing property with `print_blob` turned off in
/// its settings, which `HEGEL_PRINT_BLOB=true` must not turn back on.
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn env_print_blob_compiled_in_fixture() {
    failing_property_with(hegel::Settings::new().print_blob(false));
}

const REPRODUCER_LINE: &str = r#"#\[hegel::reproduce_failure\("[A-Za-z0-9+/=_-]+"\)\]"#;

fn assert_no_reproducer_line(output: &common::exec::RunOutput) {
    assert!(
        !output.stderr.contains("reproduce_failure("),
        "unexpected reproducer line:\n{}",
        output.stderr
    );
}

#[test]
fn test_hegel_print_blob_env_turns_the_reproducer_off() {
    self_test("env_print_blob_fixture")
        .env_remove("HEGEL_PRINT_BLOB")
        .expect_failure(REPRODUCER_LINE)
        .run();
    let without = self_test("env_print_blob_fixture")
        .env("HEGEL_PRINT_BLOB", "false")
        .expect_failure("assertion `left == right` failed")
        .run();
    assert_no_reproducer_line(&without);
}

#[test]
fn test_hegel_print_blob_compiled_in_beats_the_env() {
    let with = self_test("env_print_blob_compiled_in_fixture")
        .env("HEGEL_PRINT_BLOB", "true")
        .expect_failure("assertion `left == right` failed")
        .run();
    assert_no_reproducer_line(&with);
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

use super::*;
use crate::runner::Phase;

// Serialize the three tests below that mutate process-global CI env
// vars.  Without a lock, `cargo test`'s parallelism can interleave one
// test's "set TEAMCITY_VERSION" with another test's "remove
// TEAMCITY_VERSION", and `Settings::new()`'s CI detection sees the
// wrong state.
static CI_ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_settings_verbosity() {
    let _ = Settings::new().verbosity(Verbosity::Debug);
}

#[test]
fn test_settings_phases() {
    let s = Settings::new().phases([Phase::Explicit, Phase::Generate]);
    assert_eq!(s.phases, vec![Phase::Explicit, Phase::Generate]);
}

#[test]
fn test_settings_report_multiple_failures_default_true() {
    let s = Settings::new();
    assert!(s.report_multiple_failures);
}

#[test]
fn test_settings_report_multiple_failures_setter() {
    let s = Settings::new().report_multiple_failures(false);
    assert!(!s.report_multiple_failures);
    let s = s.report_multiple_failures(true);
    assert!(s.report_multiple_failures);
}

#[test]
fn test_settings_has_phase() {
    let s = Settings::new().phases([Phase::Generate, Phase::Shrink]);
    assert!(s.has_phase(Phase::Generate));
    assert!(s.has_phase(Phase::Shrink));
    assert!(!s.has_phase(Phase::Reuse));
    assert!(!s.has_phase(Phase::Explicit));
}

#[test]
fn test_is_in_ci_some_expected_variant() {
    // Removing "CI" (a None-type entry) forces the iterator to continue and
    // evaluate the Some("true") entries such as TF_BUILD and GITHUB_ACTIONS,
    // exercising the `Some(expected)` match arm in is_in_ci().
    let _guard = CI_ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ci = std::env::var_os("CI");
    unsafe {
        std::env::remove_var("CI");
        std::env::set_var("TF_BUILD", "true");
    }
    let result = is_in_ci();
    unsafe {
        std::env::remove_var("TF_BUILD");
        if let Some(val) = ci {
            std::env::set_var("CI", val);
        }
    }
    assert!(
        result,
        "TF_BUILD=true should be detected as a CI environment"
    );
}

// On CI, every test runs under `is_in_ci() == true`, so the
// `Database::Unset` arm of `Settings::new` (and of the native engine's
// `run_main` match in `src/native/test_runner.rs`) is otherwise
// dead from a coverage perspective.  This test temporarily clears
// the CI env vars and runs the engine through that arm.
#[cfg(feature = "native")]
#[test]
fn test_native_engine_creates_default_dot_hegel_when_database_unset() {
    use crate::Hegel;
    use crate::generators as gs;
    use crate::runner::Database;

    let _guard = CI_ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    const CI_VAR_NAMES: &[&str] = &[
        "CI",
        "TF_BUILD",
        "BUILDKITE",
        "CIRCLECI",
        "CIRRUS_CI",
        "CODEBUILD_BUILD_ID",
        "GITHUB_ACTIONS",
        "GITLAB_CI",
        "HEROKU_TEST_RUN_ID",
        "TEAMCITY_VERSION",
        "bamboo.buildKey",
    ];
    let saved: Vec<(&str, Option<std::ffi::OsString>)> = CI_VAR_NAMES
        .iter()
        .map(|name| (*name, std::env::var_os(name)))
        .collect();
    unsafe {
        for (name, _) in &saved {
            std::env::remove_var(name);
        }
    }
    // Run in a fresh tempdir so we don't pollute cwd.
    let tmp = tempfile::TempDir::new().unwrap();
    let prev_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Settings::new() now defaults to Database::Unset.
    let settings = Settings::new();
    assert_eq!(settings.database, Database::Unset);
    Hegel::new(|tc| {
        let _ = tc.draw(gs::booleans());
    })
    .settings(settings.test_cases(1))
    .run();

    std::env::set_current_dir(&prev_cwd).unwrap();
    unsafe {
        for (name, val) in saved {
            if let Some(v) = val {
                std::env::set_var(name, v);
            }
        }
    }
}

#[test]
fn test_settings_new_in_ci_disables_database() {
    // Temporarily set a CI env var so is_in_ci() returns true.
    // Using TEAMCITY_VERSION (checked with None, i.e. any value suffices).
    let _guard = CI_ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let key = "TEAMCITY_VERSION";
    let had_key = std::env::var_os(key).is_some();
    unsafe {
        std::env::set_var(key, "1");
    }
    let settings = Settings::new();
    if !had_key {
        unsafe {
            std::env::remove_var(key);
        }
    }
    assert_eq!(settings.database, Database::Disabled);
    assert!(settings.derandomize);
}

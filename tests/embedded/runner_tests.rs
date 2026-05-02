use super::*;
use crate::runner::Phase;

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

#[test]
fn test_settings_new_in_ci_disables_database() {
    // Temporarily set a CI env var so is_in_ci() returns true.
    // Using TEAMCITY_VERSION (checked with None, i.e. any value suffices).
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

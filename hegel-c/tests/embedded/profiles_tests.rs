use super::*;
use alloc::borrow::ToOwned;
use alloc::string::ToString;
use alloc::vec;

fn no_config() -> ConfigFile {
    ConfigFile::default()
}

fn config_of(text: &str) -> ConfigFile {
    crate::config::parse(text).unwrap()
}

fn env_of(
    pairs: &'static [(&'static str, &'static str)],
) -> impl Fn(&str) -> Option<String> + Copy {
    move |key| {
        pairs
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| (*v).to_string())
    }
}

fn resolve_named(name: &str, config: &ConfigFile) -> Settings {
    resolve(name, config, &[], &Settings::base(false)).unwrap()
}

#[test]
fn the_default_profile_is_the_base_defaults() {
    let s = resolve_named("default", &no_config());
    assert_eq!(s.test_cases, 100);
    assert_eq!(s.verbosity, Verbosity::Normal);
    assert_eq!(s.seed, None);
    assert!(!s.derandomize);
    assert_eq!(s.database, Database::Unset);
    assert!(s.suppress_health_check.is_empty());
    assert_eq!(s.phases.len(), 5);
    assert!(!s.report_multiple_failures);
    assert!(!s.show_statistics);
    assert!(!s.print_blob);
    assert_eq!(s.backend, None);
}

#[test]
fn the_ci_profile_derandomizes_disables_the_database_and_prints_blobs() {
    let s = resolve_named("ci", &no_config());
    assert!(s.derandomize);
    assert_eq!(s.database, Database::Disabled);
    assert!(s.print_blob);
    assert_eq!(s.test_cases, 100);
    assert!(s.suppress_health_check.is_empty());
}

#[test]
fn the_antithesis_profile_disables_only_the_database() {
    let s = resolve_named("antithesis", &no_config());
    assert_eq!(s.database, Database::Disabled);
    assert!(!s.derandomize, "Antithesis controls randomness itself");
    assert!(!s.print_blob);
}

const ALL_HEALTH_CHECKS: [HealthCheck; 4] = [
    HealthCheck::FilterTooMuch,
    HealthCheck::TooSlow,
    HealthCheck::TestCasesTooLarge,
    HealthCheck::LargeInitialTestCase,
];

#[test]
fn no_profile_overrides_antithesis_health_check_forcing() {
    for name in ["default", "ci", "antithesis"] {
        let s = resolve(name, &no_config(), &[], &Settings::base(true)).unwrap();
        for check in ALL_HEALTH_CHECKS {
            assert!(s.health_check_suppressed(check), "{name}: {check:?}");
        }
    }
    let config = config_of("[profiles.antithesis]\nsuppress_health_check = []\n");
    let s = resolve("antithesis", &config, &[], &Settings::base(true)).unwrap();
    for check in ALL_HEALTH_CHECKS {
        assert!(s.health_check_suppressed(check), "{check:?}");
    }
}

#[test]
fn health_checks_run_outside_antithesis_unless_suppressed() {
    let s = resolve_named("default", &no_config());
    for check in ALL_HEALTH_CHECKS {
        assert!(!s.health_check_suppressed(check), "{check:?}");
    }
    let config = config_of("[profiles.default]\nsuppress_health_check = [\"too_slow\"]\n");
    let s = resolve_named("default", &config);
    assert!(s.health_check_suppressed(HealthCheck::TooSlow));
    assert!(!s.health_check_suppressed(HealthCheck::FilterTooMuch));
}

#[test]
fn config_deltas_merge_onto_shipped_profiles() {
    let config = config_of("[profiles.ci]\ntest_cases = 7\nderandomize = false\n");
    let s = resolve_named("ci", &config);
    assert_eq!(s.test_cases, 7);
    assert!(!s.derandomize, "the user delta wins over the shipped delta");
    assert_eq!(
        s.database,
        Database::Disabled,
        "unset fields keep the shipped value"
    );
    assert!(s.print_blob);
}

#[test]
fn config_changes_to_default_flow_through_to_shipped_profiles() {
    let config = config_of("[profiles.default]\ntest_cases = 200\nderandomize = false\n");
    let s = resolve_named("ci", &config);
    assert_eq!(s.test_cases, 200);
    assert!(s.derandomize, "ci's own delta beats the inherited default");
}

#[test]
fn config_profiles_extend_default_implicitly() {
    let config = config_of(
        "[profiles.default]\ntest_cases = 200\n[profiles.nightly]\nshow_statistics = true\n",
    );
    let s = resolve_named("nightly", &config);
    assert_eq!(s.test_cases, 200);
    assert!(s.show_statistics);
}

#[test]
fn config_profiles_extend_a_named_parent() {
    let config = config_of(
        "[profiles.ci]\ntest_cases = 1000\n[profiles.nightly]\nextends = \"ci\"\ntest_cases = 10000\n",
    );
    let s = resolve_named("nightly", &config);
    assert_eq!(s.test_cases, 10000);
    assert!(s.derandomize, "inherited from shipped ci");
    assert!(s.print_blob, "inherited from shipped ci");
    let s = resolve_named("ci", &config);
    assert_eq!(s.test_cases, 1000);
}

#[test]
fn long_extends_chains_resolve() {
    let config = config_of(
        "[profiles.a]\ntest_cases = 1\n\
         [profiles.b]\nextends = \"a\"\nseed = 3\n\
         [profiles.c]\nextends = \"b\"\nshow_statistics = true\n",
    );
    let s = resolve_named("c", &config);
    assert_eq!(s.test_cases, 1);
    assert_eq!(s.seed, Some(3));
    assert!(s.show_statistics);
}

#[test]
fn backend_auto_clears_an_inherited_choice() {
    let config =
        config_of("[profiles.default]\nbackend = \"urandom\"\n[profiles.x]\nbackend = \"auto\"\n");
    assert_eq!(
        resolve_named("default", &config).backend,
        Some(Backend::Urandom)
    );
    assert_eq!(resolve_named("x", &config).backend, None);
}

#[test]
fn self_cycles_are_reported() {
    let config = config_of("[profiles.a]\nextends = \"a\"\n");
    assert_eq!(
        resolve("a", &config, &[], &Settings::base(false)).unwrap_err(),
        ProfileError::ExtendsCycle(vec!["a".to_owned(), "a".to_owned()])
    );
}

#[test]
fn two_step_cycles_are_reported_with_the_walk() {
    let config = config_of("[profiles.a]\nextends = \"b\"\n[profiles.b]\nextends = \"a\"\n");
    let e = resolve("a", &config, &[], &Settings::base(false)).unwrap_err();
    assert_eq!(
        e,
        ProfileError::ExtendsCycle(vec!["a".to_owned(), "b".to_owned(), "a".to_owned()])
    );
    assert_eq!(e.to_string(), "profile extends cycle: a -> b -> a");
}

#[test]
fn unknown_profiles_are_reported() {
    let e = resolve("nope", &no_config(), &[], &Settings::base(false)).unwrap_err();
    assert_eq!(e, ProfileError::UnknownProfile("nope".to_owned()));
    assert_eq!(e.to_string(), "unknown settings profile \"nope\"");
}

#[test]
fn unknown_extends_name_the_referring_profile() {
    let config = config_of("[profiles.a]\nextends = \"ghost\"\n");
    let e = resolve("a", &config, &[], &Settings::base(false)).unwrap_err();
    assert_eq!(
        e,
        ProfileError::UnknownExtends {
            profile: "a".to_owned(),
            extends: "ghost".to_owned(),
        }
    );
    assert_eq!(
        e.to_string(),
        "profile \"a\" extends unknown profile \"ghost\""
    );
}

#[test]
fn extends_is_rejected_on_shipped_profiles() {
    let config = config_of("[profiles.ci]\nextends = \"default\"\n");
    let e = resolve("ci", &config, &[], &Settings::base(false)).unwrap_err();
    assert_eq!(
        e,
        ProfileError::ExtendsNotAllowed {
            profile: "ci".to_owned(),
            kind: "shipped",
        }
    );
    assert_eq!(
        e.to_string(),
        "cannot set extends on shipped profile \"ci\""
    );
}

#[test]
fn extends_is_rejected_on_registered_profiles() {
    let registry = vec![(
        "mine".to_owned(),
        ProfileDelta::snapshot(&Settings::base(false)),
    )];
    let config = config_of("[profiles.mine]\nextends = \"ci\"\n");
    let e = resolve("mine", &config, &registry, &Settings::base(false)).unwrap_err();
    assert_eq!(
        e,
        ProfileError::ExtendsNotAllowed {
            profile: "mine".to_owned(),
            kind: "registered",
        }
    );
}

#[test]
fn registered_profiles_resolve_to_their_snapshot() {
    let snapshot = Settings::base(false).test_cases(5).derandomize(true);
    let registry = vec![("mine".to_owned(), ProfileDelta::snapshot(&snapshot))];
    let s = resolve("mine", &no_config(), &registry, &Settings::base(false)).unwrap();
    assert_eq!(s.test_cases, 5);
    assert!(s.derandomize);
}

#[test]
fn config_deltas_merge_onto_registered_profiles() {
    let snapshot = Settings::base(false).test_cases(5);
    let registry = vec![("mine".to_owned(), ProfileDelta::snapshot(&snapshot))];
    let config = config_of("[profiles.mine]\ntest_cases = 9\n");
    let s = resolve("mine", &config, &registry, &Settings::base(false)).unwrap();
    assert_eq!(s.test_cases, 9);
}

#[test]
fn registered_profiles_replace_shipped_ones_as_the_base() {
    let registry = vec![(
        "default".to_owned(),
        ProfileDelta::snapshot(&Settings::base(false).test_cases(3)),
    )];
    let s = resolve("ci", &no_config(), &registry, &Settings::base(false)).unwrap();
    assert_eq!(s.test_cases, 3, "ci chains through the registered default");
    assert!(s.derandomize);
}

#[test]
fn snapshots_reproduce_their_settings_over_any_base() {
    let original = Settings::base(false)
        .test_cases(7)
        .verbosity(Verbosity::Debug)
        .seed(Some(11))
        .derandomize(true)
        .database(Some("db".to_owned()))
        .suppress_health_check([HealthCheck::TooSlow])
        .phases([Phase::Generate])
        .report_multiple_failures(true)
        .show_statistics(true)
        .print_blob(true)
        .backend(Backend::Urandom);
    let mut restored = Settings::base(false)
        .test_cases(1)
        .verbosity(Verbosity::Quiet);
    ProfileDelta::snapshot(&original).apply(&mut restored);
    assert_eq!(restored.test_cases, 7);
    assert_eq!(restored.verbosity, Verbosity::Debug);
    assert_eq!(restored.seed, Some(11));
    assert!(restored.derandomize);
    assert_eq!(restored.database, Database::Path("db".to_owned()));
    assert_eq!(restored.suppress_health_check, vec![HealthCheck::TooSlow]);
    assert_eq!(restored.phases, vec![Phase::Generate]);
    assert!(restored.report_multiple_failures);
    assert!(restored.show_statistics);
    assert!(restored.print_blob);
    assert_eq!(restored.backend, Some(Backend::Urandom));
}

#[test]
fn register_validates_names_and_replaces_earlier_registrations() {
    assert_eq!(
        register("bad name", &Settings::base(false)),
        Err(ProfileError::InvalidName("bad name".to_owned()))
    );
    assert_eq!(
        register("", &Settings::base(false)),
        Err(ProfileError::InvalidName(String::new()))
    );
    let name = "profiles_tests_register_replaces";
    register(name, &Settings::base(false).test_cases(1)).unwrap();
    register(name, &Settings::base(false).test_cases(2)).unwrap();
    let registry = registry_snapshot();
    let s = resolve(name, &no_config(), &registry, &Settings::base(false)).unwrap();
    assert_eq!(s.test_cases, 2);
}

#[test]
fn invalid_name_errors_render_helpfully() {
    let e = ProfileError::InvalidName("bad name".to_owned());
    assert_eq!(
        e.to_string(),
        "invalid profile name \"bad name\": profile names use only \
         ASCII letters, digits, '-' and '_'"
    );
}

#[test]
fn config_errors_render_with_and_without_a_line() {
    let with_line = ProfileError::Config {
        path: "x/hegel.toml".to_owned(),
        line: 3,
        message: "boom".to_owned(),
    };
    assert_eq!(with_line.to_string(), "x/hegel.toml:3: boom");
    let whole_file = ProfileError::Config {
        path: "x/hegel.toml".to_owned(),
        line: 0,
        message: "boom".to_owned(),
    };
    assert_eq!(whole_file.to_string(), "x/hegel.toml: boom");
}

#[test]
fn selected_name_prefers_the_default_profile_variable() {
    let env = env_of(&[
        ("HEGEL_DEFAULT_PROFILE", "nightly"),
        ("CI", "true"),
        ("ANTITHESIS_OUTPUT_DIR", "/tmp"),
    ]);
    assert_eq!(selected_name(env), "nightly");
}

#[test]
fn selected_name_ignores_an_empty_default_profile_variable() {
    let env = env_of(&[("HEGEL_DEFAULT_PROFILE", ""), ("CI", "true")]);
    assert_eq!(selected_name(env), "ci");
}

#[cfg(not(windows))]
#[test]
fn selected_name_prefers_antithesis_over_ci() {
    let env = env_of(&[("ANTITHESIS_OUTPUT_DIR", "/tmp"), ("CI", "true")]);
    assert_eq!(selected_name(env), "antithesis");
}

#[test]
fn selected_name_falls_back_to_default() {
    assert_eq!(selected_name(env_of(&[])), "default");
    assert_eq!(selected_name(env_of(&[("CI", "true")])), "ci");
}

#[test]
fn settings_for_from_resolves_the_selected_profile() {
    let env = env_of(&[("CI", "true")]);
    let s = settings_for_from(None, &no_config(), &[], env).unwrap();
    assert!(s.derandomize);
    assert!(s.print_blob);
    let s = settings_for_from(Some("default"), &no_config(), &[], env).unwrap();
    assert!(!s.derandomize, "an explicit profile ignores CI detection");
}

#[test]
fn settings_for_from_stamps_the_loaded_config_path() {
    let mut config = config_of("[profiles.x]\ntest_cases = 5\n");
    config.path = Some("/a/hegel.toml".to_owned());
    let s = settings_for_from(Some("x"), &config, &[], env_of(&[])).unwrap();
    assert_eq!(s.config_path.as_deref(), Some("/a/hegel.toml"));
    let s = settings_for_from(Some("default"), &no_config(), &[], env_of(&[])).unwrap();
    assert_eq!(s.config_path, None);
}

#[test]
fn settings_for_from_rejects_an_unknown_default_profile_variable() {
    let env = env_of(&[("HEGEL_DEFAULT_PROFILE", "bogus")]);
    assert_eq!(
        settings_for_from(None, &no_config(), &[], env).unwrap_err(),
        ProfileError::UnknownProfile("bogus".to_owned())
    );
}

#[test]
fn settings_for_from_validates_unselected_profiles_eagerly() {
    let cycling = config_of("[profiles.unused]\nextends = \"unused\"\n");
    assert_eq!(
        settings_for_from(Some("default"), &cycling, &[], env_of(&[])).unwrap_err(),
        ProfileError::ExtendsCycle(vec!["unused".to_owned(), "unused".to_owned()])
    );
    let dangling = config_of("[profiles.unused]\nextends = \"ghost\"\n");
    assert_eq!(
        settings_for_from(Some("default"), &dangling, &[], env_of(&[])).unwrap_err(),
        ProfileError::UnknownExtends {
            profile: "unused".to_owned(),
            extends: "ghost".to_owned(),
        }
    );
}

#[cfg(not(windows))]
#[test]
fn settings_for_from_stamps_antithesis_detection_regardless_of_profile() {
    let env = env_of(&[
        ("ANTITHESIS_OUTPUT_DIR", "/tmp"),
        ("HEGEL_DEFAULT_PROFILE", "default"),
    ]);
    let s = settings_for_from(None, &no_config(), &[], env).unwrap();
    assert!(s.in_antithesis);
    for check in ALL_HEALTH_CHECKS {
        assert!(s.health_check_suppressed(check), "{check:?}");
    }
    assert_eq!(
        s.database,
        Database::Unset,
        "explicitly selecting default opts out of the antithesis profile's database policy"
    );
}

/// The real, sys-backed `settings_for`. Must resolve whatever profile the
/// ambient environment selects.
#[test]
fn settings_for_reads_the_real_environment() {
    assert!(settings_for(Some("default")).is_ok());
}

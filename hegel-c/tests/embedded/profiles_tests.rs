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

fn no_candidates() -> Candidates {
    Candidates {
        overridden: None,
        env: None,
        toml: None,
        environment: FALLBACK,
    }
}

fn detected(name: &'static str) -> Candidates {
    Candidates {
        environment: name,
        ..no_candidates()
    }
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
    resolve(name, config, &[], &Settings::base(false), &no_candidates()).unwrap()
}

fn resolve_err(name: &str, config: &ConfigFile) -> ProfileError {
    resolve(name, config, &[], &Settings::base(false), &no_candidates()).unwrap_err()
}

#[test]
fn base_is_the_base_settings() {
    let s = resolve_named("base", &no_config());
    assert_eq!(s.test_cases, 100);
    assert_eq!(s.verbosity, Verbosity::Normal);
    assert_eq!(s.seed, None);
    assert!(!s.derandomize);
    assert_eq!(s.database, Database::Unset);
    assert!(s.suppress_health_check.is_empty());
    assert_eq!(s.phases.len(), 5);
    assert!(!s.report_multiple_failures);
    assert!(!s.show_statistics);
    assert!(s.print_blob);
    assert_eq!(s.backend, Backend::Default);
}

fn assert_same_settings(a: &Settings, b: &Settings) {
    assert_eq!(ProfileDelta::snapshot(a), ProfileDelta::snapshot(b));
}

#[test]
fn the_development_profile_is_the_base_defaults_unchanged() {
    let s = resolve_named("development", &no_config());
    assert_same_settings(&s, &resolve_named("base", &no_config()));
}

#[test]
fn the_default_alias_resolves_to_development_with_no_candidates() {
    let s = resolve_named("default", &no_config());
    assert_same_settings(&s, &resolve_named("development", &no_config()));
}

#[test]
fn the_ci_profile_derandomizes_disables_the_database_and_prints_blobs() {
    let s = resolve_named("ci", &no_config());
    assert!(s.derandomize);
    assert_eq!(s.database, Database::Disabled);
    assert!(s.print_blob);
    assert_eq!(s.test_cases, 100);
    assert_eq!(s.suppress_health_check, vec![HealthCheck::TooSlow]);
}

#[test]
fn a_config_delta_re_enables_the_too_slow_check_on_ci() {
    let config = config_of("[profiles.ci]\nsuppress_health_check = []\n");
    let s = resolve_named("ci", &config);
    assert!(s.suppress_health_check.is_empty());
}

const ALL_HEALTH_CHECKS: [HealthCheck; 4] = [
    HealthCheck::FilterTooMuch,
    HealthCheck::TooSlow,
    HealthCheck::TestCasesTooLarge,
    HealthCheck::LargeInitialTestCase,
];

#[test]
fn the_workload_profile_selects_urandom_and_disables_the_database_and_every_health_check() {
    let s = resolve_named("workload", &no_config());
    assert_eq!(s.backend, Backend::Urandom);
    assert_eq!(s.database, Database::Disabled);
    for check in ALL_HEALTH_CHECKS {
        assert!(s.health_check_suppressed(check), "{check:?}");
    }
    assert!(!s.derandomize, "Antithesis controls randomness itself");
    assert!(s.print_blob);
}

#[test]
fn antithesis_detection_does_not_force_health_checks_off() {
    for name in ["base", "development"] {
        let s = resolve(
            name,
            &no_config(),
            &[],
            &Settings::base(true),
            &no_candidates(),
        )
        .unwrap();
        assert!(s.in_antithesis);
        for check in ALL_HEALTH_CHECKS {
            assert!(!s.health_check_suppressed(check), "{name}: {check:?}");
        }
    }
}

#[test]
fn a_config_delta_re_enables_health_checks_in_antithesis() {
    let config = config_of("[profiles.workload]\nsuppress_health_check = [\"too_slow\"]\n");
    let s = resolve(
        "workload",
        &config,
        &[],
        &Settings::base(true),
        &no_candidates(),
    )
    .unwrap();
    assert!(s.health_check_suppressed(HealthCheck::TooSlow));
    assert!(!s.health_check_suppressed(HealthCheck::FilterTooMuch));
}

#[test]
fn health_checks_run_unless_suppressed() {
    let s = resolve_named("development", &no_config());
    for check in ALL_HEALTH_CHECKS {
        assert!(!s.health_check_suppressed(check), "{check:?}");
    }
    let config = config_of("[profiles.development]\nsuppress_health_check = [\"too_slow\"]\n");
    let s = resolve_named("development", &config);
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
fn shipped_profiles_root_in_the_base_defaults_not_development() {
    let config = config_of("[profiles.development]\ntest_cases = 200\nderandomize = false\n");
    let s = resolve_named("ci", &config);
    assert_eq!(s.test_cases, 100, "development is a sibling, not a layer");
    assert!(s.derandomize);
    let s = resolve_named("workload", &config);
    assert_eq!(s.test_cases, 100);
    let s = resolve_named("development", &config);
    assert_eq!(s.test_cases, 200);
}

#[test]
fn config_profiles_extend_the_default_alias_implicitly() {
    let config = config_of(
        "[profiles.development]\ntest_cases = 200\n[profiles.nightly]\nshow_statistics = true\n",
    );
    let s = resolve_named("nightly", &config);
    assert_eq!(
        s.test_cases, 200,
        "locally the implicit parent is development"
    );
    assert!(s.show_statistics);
    let s = resolve(
        "nightly",
        &config,
        &[],
        &Settings::base(false),
        &detected("ci"),
    )
    .unwrap();
    assert!(s.derandomize, "on CI the implicit parent is ci");
    assert!(s.print_blob);
    assert_eq!(
        s.test_cases, 100,
        "development is not part of the chain on CI"
    );
}

#[test]
fn explicit_default_extends_matches_the_implicit_parent() {
    let config = config_of("[profiles.a]\nextends = \"default\"\n[profiles.b]\n");
    let candidates = detected("ci");
    let a = resolve("a", &config, &[], &Settings::base(false), &candidates).unwrap();
    let b = resolve("b", &config, &[], &Settings::base(false), &candidates).unwrap();
    assert_same_settings(&a, &b);
    assert!(a.derandomize);
}

#[test]
fn extending_base_pins_the_base_settings() {
    let config = config_of("[profiles.plain]\nextends = \"base\"\ntest_cases = 7\n");
    let s = resolve(
        "plain",
        &config,
        &[],
        &Settings::base(false),
        &detected("ci"),
    )
    .unwrap();
    assert_eq!(s.test_cases, 7);
    assert!(!s.derandomize, "extends = \"base\" opts out of ci");
    assert!(s.print_blob);
}

#[test]
fn config_profiles_extend_a_named_parent() {
    let config = config_of(
        "[profiles.ci]\ntest_cases = 1000\n[profiles.nightly]\nextends = \"ci\"\ntest_cases = 10000\n",
    );
    let s = resolve_named("nightly", &config);
    assert_eq!(s.test_cases, 10000);
    assert!(s.derandomize, "inherited from shipped ci");
    assert_eq!(s.database, Database::Disabled, "inherited from shipped ci");
    let s = resolve_named("ci", &config);
    assert_eq!(s.test_cases, 1000);
}

#[test]
fn shipped_profiles_accept_an_explicit_extends() {
    let config =
        config_of("[profiles.common]\ntest_cases = 9\n[profiles.ci]\nextends = \"common\"\n");
    let s = resolve_named("ci", &config);
    assert_eq!(s.test_cases, 9);
    assert!(s.derandomize, "the shipped ci delta still applies");
}

#[test]
fn the_alias_skips_profiles_already_in_the_chain() {
    let config =
        config_of("[profiles.common]\ntest_cases = 9\n[profiles.ci]\nextends = \"common\"\n");
    let s = resolve("ci", &config, &[], &Settings::base(false), &detected("ci")).unwrap();
    assert_eq!(
        s.test_cases, 9,
        "common's implicit parent skips the already-visited ci and falls back to base"
    );
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
fn a_config_delta_overrides_an_inherited_backend() {
    let config = config_of(
        "[profiles.workload]\nbackend = \"default\"\n[profiles.x]\nextends = \"development\"\nbackend = \"urandom\"\n",
    );
    assert_eq!(resolve_named("workload", &config).backend, Backend::Default);
    assert_eq!(resolve_named("x", &config).backend, Backend::Urandom);
}

#[test]
fn seed_none_clears_an_inherited_seed() {
    let config = config_of("[profiles.development]\nseed = 5\n[profiles.x]\nseed = \"none\"\n");
    assert_eq!(resolve_named("development", &config).seed, Some(5));
    assert_eq!(resolve_named("x", &config).seed, None);
}

#[test]
fn database_default_restores_the_default_database() {
    let config = config_of("[profiles.x]\nextends = \"ci\"\ndatabase = \"default\"\n");
    let s = resolve_named("x", &config);
    assert_eq!(s.database, Database::Unset);
    assert!(s.derandomize, "the rest of ci still applies");
}

#[test]
fn self_cycles_are_reported() {
    let config = config_of("[profiles.a]\nextends = \"a\"\n");
    assert_eq!(
        resolve_err("a", &config),
        ProfileError::ExtendsCycle(vec!["a".to_owned(), "a".to_owned()])
    );
}

#[test]
fn two_step_cycles_are_reported_with_the_walk() {
    let config = config_of("[profiles.a]\nextends = \"b\"\n[profiles.b]\nextends = \"a\"\n");
    let e = resolve_err("a", &config);
    assert_eq!(
        e,
        ProfileError::ExtendsCycle(vec!["a".to_owned(), "b".to_owned(), "a".to_owned()])
    );
    assert_eq!(e.to_string(), "profile extends cycle: a -> b -> a");
}

#[test]
fn unknown_profiles_are_reported_with_the_known_names() {
    let e = resolve_err("nope", &no_config());
    assert_eq!(
        e,
        ProfileError::UnknownProfile {
            name: "nope".to_owned(),
            source: None,
            known: vec![
                "base".to_owned(),
                "ci".to_owned(),
                "development".to_owned(),
                "workload".to_owned(),
            ],
        }
    );
    assert_eq!(
        e.to_string(),
        "unknown settings profile \"nope\"; known profiles: \
         base, ci, development, workload"
    );
}

#[test]
fn known_names_include_config_and_registered_profiles() {
    let config = config_of("[profiles.nightly]\n");
    let registry = vec![(
        "mine".to_owned(),
        ProfileDelta::snapshot(&Settings::base(false)),
    )];
    let e = resolve(
        "nope",
        &config,
        &registry,
        &Settings::base(false),
        &no_candidates(),
    )
    .unwrap_err();
    let ProfileError::UnknownProfile { known, .. } = e else {
        panic!("expected UnknownProfile, got {e:?}");
    };
    assert_eq!(
        known,
        vec!["base", "ci", "development", "mine", "nightly", "workload"]
    );
}

#[test]
fn unknown_extends_name_the_referring_profile() {
    let config = config_of("[profiles.a]\nextends = \"ghost\"\n");
    let e = resolve_err("a", &config);
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
fn extends_is_rejected_on_registered_profiles() {
    let registry = vec![(
        "mine".to_owned(),
        ProfileDelta::snapshot(&Settings::base(false)),
    )];
    let config = config_of("[profiles.mine]\nextends = \"ci\"\n");
    let e = resolve(
        "mine",
        &config,
        &registry,
        &Settings::base(false),
        &no_candidates(),
    )
    .unwrap_err();
    assert_eq!(e, ProfileError::ExtendsOnRegistered("mine".to_owned()));
    assert_eq!(
        e.to_string(),
        "cannot set extends on registered profile \"mine\": \
         a registered profile is a complete snapshot"
    );
}

#[test]
fn registered_profiles_resolve_to_their_snapshot() {
    let snapshot = Settings::base(false).test_cases(5).derandomize(true);
    let registry = vec![("mine".to_owned(), ProfileDelta::snapshot(&snapshot))];
    let s = resolve(
        "mine",
        &no_config(),
        &registry,
        &Settings::base(false),
        &no_candidates(),
    )
    .unwrap();
    assert_eq!(s.test_cases, 5);
    assert!(s.derandomize);
}

#[test]
fn config_deltas_merge_onto_registered_profiles() {
    let snapshot = Settings::base(false).test_cases(5);
    let registry = vec![("mine".to_owned(), ProfileDelta::snapshot(&snapshot))];
    let config = config_of("[profiles.mine]\ntest_cases = 9\n");
    let s = resolve(
        "mine",
        &config,
        &registry,
        &Settings::base(false),
        &no_candidates(),
    )
    .unwrap();
    assert_eq!(s.test_cases, 9);
}

#[test]
fn registered_profiles_replace_shipped_ones() {
    let registry = vec![(
        "development".to_owned(),
        ProfileDelta::snapshot(&Settings::base(false).test_cases(3)),
    )];
    let s = resolve(
        "development",
        &no_config(),
        &registry,
        &Settings::base(false),
        &no_candidates(),
    )
    .unwrap();
    assert_eq!(s.test_cases, 3);
    let s = resolve(
        "ci",
        &no_config(),
        &registry,
        &Settings::base(false),
        &no_candidates(),
    )
    .unwrap();
    assert_eq!(s.test_cases, 100, "ci does not chain through development");
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
        .seed(Some(23))
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
    assert_eq!(restored.backend, Backend::Urandom);
}

#[test]
fn snapshots_clear_an_inherited_seed() {
    let mut restored = Settings::base(false).seed(Some(23));
    ProfileDelta::snapshot(&Settings::base(false)).apply(&mut restored);
    assert_eq!(restored.seed, None);
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
    let s = resolve(
        name,
        &no_config(),
        &registry,
        &Settings::base(false),
        &no_candidates(),
    )
    .unwrap();
    assert_eq!(s.test_cases, 2);
}

#[test]
fn register_rejects_the_reserved_names() {
    for name in ["base", "default"] {
        let e = register(name, &Settings::base(false)).unwrap_err();
        assert_eq!(e, ProfileError::ReservedName(name.to_owned()));
    }
    assert_eq!(
        ProfileError::ReservedName("base".to_owned()).to_string(),
        "cannot register reserved profile name \"base\""
    );
}

#[test]
fn set_default_profile_validates_the_name() {
    assert_eq!(
        set_default_profile(Some("bad name")),
        Err(ProfileError::InvalidName("bad name".to_owned()))
    );
    let e = set_default_profile(Some("default")).unwrap_err();
    assert_eq!(
        e,
        ProfileError::CircularDefault {
            source: "hegel_set_default_profile",
        }
    );
    assert_eq!(
        e.to_string(),
        "hegel_set_default_profile cannot name the \"default\" alias it resolves"
    );
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
fn the_strongest_named_default_displaces_the_weaker_ones() {
    let candidates = Candidates {
        overridden: Some("a".to_owned()),
        env: Some("b".to_owned()),
        toml: Some("c".to_owned()),
        environment: "ci",
    };
    assert_eq!(
        candidates.resolve(&[]).unwrap(),
        ("a", Some("hegel_set_default_profile"))
    );
    assert_eq!(
        candidates.resolve(&["a"]).unwrap(),
        ("ci", None),
        "a visited named default falls through to the environment, never to a weaker setting"
    );
    let candidates = Candidates {
        env: Some("b".to_owned()),
        toml: Some("c".to_owned()),
        ..no_candidates()
    };
    assert_eq!(
        candidates.resolve(&[]).unwrap(),
        ("b", Some("HEGEL_DEFAULT_PROFILE"))
    );
    let candidates = Candidates {
        toml: Some("c".to_owned()),
        ..no_candidates()
    };
    assert_eq!(
        candidates.resolve(&[]).unwrap(),
        ("c", Some("the default entry in hegel.toml"))
    );
}

#[test]
fn the_alias_falls_back_from_the_environment_to_base() {
    let candidates = detected("ci");
    assert_eq!(candidates.resolve(&[]).unwrap(), ("ci", None));
    assert_eq!(
        candidates.resolve(&["ci"]).unwrap(),
        ("base", None),
        "the environment profiles never layer over one another"
    );
    assert_eq!(no_candidates().resolve(&[]).unwrap(), ("development", None));
    assert_eq!(
        no_candidates().resolve(&["development"]).unwrap(),
        ("base", None)
    );
}

#[test]
fn a_default_profile_variable_naming_the_alias_is_circular() {
    let candidates = Candidates {
        env: Some("default".to_owned()),
        ..no_candidates()
    };
    assert_eq!(
        candidates.resolve(&[]).unwrap_err(),
        ProfileError::CircularDefault {
            source: "HEGEL_DEFAULT_PROFILE",
        }
    );
    assert_eq!(
        ProfileError::CircularDefault {
            source: "HEGEL_DEFAULT_PROFILE",
        }
        .to_string(),
        "HEGEL_DEFAULT_PROFILE cannot name the \"default\" alias it resolves"
    );
}

fn settings_for_env(
    name: Option<&str>,
    config: &ConfigFile,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Settings, ProfileError> {
    settings_for_from(name, config, &[], None, env)
}

#[test]
fn settings_for_from_resolves_the_default_profile() {
    let env = env_of(&[("CI", "true")]);
    let s = settings_for_env(None, &no_config(), env).unwrap();
    assert!(s.derandomize);
    assert!(s.print_blob);
    let s = settings_for_env(Some("base"), &no_config(), env).unwrap();
    assert!(!s.derandomize, "base ignores CI detection");
}

#[test]
fn explicitly_selected_profiles_still_layer_over_the_environment() {
    let env = env_of(&[("CI", "true")]);
    let config = config_of("[profiles.nightly]\ntest_cases = 7\n");
    let s = settings_for_from(Some("nightly"), &config, &[], None, env).unwrap();
    assert_eq!(s.test_cases, 7);
    assert!(
        s.derandomize,
        "nightly's implicit parent is ci on a CI server"
    );
}

#[test]
fn the_default_profile_variable_prefers_over_detection() {
    let env = env_of(&[("HEGEL_DEFAULT_PROFILE", "nightly"), ("CI", "true")]);
    let config = config_of("[profiles.nightly]\ntest_cases = 7\nderandomize = false\n");
    let s = settings_for_from(None, &config, &[], None, env).unwrap();
    assert_eq!(s.test_cases, 7);
    assert!(!s.derandomize, "nightly's delta wins over the inherited ci");
    assert_eq!(
        s.database,
        Database::Disabled,
        "ci still sits under nightly"
    );
}

#[test]
fn an_empty_default_profile_variable_is_ignored() {
    let env = env_of(&[("HEGEL_DEFAULT_PROFILE", ""), ("CI", "true")]);
    let s = settings_for_env(None, &no_config(), env).unwrap();
    assert!(s.derandomize);
}

#[cfg(not(windows))]
#[test]
fn antithesis_detection_beats_ci_detection() {
    let env = env_of(&[("ANTITHESIS_OUTPUT_DIR", "/tmp"), ("CI", "true")]);
    let s = settings_for_env(None, &no_config(), env).unwrap();
    assert!(!s.derandomize, "the workload profile won, not ci");
    assert_eq!(s.database, Database::Disabled);
}

#[test]
fn the_toml_default_entry_selects_a_profile() {
    let config = config_of("default = \"nightly\"\n[profiles.nightly]\ntest_cases = 7\n");
    let s = settings_for_env(None, &config, env_of(&[])).unwrap();
    assert_eq!(s.test_cases, 7);
    let env = env_of(&[("HEGEL_DEFAULT_PROFILE", "development")]);
    let s = settings_for_env(None, &config, env).unwrap();
    assert_eq!(
        s.test_cases, 100,
        "the environment variable wins over the entry"
    );
}

#[test]
fn the_override_wins_over_everything() {
    let env = env_of(&[("HEGEL_DEFAULT_PROFILE", "nightly"), ("CI", "true")]);
    let config = config_of(
        "default = \"nightly\"\n[profiles.nightly]\ntest_cases = 7\n\
         [profiles.mine]\ntest_cases = 9\n",
    );
    let s = settings_for_from(None, &config, &[], Some("mine".to_owned()), env).unwrap();
    assert_eq!(s.test_cases, 9);
}

#[test]
fn settings_for_from_stamps_the_loaded_config_path() {
    let mut config = config_of("[profiles.x]\ntest_cases = 5\n");
    config.path = Some("/a/hegel.toml".to_owned());
    let s = settings_for_env(Some("x"), &config, env_of(&[])).unwrap();
    assert_eq!(s.config_path.as_deref(), Some("/a/hegel.toml"));
    let s = settings_for_env(Some("base"), &no_config(), env_of(&[])).unwrap();
    assert_eq!(s.config_path, None);
}

#[test]
fn settings_for_from_rejects_an_unknown_default_profile_variable() {
    let env = env_of(&[("HEGEL_DEFAULT_PROFILE", "bogus")]);
    let e = settings_for_env(None, &no_config(), env).unwrap_err();
    assert_eq!(
        e,
        ProfileError::UnknownProfile {
            name: "bogus".to_owned(),
            source: Some("HEGEL_DEFAULT_PROFILE"),
            known: vec![
                "base".to_owned(),
                "ci".to_owned(),
                "development".to_owned(),
                "workload".to_owned(),
            ],
        }
    );
    assert_eq!(
        e.to_string(),
        "unknown settings profile \"bogus\" (named by HEGEL_DEFAULT_PROFILE); \
         known profiles: base, ci, development, workload"
    );
}

#[test]
fn base_resolves_despite_a_broken_default_profile_variable() {
    let env = env_of(&[("HEGEL_DEFAULT_PROFILE", "bogus")]);
    assert!(settings_for_env(Some("base"), &no_config(), env).is_ok());
}

#[test]
fn settings_for_from_validates_unselected_profiles_eagerly() {
    let cycling = config_of("[profiles.unused]\nextends = \"unused\"\n");
    assert_eq!(
        settings_for_env(Some("base"), &cycling, env_of(&[])).unwrap_err(),
        ProfileError::ExtendsCycle(vec!["unused".to_owned(), "unused".to_owned()])
    );
    let dangling = config_of("[profiles.unused]\nextends = \"ghost\"\n");
    assert_eq!(
        settings_for_env(Some("base"), &dangling, env_of(&[])).unwrap_err(),
        ProfileError::UnknownExtends {
            profile: "unused".to_owned(),
            extends: "ghost".to_owned(),
        }
    );
}

#[test]
fn settings_for_from_validates_the_toml_default_entry_eagerly() {
    let config = config_of("default = \"ghost\"\n");
    let e = settings_for_env(Some("base"), &config, env_of(&[])).unwrap_err();
    assert_eq!(
        e,
        ProfileError::UnknownProfile {
            name: "ghost".to_owned(),
            source: Some("the default entry in hegel.toml"),
            known: vec![
                "base".to_owned(),
                "ci".to_owned(),
                "development".to_owned(),
                "workload".to_owned(),
            ],
        }
    );
}

#[cfg(not(windows))]
#[test]
fn settings_for_from_stamps_antithesis_detection_regardless_of_profile() {
    let env = env_of(&[
        ("ANTITHESIS_OUTPUT_DIR", "/tmp"),
        ("HEGEL_DEFAULT_PROFILE", "base"),
    ]);
    let s = settings_for_env(None, &no_config(), env).unwrap();
    assert!(s.in_antithesis);
    assert_eq!(
        s.backend,
        Backend::Default,
        "explicitly selecting base opts out of the workload profile's backend"
    );
    for check in ALL_HEALTH_CHECKS {
        assert!(
            !s.health_check_suppressed(check),
            "explicitly selecting base opts out of the workload profile's health-check policy: {check:?}"
        );
    }
    assert_eq!(
        s.database,
        Database::Unset,
        "explicitly selecting base opts out of the workload profile's database policy"
    );
}

/// The real, sys-backed `settings_for`. Must resolve whatever the ambient
/// environment selects.
#[test]
fn settings_for_reads_the_real_environment() {
    assert!(settings_for(Some("base")).is_ok());
}

#[test]
fn set_default_profile_stores_and_clears_the_override() {
    let name = "profiles_tests_default_override";
    register(name, &Settings::base(false)).unwrap();
    set_default_profile(Some(name)).unwrap();
    assert_eq!(DEFAULT_OVERRIDE.lock().as_deref(), Some(name));
    set_default_profile(None).unwrap();
    assert_eq!(*DEFAULT_OVERRIDE.lock(), None);
}

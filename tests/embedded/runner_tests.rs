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
fn test_settings_suppress_health_check_replaces() {
    let s = Settings::new()
        .suppress_health_check([HealthCheck::TooSlow])
        .suppress_health_check([HealthCheck::FilterTooMuch]);
    assert_eq!(s.suppress_health_check, vec![HealthCheck::FilterTooMuch]);
    let s = s.suppress_health_check([]);
    assert_eq!(s.suppress_health_check, vec![]);
}

#[test]
fn test_settings_report_multiple_failures_default_false() {
    let s = Settings::new();
    assert!(!s.report_multiple_failures);
}

#[test]
fn test_settings_report_multiple_failures_setter() {
    let s = Settings::new().report_multiple_failures(true);
    assert!(s.report_multiple_failures);
    let s = s.report_multiple_failures(false);
    assert!(!s.report_multiple_failures);
}

#[test]
fn test_settings_backend_default_unset() {
    let s = Settings::new();
    assert_eq!(s.backend, None);
}

#[test]
fn test_settings_backend_setter() {
    let s = Settings::new().backend(Backend::Urandom);
    assert_eq!(s.backend, Some(Backend::Urandom));
    let s = s.backend(Backend::Default);
    assert_eq!(s.backend, Some(Backend::Default));
}

#[test]
fn test_settings_mode_default_unset() {
    let s = Settings::new();
    assert_eq!(s.mode, None);
}

#[test]
fn test_settings_resolved_mode_explicit_wins() {
    let s = Settings::new().mode(Mode::TestRun);
    assert_eq!(s.resolved_mode(true), Mode::TestRun);
    assert_eq!(s.resolved_mode(false), Mode::TestRun);
    let s = s.mode(Mode::SingleTestCase);
    assert_eq!(s.resolved_mode(true), Mode::SingleTestCase);
    assert_eq!(s.resolved_mode(false), Mode::SingleTestCase);
}

#[test]
fn test_settings_resolved_mode_single_test_case_under_antithesis() {
    let s = Settings::new();
    assert_eq!(s.resolved_mode(true), Mode::SingleTestCase);
    assert_eq!(s.resolved_mode(false), Mode::TestRun);
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
fn test_env_override_replaces_test_cases() {
    let s = Settings::new()
        .test_cases(5)
        .with_env_overrides_from(|key| (key == "HEGEL_TEST_CASES").then(|| "17".to_string()));
    assert_eq!(s.test_cases, 17);
}

#[test]
fn test_env_override_test_cases_absent_is_ignored() {
    let s = Settings::new()
        .test_cases(5)
        .with_env_overrides_from(|_| None);
    assert_eq!(s.test_cases, 5);
}

#[test]
fn test_env_override_test_cases_empty_is_ignored() {
    let s = Settings::new()
        .test_cases(5)
        .with_env_overrides_from(|key| (key == "HEGEL_TEST_CASES").then(String::new));
    assert_eq!(s.test_cases, 5);
}

#[test]
#[should_panic(expected = "HEGEL_TEST_CASES must be a positive integer, got \"lots\"")]
fn test_env_override_test_cases_non_numeric_is_a_usage_error() {
    Settings::new()
        .with_env_overrides_from(|key| (key == "HEGEL_TEST_CASES").then(|| "lots".to_string()));
}

#[test]
#[should_panic(expected = "HEGEL_TEST_CASES must be a positive integer, got \"0\"")]
fn test_env_override_test_cases_zero_is_a_usage_error() {
    Settings::new()
        .with_env_overrides_from(|key| (key == "HEGEL_TEST_CASES").then(|| "0".to_string()));
}

#[test]
fn test_env_override_database_disabled_keyword() {
    use crate::runner::Database;
    let s = Settings::new()
        .database(Some("custom".to_string()))
        .with_env_overrides_from(|key| (key == "HEGEL_DATABASE").then(|| "disabled".to_string()));
    assert_eq!(s.database, Database::Disabled);
}

#[test]
fn test_env_override_database_path() {
    use crate::runner::Database;
    let s = Settings::new()
        .database(None)
        .with_env_overrides_from(|key| (key == "HEGEL_DATABASE").then(|| "my-db".to_string()));
    assert_eq!(s.database, Database::Path("my-db".to_string()));
}

#[test]
fn test_env_override_database_empty_is_ignored() {
    use crate::runner::Database;
    let s = Settings::new()
        .database(Some("custom".to_string()))
        .with_env_overrides_from(|key| (key == "HEGEL_DATABASE").then(String::new));
    assert_eq!(s.database, Database::Path("custom".to_string()));
}

#[test]
fn test_is_in_ci_from_detects_presence_and_value_variables() {
    assert!(!is_in_ci_from(|_| None));
    assert!(is_in_ci_from(|key| (key == "CI").then(String::new)));
    assert!(is_in_ci_from(
        |key| (key == "TF_BUILD").then(|| "true".to_string())
    ));
    assert!(!is_in_ci_from(
        |key| (key == "TF_BUILD").then(|| "false".to_string())
    ));
}

#[test]
fn test_native_engine_creates_default_dot_hegel_when_database_unset() {
    use crate::Hegel;
    use crate::generators as gs;
    use crate::runner::Database;

    if std::env::var_os("HEGEL_DOT_HEGEL_TEST_CHILD").is_some() {
        let settings = Settings::new();
        assert_eq!(settings.database, Database::Unset);
        let result = std::panic::catch_unwind(|| {
            Hegel::new(|tc| {
                let _ = tc.draw(gs::booleans());
                panic!("stored failure");
            })
            .__database_key("dot_hegel_child".to_string())
            .settings(Settings::new().test_cases(1))
            .run();
        });
        assert!(result.is_err());
        assert!(std::path::Path::new(".hegel").is_dir());
        return;
    }

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
    let tmp = tempfile::TempDir::new().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            "runner::tests::test_native_engine_creates_default_dot_hegel_when_database_unset",
        ])
        .current_dir(tmp.path())
        .env("HEGEL_DOT_HEGEL_TEST_CHILD", "1");
    for name in CI_VAR_NAMES {
        child.env_remove(name);
    }
    let output = child.output().unwrap();
    assert!(
        output.status.success(),
        "child test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_settings_for_ci_disables_database_and_derandomizes() {
    use crate::runner::Database;
    let settings = Settings::for_ci(true);
    assert_eq!(settings.database, Database::Disabled);
    assert!(settings.derandomize);
}

#[test]
fn test_settings_outside_ci_leave_database_unset_and_randomized() {
    use crate::runner::Database;
    let settings = Settings::for_ci(false);
    assert_eq!(settings.database, Database::Unset);
    assert!(!settings.derandomize);
}

#[test]
fn multiple_failures_with_print_blob_emit_per_failure_reproducer_lines() {
    use crate::generators as gs;
    let result = std::panic::catch_unwind(|| {
        Hegel::new(|tc: TestCase| {
            let n: i32 = tc.draw(gs::integers::<i32>().min_value(-100).max_value(100));
            if n >= 50 {
                panic!("high {n}");
            }
            if n <= -50 {
                panic!("low {n}");
            }
        })
        .settings(
            Settings::new()
                .database(None)
                .seed(Some(1))
                .print_blob(true)
                .report_multiple_failures(true)
                .verbosity(Verbosity::Normal),
        )
        .run()
    });
    assert!(result.is_err(), "the property should fail");
}

#[test]
fn hegel_run_skips_when_generate_phase_disabled() {
    Hegel::new(|_tc: TestCase| panic!("must not run"))
        .settings(Settings::new().phases([]))
        .run();
}

mod reproduce {
    use super::*;
    use crate::ffi::{RunHandle, SettingsHandle};
    use crate::generators as gs;

    /// Property used by the replay tests: fails for any drawn i32 >= 1000.
    fn failing_property(tc: TestCase) {
        let n: i32 = tc.draw(gs::integers::<i32>());
        assert!(n < 1000, "boom: n = {n}");
    }

    /// Drive the failing property through a real run (via the C ABI) and
    /// return the reproduce blob the engine attached to the shrunk
    /// counterexample.
    fn discover_reproduce_blob() -> String {
        crate::run_lifecycle::init_panic_hook();
        let mut test_fn = failing_property;
        let settings = Settings::new()
            .test_cases(200)
            .seed(Some(7))
            .database(None)
            .verbosity(Verbosity::Quiet);
        let c_settings = SettingsHandle::build(&settings, None);
        let run = RunHandle::start(&c_settings, None).expect("the engine starts");
        while let Some(c_tc) = run.next_test_case() {
            crate::run_lifecycle::run_test_case(
                c_tc,
                &mut test_fn,
                false,
                Mode::TestRun,
                Verbosity::Quiet,
                &crate::test_case::RunOutput::resolve(),
                None,
            );
        }
        let result = run.result();
        assert!(result.failure_count() > 0, "property should have failed");
        result
            .failure(0)
            .reproduce_blob
            .expect("a shrunk failure carries a reproduce blob")
    }

    /// Drive `hegel.run()` to its failure panic and return the panic message.
    fn run_panic_message<F: FnMut(TestCase) + std::panic::UnwindSafe>(hegel: Hegel<F>) -> String {
        let result = std::panic::catch_unwind(|| hegel.run());
        let payload = result.expect_err("run should panic on a failing replay");
        payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("")
            .to_string()
    }

    #[test]
    fn hegel_reproduce_failure_replays_regardless_of_phases() {
        let blob = discover_reproduce_blob();
        let msg = run_panic_message(
            Hegel::new(failing_property)
                .settings(
                    Settings::new()
                        .phases([])
                        .database(None)
                        .verbosity(Verbosity::Quiet),
                )
                .reproduce_failure(blob),
        );
        assert!(msg.contains("boom: n ="), "unexpected panic message: {msg}");
    }

    #[test]
    fn hegel_reproduce_failure_first_blob_wins() {
        let blob = discover_reproduce_blob();
        let msg = run_panic_message(
            Hegel::new(failing_property)
                .settings(Settings::new().database(None).verbosity(Verbosity::Quiet))
                .reproduce_failure(blob)
                .reproduce_failure("!!! not a blob !!!"),
        );
        assert!(msg.contains("boom: n ="), "unexpected panic message: {msg}");
    }

    #[test]
    fn hegel_reproduce_failure_emits_its_diagnostic_when_not_quiet() {
        let blob = discover_reproduce_blob();
        let msg = run_panic_message(
            Hegel::new(failing_property)
                .settings(Settings::new().database(None).verbosity(Verbosity::Normal))
                .reproduce_failure(blob),
        );
        assert!(msg.contains("boom: n ="), "unexpected panic message: {msg}");
    }

    #[test]
    fn hegel_reproduce_failure_undecodable_blob_panics() {
        let msg = run_panic_message(
            Hegel::new(failing_property)
                .settings(Settings::new().database(None).verbosity(Verbosity::Quiet))
                .reproduce_failure("!!! not a blob !!!"),
        );
        assert!(msg.contains("could not be decoded"), "got: {msg}");
    }

    #[test]
    fn hegel_reproduce_failure_stale_blob_panics() {
        let blob = discover_reproduce_blob();
        let msg = run_panic_message(
            Hegel::new(|tc: TestCase| {
                let _: i32 = tc.draw(gs::integers::<i32>());
            })
            .settings(Settings::new().database(None).verbosity(Verbosity::Quiet))
            .reproduce_failure(blob),
        );
        assert!(
            msg.contains("no longer reproduces") || msg.to_lowercase().contains("stale"),
            "got: {msg}"
        );
    }
}

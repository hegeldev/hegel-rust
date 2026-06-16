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

// The backend's *resolution* (explicit choice vs. auto-urandom-under-
// Antithesis) is now the engine's job inside hegel-c, reached through the
// `hegel_settings_backend` C setter; the frontend only records the choice.

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

#[test]
fn multiple_failures_with_print_blob_emit_per_failure_reproducer_lines() {
    use crate::generators as gs;
    // Two distinct panic sites → two distinct origins, so the run reports
    // multiple failures. With print_blob enabled each per-failure block is
    // followed by its reproducer line (the `eprintln!` in `drive`'s
    // multi-failure path). The diagnostics go to this test's stderr.
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

// ── Hegel::run dispatch (phase gating) ───────────────────────────────────

#[test]
fn hegel_run_skips_when_generate_phase_disabled() {
    // Without Phase::Generate (and no replay blob) the engine generates
    // nothing: a body that always panics must never execute, and the run
    // passes (no panic).
    Hegel::new(|_tc: TestCase| panic!("must not run"))
        .settings(Settings::new().phases([]))
        .run();
}

// The `#[hegel::reproduce_failure]` replay path through the public API. The
// end-to-end attribute wiring is also exercised in tests/test_reproduce_failure.rs,
// but those run in subprocesses (so they don't contribute coverage); these
// in-process tests cover `drive_blob_replay` and the `Hegel::run` reproduce
// dispatch directly.
mod reproduce {
    use super::*;
    use crate::ffi::{RunHandle, SettingsHandle};

    /// Property used by the replay tests: fails for any drawn i32 >= 1000.
    fn failing_property(tc: TestCase) {
        let n: i32 = tc.draw(crate::generators::integers::<i32>());
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
        let run = RunHandle::start(&c_settings).expect("the engine starts");
        while let Some(c_tc) = run.next_test_case() {
            let is_final = c_tc.is_final_replay();
            crate::run_lifecycle::run_test_case(
                c_tc,
                &mut test_fn,
                is_final,
                Mode::TestRun,
                Verbosity::Quiet,
            );
        }
        let result = run.result();
        assert!(result.failure_count() > 0, "property should have failed");
        result
            .failure(0)
            .expect("a failure")
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
        // A blob replay is phase-agnostic: it runs (and surfaces the failure)
        // even with Phase::Generate disabled.
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
        // Only the first blob replays; later ones are source-level bookkeeping.
        // Were the second (undecodable) blob replayed instead, the run would
        // panic with a decode error rather than the property failure.
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
        // A non-quiet blob replay renders and emits the counterexample's
        // diagnostic block (the `eprint!` path in `drive_blob_replay`) before
        // re-raising the failure.
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
        // An undecodable blob is invalid input: the run panics with the decode
        // diagnostic rather than running the property.
        let msg = run_panic_message(
            Hegel::new(failing_property)
                .settings(Settings::new().database(None).verbosity(Verbosity::Quiet))
                .reproduce_failure("!!! not a blob !!!"),
        );
        assert!(msg.contains("could not be decoded"), "got: {msg}");
    }

    #[test]
    fn hegel_reproduce_failure_stale_blob_panics() {
        // A blob that decodes but no longer fails (replayed against a body that
        // doesn't panic) is reported as stale.
        let blob = discover_reproduce_blob();
        let msg = run_panic_message(
            Hegel::new(|tc: TestCase| {
                let _: i32 = tc.draw(crate::generators::integers::<i32>());
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

//! Ported from hypothesis-python/tests/cover/test_phases.py
//!
//! Tests asserting server-side phase enforcement (Reuse, Shrink, Generate
//! skipping) are gated on `#[cfg(feature = "native")]` because hegel-core
//! 0.6.1 does not yet forward the phases parameter to the engine.
// The `native` feature is defined in the DRMacIver/native branch, not here;
// suppress the unknown-feature warning so the tests compile cleanly.
#![allow(unexpected_cfgs)]

mod common;

use hegel::generators as gs;
use hegel::{Phase, Settings, TestCase};
#[cfg(feature = "native")]
use hegel::Hegel;

// With phases not including Explicit, explicit cases are skipped.
// The explicit case would fail at runtime (name mismatch: "hello_world" vs "b"),
// but it is never run because Phase::Explicit is not in the phase list.
#[hegel::test(test_cases = 5, phases = [Phase::Reuse, Phase::Generate])]
#[hegel::explicit_test_case(hello_world = "hello world".to_string())]
fn test_does_not_use_explicit_examples(tc: TestCase) {
    let b: bool = tc.draw(gs::booleans());
    let _ = b;
}

// Default phases include Explicit so that explicit_test_case attributes work
// without any phases configuration.
#[test]
fn test_default_phases_include_explicit() {
    let settings = Settings::new();
    assert!(settings.has_phase(Phase::Explicit));
    assert!(settings.has_phase(Phase::Reuse));
    assert!(settings.has_phase(Phase::Generate));
    assert!(settings.has_phase(Phase::Target));
    assert!(settings.has_phase(Phase::Shrink));
}

// When phases are overridden, only the specified phases are active.
#[test]
fn test_overriding_phases_excludes_others() {
    let settings = Settings::new().phases([Phase::Generate]);
    assert!(!settings.has_phase(Phase::Explicit));
    assert!(!settings.has_phase(Phase::Reuse));
    assert!(settings.has_phase(Phase::Generate));
    assert!(!settings.has_phase(Phase::Target));
    assert!(!settings.has_phase(Phase::Shrink));
}

// ── Phase behavior tests (native only; server doesn't yet enforce phases) ────

// With phases=[Explicit] only, only the explicit case runs.
// The body asserts i == 11, which would fail for any generated integer.
// The test passes because the generate phase is disabled.
// Port of test_only_runs_explicit_examples.
#[cfg(feature = "native")]
#[hegel::test(test_cases = 100, phases = [Phase::Explicit])]
#[hegel::explicit_test_case(i = 11i32)]
fn test_only_runs_explicit_examples(tc: TestCase) {
    let i: i32 = tc.draw(gs::integers());
    assert_eq!(i, 11);
}

// Without Phase::Generate, no test cases are generated.  A body that always
// panics never runs, so even a test that would always fail passes.
// Port of test_this_would_fail_if_you_ran_it.
#[cfg(feature = "native")]
#[test]
fn test_no_generate_means_body_never_runs() {
    Hegel::new(|tc: TestCase| {
        let _: bool = tc.draw(gs::booleans());
        panic!("generate phase is disabled; this body should never run");
    })
    .settings(
        Settings::new()
            .phases([Phase::Reuse, Phase::Shrink])
            .database(None),
    )
    .run();
}

// Without Phase::Shrink the shrinker is never invoked, so the test body is
// called at most twice: once for the initial failure discovery and once for
// the final replay to produce counterexample output.
#[cfg(feature = "native")]
#[test]
fn test_disabling_shrink_limits_interesting_calls() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let call_count = Arc::new(AtomicUsize::new(0));
    let count = call_count.clone();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        Hegel::new(move |tc: TestCase| {
            let _: bool = tc.draw(gs::booleans());
            count.fetch_add(1, Ordering::SeqCst);
            panic!("always fails");
        })
        .settings(
            Settings::new()
                .phases([Phase::Generate]) // Shrink excluded
                .database(None)
                .test_cases(100),
        )
        .run();
    }));

    assert!(result.is_err(), "expected the test to fail");
    let n = call_count.load(Ordering::SeqCst);
    // Without shrinking: at most initial discovery (1) + final replay (1) = 2.
    assert!(
        n <= 2,
        "expected at most 2 body calls without shrinking, got {n}"
    );
}

// Without Phase::Reuse a previously saved failing example is not replayed at
// the start of the next run.
// Port of test_does_not_reuse_saved_examples_if_reuse_not_in_phases.
#[cfg(feature = "native")]
#[test]
fn test_disabling_reuse_skips_saved_example() {
    use std::sync::{Arc, Mutex};

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("db");
    std::fs::create_dir_all(&db_path).unwrap();
    let db_str = db_path.to_str().unwrap().to_string();

    let drawn: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(Vec::new()));

    // Run 1: full phases — finds and shrinks the failing example (1_000_000),
    // which is saved to the database under the shared key.
    {
        let vals = drawn.clone();
        let db = db_str.clone();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            Hegel::new(move |tc: TestCase| {
                let n: i64 = tc.draw(gs::integers());
                vals.lock().unwrap().push(n);
                assert!(n < 1_000_000);
            })
            .settings(Settings::new().database(Some(db)))
            .__database_key("phase_reuse_test".to_string())
            .run();
        }));
    }

    let shrunk = *drawn.lock().unwrap().last().unwrap();
    assert_eq!(shrunk, 1_000_000);
    drawn.lock().unwrap().clear();

    // Run 2: phases=[Generate] (no Reuse) — the saved example is not fetched,
    // so the first drawn value is freshly generated rather than 1_000_000.
    {
        let vals = drawn.clone();
        let db = db_str.clone();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            Hegel::new(move |tc: TestCase| {
                let n: i64 = tc.draw(gs::integers());
                vals.lock().unwrap().push(n);
                assert!(n < 1_000_000);
            })
            .settings(
                Settings::new()
                    .phases([Phase::Generate])
                    .database(Some(db))
                    .derandomize(false),
            )
            .__database_key("phase_reuse_test".to_string())
            .run();
        }));
    }

    let first = drawn.lock().unwrap()[0];
    assert_ne!(
        first, shrunk,
        "Phase::Reuse is disabled but the saved example ({shrunk}) was \
         still replayed as the first drawn value"
    );
}

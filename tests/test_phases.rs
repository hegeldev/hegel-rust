//! Ported from hypothesis-python/tests/cover/test_phases.py

mod common;

use hegel::generators as gs;
use hegel::{Hegel, Phase, Settings, TestCase};

// With phases not including Explicit, explicit cases are skipped.
// The explicit case would fail at runtime (name mismatch: "hello_world" vs "b"),
// but it is never run because Phase::Explicit is not in the phase list.
#[hegel::test(test_cases = 5, phases = [Phase::Reuse, Phase::Generate])]
#[hegel::explicit_test_case(hello_world = "hello world".to_string())]
fn test_does_not_use_explicit_examples(tc: TestCase) {
    let b: bool = tc.draw(gs::booleans());
    let _ = b;
}

// With phases=[Explicit] only, only the explicit case runs.
// The body asserts i == 11, which would fail for any generated integer.
// The test passes because the generate phase is disabled.
#[hegel::test(test_cases = 100, phases = [Phase::Explicit])]
#[hegel::explicit_test_case(i = 11i32)]
fn test_only_runs_explicit_examples(tc: TestCase) {
    let i: i32 = tc.draw(gs::integers());
    assert_eq!(i, 11);
}

// Without Phase::Generate, no test cases are generated. A body that always
// panics never runs, so even a test that would always fail passes.
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
                .phases([Phase::Generate])
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

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

// Without Phase::Shrink the shrinker is never invoked.  The generate phase
// still produces multiple test cases, but the overall body-call count should
// stay well below `test_cases` because Hypothesis stops the generate phase
// early once it has enough interesting examples, and there are no shrink
// iterations on top.
#[test]
fn test_disabling_shrink_limits_interesting_calls() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let test_cases: u64 = 100;

    let call_count = Arc::new(AtomicUsize::new(0));
    let count = call_count.clone();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        Hegel::new(move |tc: TestCase| {
            let _: i64 = tc.draw(gs::integers::<i64>());
            count.fetch_add(1, Ordering::SeqCst);
            panic!("always fails");
        })
        .settings(
            Settings::new()
                .phases([Phase::Generate])
                .database(None)
                .test_cases(test_cases),
        )
        .run();
    }));

    assert!(result.is_err(), "expected the test to fail");
    let n = call_count.load(Ordering::SeqCst);
    // The generate phase runs some test cases, plus one final replay.
    // Without shrinking this should be much less than test_cases.
    assert!(
        n <= (test_cases / 2) as usize,
        "expected fewer than {half} body calls without shrinking, got {n}",
        half = test_cases / 2,
    );
}

// At the head of the Generate phase, Hypothesis's ConjectureRunner runs a
// deterministic all-simplest test case (engine.py:1147,
// `cached_test_function((ChoiceTemplate("simplest", count=None),))`). The
// native runner ports the same pre-trial. Verify behaviorally that the
// first test case sees every draw at its `simplest()` value — for an
// unbounded `gs::integers::<i32>()`, that's 0.
#[test]
fn test_generate_phase_runs_all_simplest_first() {
    use std::sync::Arc;
    use std::sync::Mutex;

    let draws: Arc<Mutex<Vec<i32>>> = Arc::new(Mutex::new(Vec::new()));
    let draws_clone = Arc::clone(&draws);

    Hegel::new(move |tc: TestCase| {
        let v: i32 = tc.draw(gs::integers::<i32>());
        draws_clone.lock().unwrap().push(v);
    })
    .settings(
        Settings::new()
            .phases([Phase::Generate])
            .database(None)
            .test_cases(5),
    )
    .run();

    let recorded = draws.lock().unwrap();
    assert!(!recorded.is_empty(), "no test cases ran");
    assert_eq!(
        recorded[0], 0,
        "first test case should be the all-simplest pre-trial (drawn value = 0), \
         got {} — pre-trial likely not running",
        recorded[0]
    );
}

#[test]
fn test_generate_phase_simplest_propagates_to_all_draws() {
    // The pre-trial forces every choice (not just the first) to simplest.
    // A test that draws five independent integers should see (0, 0, 0, 0, 0)
    // on its very first call.
    use std::sync::Arc;
    use std::sync::Mutex;

    type FirstDraws = Option<[i32; 5]>;
    let first_call: Arc<Mutex<FirstDraws>> = Arc::new(Mutex::new(None));
    let first_call_clone = Arc::clone(&first_call);

    Hegel::new(move |tc: TestCase| {
        let arr: [i32; 5] = [
            tc.draw(gs::integers::<i32>()),
            tc.draw(gs::integers::<i32>()),
            tc.draw(gs::integers::<i32>()),
            tc.draw(gs::integers::<i32>()),
            tc.draw(gs::integers::<i32>()),
        ];
        let mut slot = first_call_clone.lock().unwrap();
        if slot.is_none() {
            *slot = Some(arr);
        }
    })
    .settings(
        Settings::new()
            .phases([Phase::Generate])
            .database(None)
            .test_cases(3),
    )
    .run();

    let first = first_call.lock().unwrap().expect("no test cases ran");
    assert_eq!(
        first, [0; 5],
        "first test case should have every draw at simplest, got {first:?}"
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

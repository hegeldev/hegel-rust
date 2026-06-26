//! Ported from hypothesis-python/tests/cover/test_phases.py

mod common;

use hegel::generators as gs;
use hegel::{Hegel, Phase, Settings, TestCase};

#[hegel::test(test_cases = 5, phases = [Phase::Reuse, Phase::Generate])]
#[hegel::explicit_test_case(hello_world = "hello world".to_string())]
fn test_does_not_use_explicit_examples(tc: TestCase) {
    let b: bool = tc.draw(gs::booleans());
    let _ = b;
}

#[hegel::test(test_cases = 100, phases = [Phase::Explicit])]
#[hegel::explicit_test_case(i = 11i32)]
fn test_only_runs_explicit_examples(tc: TestCase) {
    let i: i32 = tc.draw(gs::integers());
    assert_eq!(i, 11);
}

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
    assert!(
        n <= (test_cases / 2) as usize,
        "expected fewer than {half} body calls without shrinking, got {n}",
        half = test_cases / 2,
    );
}

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

#[test]
fn test_default_phases_include_explicit() {
    let settings = Settings::new();
    assert!(settings.has_phase(Phase::Explicit));
    assert!(settings.has_phase(Phase::Reuse));
    assert!(settings.has_phase(Phase::Generate));
    assert!(settings.has_phase(Phase::Target));
    assert!(settings.has_phase(Phase::Shrink));
}

#[test]
fn test_overriding_phases_excludes_others() {
    let settings = Settings::new().phases([Phase::Generate]);
    assert!(!settings.has_phase(Phase::Explicit));
    assert!(!settings.has_phase(Phase::Reuse));
    assert!(settings.has_phase(Phase::Generate));
    assert!(!settings.has_phase(Phase::Target));
    assert!(!settings.has_phase(Phase::Shrink));
}

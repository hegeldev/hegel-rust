//! Tests for the `one_shot` setting, which runs a single test case in final
//! mode with no shrinking or replay.
//!
//! Requires hegel-core 0.4.4 or later (PR hegeldev/hegel-core#97).

use hegel::generators as gs;
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn one_shot_runs_exactly_one_test_case() {
    let count = Cell::new(0);

    hegel::Hegel::new(|tc| {
        let _ = tc.draw(gs::booleans());
        count.set(count.get() + 1);
    })
    .settings(hegel::Settings::new().one_shot(true).test_cases(100))
    .run();

    assert_eq!(count.get(), 1);
}

#[test]
fn one_shot_does_not_shrink_or_replay_on_failure() {
    static COUNT: AtomicUsize = AtomicUsize::new(0);

    let result = std::panic::catch_unwind(|| {
        hegel::Hegel::new(|tc| {
            let _ = tc.draw(gs::integers::<i32>().min_value(0).max_value(1_000_000_000));
            COUNT.fetch_add(1, Ordering::SeqCst);
            panic!("always fails");
        })
        .settings(hegel::Settings::new().one_shot(true))
        .run();
    });

    assert!(result.is_err(), "expected one-shot failure to panic");
    assert_eq!(
        COUNT.load(Ordering::SeqCst),
        1,
        "one_shot must not shrink or replay"
    );
}

#[test]
fn one_shot_runs_in_final_mode_so_note_is_emitted() {
    // In final mode, `note()` writes to stderr. We can't easily capture that
    // from within the test, but we can at least verify that the test runs
    // in final mode by calling `note()` — this exercises the is_last_run
    // branch of TestCase. Coverage of the actual stderr output is handled
    // via the end-to-end output tests.
    hegel::Hegel::new(|tc| {
        let x = tc.draw(gs::integers::<i32>());
        tc.note(&format!("x = {x}"));
    })
    .settings(hegel::Settings::new().one_shot(true))
    .run();
}

#[test]
fn one_shot_false_runs_normally() {
    let count = Cell::new(0);
    hegel::Hegel::new(|tc| {
        let _ = tc.draw(gs::integers::<i32>());
        count.set(count.get() + 1);
    })
    .settings(hegel::Settings::new().one_shot(false).test_cases(5))
    .run();
    assert_eq!(count.get(), 5);
}

/// The `#[hegel::test(one_shot = true)]` attribute form compiles and runs.
#[hegel::test(one_shot = true)]
fn attribute_form_with_one_shot(tc: hegel::TestCase) {
    let _ = tc.draw(gs::integers::<i32>());
}

#[test]
fn one_shot_can_use_full_generator_surface() {
    hegel::Hegel::new(|tc| {
        let xs: Vec<i32> = tc.draw(
            gs::vecs(gs::integers::<i32>().min_value(0).max_value(100))
                .min_size(1)
                .max_size(5),
        );
        assert!(!xs.is_empty());
        assert!(xs.len() <= 5);
        for x in xs {
            assert!((0..=100).contains(&x));
        }
    })
    .settings(hegel::Settings::new().one_shot(true))
    .run();
}

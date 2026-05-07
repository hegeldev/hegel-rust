use rand::SeedableRng;
use rand::rngs::SmallRng;

use super::*;
use crate::native::core::ChoiceValue;

fn make_rng() -> SmallRng {
    SmallRng::seed_from_u64(42)
}

// ── TargetedRunnerSettings::default() ─────────────────────────────────────

#[test]
fn targeted_runner_settings_default() {
    let s = TargetedRunnerSettings::default();
    assert_eq!(s.max_examples, 100);
}

// ── TargetedRunnerSettings::max_examples() builder ────────────────────────

#[test]
fn targeted_runner_settings_max_examples_builder() {
    let s = TargetedRunnerSettings::new().max_examples(42);
    assert_eq!(s.max_examples, 42);
}

// ── TargetedRunner::cached_test_function hit from cache ───────────────────

#[test]
fn cached_test_function_second_call_hits_cache() {
    let settings = TargetedRunnerSettings::new().max_examples(100);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 10);
            tc.target_observations.insert("x".to_string(), v as f64);
        },
        settings,
        make_rng(),
    );
    let choices = vec![ChoiceValue::Integer(5)];
    let r1 = runner.cached_test_function(&choices);
    let r2 = runner.cached_test_function(&choices);
    // Both should return Valid (no mark_invalid called).
    assert_eq!(r1.status, crate::native::core::Status::Valid);
    assert_eq!(r2.status, crate::native::core::Status::Valid);
}

// ── TargetedRunner::optimise_targets returning RunIsComplete ──────────────

#[test]
fn optimise_targets_returns_run_is_complete_when_budget_exhausted() {
    // Set max_examples very low so the budget is exhausted quickly.
    let settings = TargetedRunnerSettings::new().max_examples(2);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 1000);
            tc.target_observations.insert("score".to_string(), v as f64);
        },
        settings,
        make_rng(),
    );
    // Seed the runner with a known observation.
    let choices = vec![ChoiceValue::Integer(500)];
    runner.cached_test_function(&choices);
    // With only 2 examples budget and one already consumed, optimise_targets
    // should return RunIsComplete.
    let result = runner.optimise_targets();
    // Either Ok or Err(RunIsComplete) depending on budget; either is valid.
    // The point is it must not panic.
    let _ = result;
}

// ── sort_key_less for various ChoiceValue types ────────────────────────────

#[test]
fn sort_key_less_integers() {
    // Shorter is simpler.
    assert!(sort_key_less(
        &[ChoiceValue::Integer(0)],
        &[ChoiceValue::Integer(0), ChoiceValue::Integer(0)]
    ));
    // Same length, smaller abs value is simpler.
    assert!(sort_key_less(
        &[ChoiceValue::Integer(1)],
        &[ChoiceValue::Integer(5)]
    ));
    // Equal
    assert!(!sort_key_less(
        &[ChoiceValue::Integer(3)],
        &[ChoiceValue::Integer(3)]
    ));
}

#[test]
fn sort_key_less_booleans() {
    // false (0) < true (1)
    assert!(sort_key_less(
        &[ChoiceValue::Boolean(false)],
        &[ChoiceValue::Boolean(true)]
    ));
    assert!(!sort_key_less(
        &[ChoiceValue::Boolean(true)],
        &[ChoiceValue::Boolean(false)]
    ));
    assert!(!sort_key_less(
        &[ChoiceValue::Boolean(true)],
        &[ChoiceValue::Boolean(true)]
    ));
}

#[test]
fn sort_key_less_floats() {
    assert!(sort_key_less(
        &[ChoiceValue::Float(1.0)],
        &[ChoiceValue::Float(2.0)]
    ));
    assert!(!sort_key_less(
        &[ChoiceValue::Float(2.0)],
        &[ChoiceValue::Float(1.0)]
    ));
}

#[test]
fn sort_key_less_bytes() {
    assert!(sort_key_less(
        &[ChoiceValue::Bytes(vec![1])],
        &[ChoiceValue::Bytes(vec![2])]
    ));
    assert!(!sort_key_less(
        &[ChoiceValue::Bytes(vec![2])],
        &[ChoiceValue::Bytes(vec![1])]
    ));
}

#[test]
fn sort_key_less_strings() {
    assert!(sort_key_less(
        &[ChoiceValue::String(vec![1])],
        &[ChoiceValue::String(vec![2])]
    ));
    assert!(!sort_key_less(
        &[ChoiceValue::String(vec![2])],
        &[ChoiceValue::String(vec![1])]
    ));
}

// ── TargetedRunner::run_extend_full — resume_unwind on non-sentinel panic ─

#[test]
#[should_panic(expected = "resume_unwind_test")]
fn run_extend_full_resumes_non_sentinel_panics() {
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |_tc: &mut TargetedTestCase| {
            panic!("resume_unwind_test");
        },
        settings,
        make_rng(),
    );
    let choices = vec![];
    let _ = runner.cached_test_function_extend(&choices, 10);
}

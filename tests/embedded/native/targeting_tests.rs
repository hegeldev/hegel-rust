//! Embedded tests for `src/native/targeting.rs`.
//!
//! These exercise the defensive return paths that aren't reliably reachable
//! from the end-to-end production path (where budget is rarely exactly
//! exhausted at the precise moment a defensive guard fires). Each test sets
//! up `TargetingState` + `OptimiseCtx` directly and drives the helper under
//! test in isolation.

use super::*;
use crate::native::core::ChoiceValue;
use crate::native::tree::CachedTestFunction;

fn ctx_with_budget<'a>(
    result: &'a mut Option<Vec<crate::native::core::ChoiceNode>>,
    calls: &'a mut u64,
    valid_test_cases: &'a mut u64,
    max_valid: u64,
    max_calls: u64,
) -> OptimiseCtx<'a> {
    OptimiseCtx {
        result,
        calls,
        valid_test_cases,
        max_valid,
        max_calls,
    }
}

// ── run_trial: returns None when the budget is already exhausted on entry.

#[test]
fn run_trial_returns_none_when_budget_exhausted_on_entry() {
    let mut ctf = CachedTestFunction::new(|_tc: crate::TestCase| {
        panic!("test body must not run when budget is already exhausted");
    });
    let mut targeting = TargetingState::new();
    let mut result = None;
    // calls == max_calls trips budget_exhausted before the test runs.
    let mut calls: u64 = 100;
    let mut valid: u64 = 0;
    let mut ctx = ctx_with_budget(&mut result, &mut calls, &mut valid, 1000, 100);

    let trial = run_trial(
        &mut ctf,
        &mut targeting,
        &mut ctx,
        &[ChoiceValue::Integer(0)],
    );
    assert!(trial.is_none());
    // The test body wasn't invoked, so the calls counter didn't advance.
    assert_eq!(calls, 100);
}

// ── hill_climb: run_trial returns None for the start_choices replay,
//    so hill_climb returns 0 immediately.

#[test]
fn hill_climb_returns_zero_when_initial_replay_runs_out_of_budget() {
    let mut ctf = CachedTestFunction::new(|_tc: crate::TestCase| {});
    let mut targeting = TargetingState::new();
    targeting.best_observed_targets.insert("score".into(), 1.0);
    targeting
        .best_choices_for_target
        .insert("score".into(), vec![ChoiceValue::Integer(0)]);
    let mut result = None;
    let mut calls: u64 = 50;
    let mut valid: u64 = 0;
    let mut ctx = ctx_with_budget(&mut result, &mut calls, &mut valid, 1000, 50);

    let imps = hill_climb(&mut ctf, &mut targeting, &mut ctx, "score", 10);
    assert_eq!(imps, 0);
    assert_eq!(calls, 50);
}

// ── hill_climb: status < Valid on the start_choices replay returns 0.

#[test]
fn hill_climb_returns_zero_when_start_replay_is_invalid() {
    use crate::test_case::ASSUME_FAIL_STRING;
    let mut ctf = CachedTestFunction::new(|_tc: crate::TestCase| {
        // Invalid status (filtered out).
        panic!("{}", ASSUME_FAIL_STRING);
    });
    let mut targeting = TargetingState::new();
    targeting.best_observed_targets.insert("score".into(), 1.0);
    targeting
        .best_choices_for_target
        .insert("score".into(), vec![ChoiceValue::Integer(0)]);
    let mut result = None;
    let mut calls: u64 = 0;
    let mut valid: u64 = 0;
    let mut ctx = ctx_with_budget(&mut result, &mut calls, &mut valid, 100, 1000);

    let imps = hill_climb(&mut ctf, &mut targeting, &mut ctx, "score", 10);
    assert_eq!(imps, 0);
    // The replay ran but produced Invalid → valid_test_cases unchanged.
    assert_eq!(valid, 0);
    assert_eq!(calls, 1);
}

// ── try_replace: run_trial returns None mid-call → returns false.
//
// We arrange for `find_integer` to enter `try_replace`, which calls
// `run_trial`. If the budget gets exhausted between the start_choices
// replay (consumed inside hill_climb) and the first probe, run_trial
// returns None and try_replace must return false. We achieve that by
// setting `max_calls` to one above the start of the test: the start
// replay uses the remaining call, then the probe trips the guard.

#[test]
fn try_replace_returns_false_when_run_trial_runs_out_of_budget() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let observed_calls = Arc::new(AtomicUsize::new(0));
    let observed_clone = observed_calls.clone();
    let mut ctf = CachedTestFunction::new(move |tc: crate::TestCase| {
        observed_clone.fetch_add(1, Ordering::SeqCst);
        // Draw an integer that the target_observation will reference.
        use crate::generators as gs;
        let _v: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
    });
    let mut targeting = TargetingState::new();
    targeting.best_observed_targets.insert("score".into(), 1.0);
    targeting
        .best_choices_for_target
        .insert("score".into(), vec![ChoiceValue::Integer(50)]);
    let mut result = None;
    // max_calls = 1 → after start replay runs (and consumes the 1 call),
    // any try_replace probe will see budget_exhausted in run_trial.
    let mut calls: u64 = 0;
    let mut valid: u64 = 0;
    let mut ctx = ctx_with_budget(&mut result, &mut calls, &mut valid, 1000, 1);

    let imps = hill_climb(&mut ctf, &mut targeting, &mut ctx, "score", 10);
    // No improvements possible: the very first probe inside try_replace hits
    // the budget guard and returns None → try_replace returns false → linear
    // scan exits immediately.
    assert_eq!(imps, 0);
    assert_eq!(calls, 1);
    assert_eq!(observed_calls.load(Ordering::SeqCst), 1);
}

// ── find_integer: hi > (1 << 20) cap fires.
//
// Use a target whose score is monotone in the integer choice over a huge
// range. Each successful try_replace doubles `hi` until it exceeds 1<<20,
// at which point the inner loop returns.

#[test]
fn find_integer_hi_cap_fires() {
    use crate::generators as gs;
    let mut ctf = CachedTestFunction::new(|tc: crate::TestCase| {
        let v: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1 << 30));
        let label = "score".to_string();
        let score = v as f64;
        // Tunnel score through tc.target so the data source records it.
        tc.target_labelled(score, label);
    });
    let mut targeting = TargetingState::new();
    // Seed: choice = 0, so there's plenty of headroom to double `hi` upward.
    targeting.best_observed_targets.insert("score".into(), 0.0);
    targeting
        .best_choices_for_target
        .insert("score".into(), vec![ChoiceValue::Integer(0)]);
    let mut result = None;
    let mut calls: u64 = 0;
    let mut valid: u64 = 0;
    let mut ctx = ctx_with_budget(&mut result, &mut calls, &mut valid, 1000, 1000);

    optimise_targets(&mut ctf, &mut targeting, &mut ctx);
    // Past the cap we stop probing, but the climb should have advanced the
    // best score significantly past 1<<20.
    let best = targeting
        .best_observed_targets
        .get("score")
        .copied()
        .unwrap();
    assert!(
        best > (1u64 << 20) as f64,
        "expected best > 2^20, got {}",
        best
    );
}

// ── find_integer: improvements >= max_improvements check inside binary search.
//
// To enter the binary-search phase we need the exponential loop to `break`
// (not hit the hi-cap and not exceed max_improvements), and then we need a
// binary-search probe to succeed, pushing `improvements` to the cap so the
// next iteration's `improvements >= max_improvements` check fires. The
// score function is `v if v <= 80 else 0`, integer range 0..=200, the
// climber starts at cur=0, and `max_improvements = 8`. Tracing:
//   linear k=1..4 → cur=1,3,6,10,  improvements=4
//   exp k=5       → cur=15,        improvements=5;  lo=5, hi=10
//   exp k=10      → cur=25,        improvements=6;  lo=10, hi=20
//   exp k=20      → cur=45,        improvements=7;  lo=20, hi=40
//   exp k=40      → cur+40=85 > 80, score=0, break; lo=20, hi=40
//   binary mid=30 → cur+30=75 ≤ 80, score=75, accept; cur=75, improvements=8; lo=30
//   binary check  → improvements(8) >= 8 → return at line 273.

#[test]
fn find_integer_max_improvements_check_in_binary_search() {
    use crate::generators as gs;
    let mut ctf = CachedTestFunction::new(|tc: crate::TestCase| {
        let v: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(200));
        let score = if v <= 80 { v as f64 } else { 0.0 };
        tc.target_labelled(score, "score");
    });
    let mut targeting = TargetingState::new();
    targeting.best_observed_targets.insert("score".into(), 0.0);
    targeting
        .best_choices_for_target
        .insert("score".into(), vec![ChoiceValue::Integer(0)]);
    let mut result = None;
    let mut calls: u64 = 0;
    let mut valid: u64 = 0;
    let mut ctx = ctx_with_budget(&mut result, &mut calls, &mut valid, 1000, 1000);

    let _ = hill_climb(&mut ctf, &mut targeting, &mut ctx, "score", 8);
    // After hitting the cap inside binary, the climb should have committed
    // to a value >= 75 (the binary mid that succeeded).
    let best = targeting
        .best_observed_targets
        .get("score")
        .copied()
        .unwrap();
    assert!(best >= 75.0, "expected best >= 75, got {}", best);
}

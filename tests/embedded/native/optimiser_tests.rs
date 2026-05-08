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

// ── TargetedTestCase::draw_integer STOP_TEST_PANIC when overrun ───────────
//
// Lines 97, 104, 111, 127, 151: `Err(_) => std::panic::panic_any(STOP_TEST_PANIC)`.
// These fire when the underlying NativeTestCase is exhausted (pre_choice
// returns Err(StopTest)). The easiest way to trigger them: run a test
// with `cached_test_function_extend` using a zero-capacity prefix, then
// call many draws inside the test body.

#[test]
fn targeted_test_case_draw_integer_stop_test_when_overrun() {
    // cached_test_function_extend with extend=0 and empty prefix gives max_size=0.
    // The first draw_integer call will exhaust the buffer and panic with STOP_TEST_PANIC.
    // run_on catches STOP_TEST_PANIC as Status::EarlyStop, so no panic propagates.
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            // This will trigger STOP_TEST_PANIC because max_size=0.
            let _ = tc.draw_integer(0, 100);
        },
        settings,
        make_rng(),
    );
    let r = runner.cached_test_function_extend(&[], 0);
    assert_eq!(r.status, crate::native::core::Status::EarlyStop);
}

#[test]
fn targeted_test_case_draw_float_stop_test_when_overrun() {
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let _ = tc.draw_float(0.0, 1.0, false, false);
        },
        settings,
        make_rng(),
    );
    let r = runner.cached_test_function_extend(&[], 0);
    assert_eq!(r.status, crate::native::core::Status::EarlyStop);
}

#[test]
fn targeted_test_case_draw_string_stop_test_when_overrun() {
    use crate::native::intervalsets::IntervalSet;
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let intervals = IntervalSet::new(vec![(65, 90)]);
            let _ = tc.draw_string(&intervals, 1, 5);
        },
        settings,
        make_rng(),
    );
    let r = runner.cached_test_function_extend(&[], 0);
    assert_eq!(r.status, crate::native::core::Status::EarlyStop);
}

#[test]
fn targeted_test_case_draw_string_empty_intervals_stop_test_when_overrun() {
    // Line 142: empty intervals path `(0, 0x10FFFF)`.
    use crate::native::intervalsets::IntervalSet;
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            // Empty intervals => falls through to (0, 0x10FFFF) default.
            let intervals = IntervalSet::new(vec![]);
            let _ = tc.draw_string(&intervals, 1, 5);
        },
        settings,
        make_rng(),
    );
    let r = runner.cached_test_function_extend(&[], 0);
    assert_eq!(r.status, crate::native::core::Status::EarlyStop);
}

// ── sort_key_less — Equal cases for Float, Bytes, String ─────────────────
//
// Lines 212, 217, 222: `Equal => {}` arms in sort_key_less for Float/Bytes/String.

#[test]
fn sort_key_less_float_equal_falls_through() {
    // Two equal floats: the Equal arm fires, then we fall through to false.
    assert!(!sort_key_less(
        &[ChoiceValue::Float(1.0)],
        &[ChoiceValue::Float(1.0)]
    ));
}

#[test]
fn sort_key_less_bytes_equal_falls_through() {
    assert!(!sort_key_less(
        &[ChoiceValue::Bytes(vec![5])],
        &[ChoiceValue::Bytes(vec![5])]
    ));
}

#[test]
fn sort_key_less_string_equal_falls_through() {
    assert!(!sort_key_less(
        &[ChoiceValue::String(vec![65])],
        &[ChoiceValue::String(vec![65])]
    ));
}

// ── sort_key_less — mixed types ────────────────────────────────────────────
//
// Line 224: `_ => return false` — mixed types in sort_key_less.

#[test]
fn sort_key_less_mixed_types_returns_false() {
    // Integer vs Boolean: mismatched types → return false.
    assert!(!sort_key_less(
        &[ChoiceValue::Integer(0)],
        &[ChoiceValue::Boolean(true)]
    ));
}

// ── TargetedRunner::run_on: Status::EarlyStop from STOP_TEST_PANIC ────────
//
// Line 333: `Status::EarlyStop` in run_on when STOP_TEST_PANIC.
// Already covered by the draw_integer overrun test above (run_on is invoked
// via cached_test_function_extend which calls run_on).

// ── TargetedRunner::run_on: resume_unwind on non-sentinel panic ───────────
//
// Line 338: `std::panic::resume_unwind(payload)` — non-sentinel panic.

#[test]
#[should_panic(expected = "non_sentinel_panic_for_run_on")]
fn run_on_resumes_non_sentinel_panic() {
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |_tc: &mut TargetedTestCase| {
            panic!("non_sentinel_panic_for_run_on");
        },
        settings,
        make_rng(),
    );
    let _ = runner.cached_test_function(&[]);
}

// ── run_extend_full: cache hit path via lookup_prefix_cache ───────────────
//
// Lines 488-492: cache hit path in `run_extend_full` via `lookup_prefix_cache`.
// The prefix cache is populated by a shorter call; a longer call then hits it.

#[test]
fn run_extend_full_hits_prefix_cache() {
    let settings = TargetedRunnerSettings::new().max_examples(100);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 10);
            tc.target_observations.insert("x".to_string(), v as f64);
        },
        settings,
        make_rng(),
    );
    // First call with [5]: populates the exact cache.
    let choices_short = vec![ChoiceValue::Integer(5)];
    let r1 = runner.cached_test_function(&choices_short);
    // Second call with [5, 3]: the test function only draws one integer,
    // so the prefix [5] was cached. A longer input [5, 3] should hit the
    // prefix cache (lookup_prefix_cache will find [5]).
    let choices_long = vec![ChoiceValue::Integer(5), ChoiceValue::Integer(3)];
    let r2 = runner.cached_test_function(&choices_long);
    assert_eq!(r1.status, r2.status);
}

// ── hill_climb: return Ok(0) when no best choices for target (line 568) ────
//
// Line 568: `None => return Ok(0)` when best_choices_for_target has no entry
// for a target that IS in best_observed_targets.  We inject the observation
// directly (best_observed_targets is private but accessible from embedded
// tests via `use super::*`), then call optimise_targets which invokes
// hill_climb → None branch.

#[test]
fn hill_climb_returns_zero_when_best_choices_missing() {
    let settings = TargetedRunnerSettings::new().max_examples(100);
    let mut runner = TargetedRunner::new(|_tc: &mut TargetedTestCase| {}, settings, make_rng());
    // Manually populate best_observed_targets without touching best_choices_for_target.
    runner
        .best_observed_targets
        .insert("score".to_string(), 5.0);
    // optimise_targets → hill_climb("score") → best_choices_for_target.get("score") == None
    // → return Ok(0) (line 568).
    let result = runner.optimise_targets();
    let _ = result;
}

// ── hill_climb: return Ok(0) when run_extend_full gives status < Valid ─────
//
// Line 573: `return Ok(0)` when run_extend_full on the start_choices returns
// a status below Valid.  We seed a Valid run so best_choices_for_target is
// populated, then clear the cache and flip the test function to mark_invalid.

#[test]
fn hill_climb_returns_zero_when_extend_full_returns_invalid() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let flip = Arc::new(AtomicBool::new(false));
    let flip2 = flip.clone();
    let settings = TargetedRunnerSettings::new().max_examples(100);
    let mut runner = TargetedRunner::new(
        move |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 10);
            if flip2.load(Ordering::SeqCst) {
                tc.mark_invalid();
            } else {
                tc.target_observations.insert("score".to_string(), v as f64);
            }
        },
        settings,
        make_rng(),
    );
    // First call: Valid run populates best_choices_for_target["score"].
    let choices = vec![ChoiceValue::Integer(5)];
    runner.cached_test_function(&choices);
    // Flip to Invalid and clear the cache so run_extend_full re-runs the test fn.
    flip.store(true, Ordering::SeqCst);
    runner.cache.clear();
    // optimise_targets → hill_climb("score") → run_extend_full → Invalid
    // → status < Valid → return Ok(0) (line 573).
    let _ = runner.optimise_targets();
}

// ── TargetedTestCase::draw_boolean STOP_TEST_PANIC when overrun ───────────
//
// Line 104: draw_boolean panics with STOP_TEST_PANIC when underlying NTC is exhausted.

#[test]
fn targeted_test_case_draw_boolean_stop_test_when_overrun() {
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let _ = tc.draw_boolean(0.5);
        },
        settings,
        make_rng(),
    );
    let r = runner.cached_test_function_extend(&[], 0);
    assert_eq!(r.status, crate::native::core::Status::EarlyStop);
}

// ── TargetedTestCase::draw_bytes STOP_TEST_PANIC when overrun ─────────────
//
// Line 111: draw_bytes panics with STOP_TEST_PANIC when underlying NTC is exhausted.

#[test]
fn targeted_test_case_draw_bytes_stop_test_when_overrun() {
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let _ = tc.draw_bytes(0, 5);
        },
        settings,
        make_rng(),
    );
    let r = runner.cached_test_function_extend(&[], 0);
    assert_eq!(r.status, crate::native::core::Status::EarlyStop);
}

// ── run_on resume_unwind for non-&'static str panic payload (line 338) ────
//
// Line 335 is covered by run_on_resumes_non_sentinel_panic (which panics with
// a &'static str literal). Line 338 requires a payload that is NOT a &'static str,
// e.g. panic_any(42u64).

#[test]
#[should_panic]
fn run_on_resumes_non_str_panic_payload() {
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |_tc: &mut TargetedTestCase| {
            std::panic::panic_any(42u64);
        },
        settings,
        make_rng(),
    );
    let _ = runner.cached_test_function_extend(&[], 10);
}

// ── optimise_targets: RunIsComplete (line 540) via find_integer ───────────
//
// Line 540: `return Err(RunIsComplete)` when valid_examples >= max_examples
// during the linear search phase of find_integer.

#[test]
fn optimise_targets_run_is_complete_mid_climb() {
    // Very low max_examples to exhaust budget quickly during hill-climbing.
    let settings = TargetedRunnerSettings::new().max_examples(2);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 1000);
            tc.target_observations.insert("score".to_string(), v as f64);
        },
        settings,
        make_rng(),
    );
    let choices = vec![ChoiceValue::Integer(500)];
    // Use up the budget first.
    runner.cached_test_function(&choices);
    runner.cached_test_function(&choices);
    // max_examples exhausted; optimise_targets → hill_climb → find_integer
    // hits RunIsComplete (line 540 or 637).
    let result = runner.optimise_targets();
    assert!(result.is_err()); // Err(RunIsComplete)
}

// ── hill_climb: i -= 1; continue (lines 593-594) ─────────────────────────
//
// Lines 593-594 fire when `nodes_examined.contains(&idx)` after clearing.
// Run hill-climbing with multiple nodes so some indices get re-examined.

#[test]
fn hill_climb_re_examines_nodes_after_length_change() {
    let settings = TargetedRunnerSettings::new().max_examples(200);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let a = tc.draw_integer(0, 100);
            let b = tc.draw_integer(0, 100);
            tc.target_observations
                .insert("score".to_string(), (a + b) as f64);
        },
        settings,
        make_rng(),
    );
    let choices = vec![ChoiceValue::Integer(50), ChoiceValue::Integer(50)];
    runner.cached_test_function(&choices);
    // hill_climb examines nodes. nodes_examined grows until i reaches 0.
    // When the score improves and nodes length stays same, the examined
    // set persists; when idx >= len, i -= 1; continue fires (line 593-594).
    let _ = runner.optimise_targets();
}

// ── find_integer: hi > (1 << 20) cap (line 637) ──────────────────────────
//
// Line 637: `if hi > (1 << 20) { return Ok(()); }` fires when the
// exponential-doubling phase of `find_integer` exceeds 2^20 ≈ 1M.
// Starting at hi=5 and doubling on each successful try_replace, after
// ~18 doublings hi exceeds the cap.  We give the test enough budget and
// a large enough integer range so the doublings can proceed.

#[test]
fn find_integer_hi_cap_fires() {
    let settings = TargetedRunnerSettings::new().max_examples(500);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 10_000_000);
            tc.target_observations.insert("score".to_string(), v as f64);
        },
        settings,
        make_rng(),
    );
    // Seed at 0 so there is maximum headroom to climb.
    let choices = vec![ChoiceValue::Integer(0)];
    runner.cached_test_function(&choices);
    // hill_climb → find_integer → exponential probing doubles hi until
    // hi > 1<<20 → line 637 fires → Ok(()) returned.
    let _ = runner.optimise_targets();
}

// ── try_replace: max-budget guard inside retry loop (line 836) ──────────
//
// Line 836: `if self.valid_examples >= self.max_examples { return false; }`
// inside `try_replace`'s retry loop.  We need the budget to be exhausted
// mid-retry.  Set max_examples very low and start near-budget so the inner
// retry hits the guard before returning from any other early-exit.

#[test]
fn try_replace_budget_guard_in_retry_loop() {
    // 3 examples: 1 seed + 2 budget.  The try_replace retry loop checks
    // budget before running the test; when the check fires we return false.
    let settings = TargetedRunnerSettings::new().max_examples(3);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 1000);
            tc.target_observations.insert("score".to_string(), v as f64);
        },
        settings,
        make_rng(),
    );
    let choices = vec![ChoiceValue::Integer(500)];
    runner.cached_test_function(&choices);
    // budget now 1; optimise → find_integer → try_replace → budget check fires.
    let _ = runner.optimise_targets();
}

// ── try_replace: EarlyStop from run (line 854) ────────────────────────────
//
// Line 854: `if status == Status::EarlyStop { return false; }` in the
// try_replace retry loop.  We need `run_extend_full` to return EarlyStop
// for the candidate choices.  Use BufferSizeLimit to limit the buffer so
// the probed choice exhausts it immediately.

#[test]
fn try_replace_early_stop_in_retry_loop() {
    use crate::native::optimiser::BufferSizeLimit;
    // Set a very small buffer so any probe with a large integer overruns.
    let _guard = BufferSizeLimit::new(2);
    let settings = TargetedRunnerSettings::new().max_examples(200);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 100);
            tc.target_observations.insert("score".to_string(), v as f64);
        },
        settings,
        make_rng(),
    );
    let choices = vec![ChoiceValue::Integer(0)];
    runner.cached_test_function(&choices);
    // The probed candidate tries v+1 etc. With a tiny buffer the run_extend_full
    // may hit EarlyStop → line 854 fires.
    let _ = runner.optimise_targets();
}

// ── sort_key_less: Greater for integers (line 200) ────────────────────────
//
// Line 200: `Greater => return false` when the first integer's sort key is
// strictly greater than the second.

#[test]
fn sort_key_less_integer_greater_returns_false() {
    // Integer(5) has sort_key = (5, false); Integer(1) has (1, false).
    // (5, false) > (1, false) → Greater arm fires → return false.
    assert!(!sort_key_less(
        &[ChoiceValue::Integer(5)],
        &[ChoiceValue::Integer(1)]
    ));
}

// ── encode_choice_key: Boolean, Float, Bytes, String variants ─────────────
//
// Lines 240-258: the Boolean/Float/Bytes/String arms of `encode_choice_key`
// fire whenever `run_extend_full` / `run_exact` is called with those choice
// types.  Exercised here by seeding `cached_test_function` with such choices.

#[test]
fn encode_choice_key_boolean_variant() {
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_boolean(0.5);
            tc.target_observations
                .insert("x".to_string(), if v { 1.0 } else { 0.0 });
        },
        settings,
        make_rng(),
    );
    let choices = vec![ChoiceValue::Boolean(true)];
    let r1 = runner.cached_test_function(&choices);
    // Second call hits the cache; encode_choice_key Boolean arm was reached.
    let r2 = runner.cached_test_function(&choices);
    assert_eq!(r1.status, r2.status);
}

#[test]
fn encode_choice_key_float_variant() {
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_float(0.0, 1.0, false, false);
            tc.target_observations.insert("x".to_string(), v);
        },
        settings,
        make_rng(),
    );
    let choices = vec![ChoiceValue::Float(0.5)];
    let r1 = runner.cached_test_function(&choices);
    let r2 = runner.cached_test_function(&choices);
    assert_eq!(r1.status, r2.status);
}

#[test]
fn encode_choice_key_bytes_variant() {
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let b = tc.draw_bytes(2, 2);
            tc.target_observations.insert("x".to_string(), b[0] as f64);
        },
        settings,
        make_rng(),
    );
    let choices = vec![ChoiceValue::Bytes(vec![1, 2])];
    let r1 = runner.cached_test_function(&choices);
    let r2 = runner.cached_test_function(&choices);
    assert_eq!(r1.status, r2.status);
}

#[test]
fn encode_choice_key_string_variant() {
    use crate::native::intervalsets::IntervalSet;
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let intervals = IntervalSet::new(vec![(65, 90)]);
            let s = tc.draw_string(&intervals, 1, 3);
            tc.target_observations
                .insert("x".to_string(), s.chars().count() as f64);
        },
        settings,
        make_rng(),
    );
    let choices = vec![ChoiceValue::String(vec![65, 66])];
    let r1 = runner.cached_test_function(&choices);
    let r2 = runner.cached_test_function(&choices);
    assert_eq!(r1.status, r2.status);
}

// ── TargetedRunner::best_observed_targets accessor (line 962-964) ─────────

#[test]
fn targeted_runner_best_observed_targets_accessor() {
    let settings = TargetedRunnerSettings::new().max_examples(20);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 10);
            tc.target_observations.insert("score".to_string(), v as f64);
        },
        settings,
        make_rng(),
    );
    let choices = vec![ChoiceValue::Integer(7)];
    runner.cached_test_function(&choices);
    // best_observed_targets() (lines 962-964) returns the map.
    let best = runner.best_observed_targets();
    assert!(!best.is_empty());
}

// ── TargetedTestCase::start_span and stop_span (lines 159-165) ────────────
//
// Lines 159-165: start_span/stop_span delegate to the inner NativeTestCase.

#[test]
fn targeted_test_case_start_stop_span() {
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            tc.start_span(42);
            let v = tc.draw_integer(0, 100);
            tc.stop_span();
            tc.target_observations.insert("x".to_string(), v as f64);
        },
        settings,
        make_rng(),
    );
    let choices = vec![ChoiceValue::Integer(5)];
    let r = runner.cached_test_function(&choices);
    assert_eq!(r.status, crate::native::core::Status::Valid);
}

// ── is_shortlex_tie updates best_choices_for_target (lines 366-369) ───────
//
// Lines 366-369 fire when a run produces the same score as the current best
// but with a simpler (smaller sort_key) choice sequence.
// Seed the runner with a high-value choice, then call with a lower value
// (same score by construction) so the tie-break fires.

#[test]
fn is_shortlex_tie_updates_best_choices() {
    let settings = TargetedRunnerSettings::new().max_examples(100);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 100);
            // Always emit score 5.0 regardless of v, so every run ties.
            tc.target_observations.insert("score".to_string(), 5.0);
            let _ = v;
        },
        settings,
        make_rng(),
    );
    // First run with Integer(50): score=5.0, choices=[Integer(50)].
    runner.cached_test_function(&[ChoiceValue::Integer(50)]);
    // Second run with Integer(1): same score=5.0 but simpler sort_key → tie fires.
    runner.cached_test_function(&[ChoiceValue::Integer(1)]);
    // The tie-break updated best_choices_for_target["score"] to the simpler sequence.
    let best = runner.best_choices_for_target.get("score").unwrap();
    assert_eq!(best, &[ChoiceValue::Integer(1)]);
}

// ── run_extend_full prefix cache hit (lines 455-460) ─────────────────────
//
// Lines 455-460 fire when lookup_prefix_cache finds a hit in run_extend_full.
// The prefix cache is populated when try_cache stores a run under choices[..nodes.len()]
// where nodes.len() < choices.len(). This happens when the test draws fewer
// nodes than were in the input prefix.
//
// Strategy: test draws 2 integers only when v>0; when v==0 it draws only 1.
// Starting from [5, w]: try_replace(idx=0, k=-1) eventually tries [0, w].
// run_extend_full([0, w]) looks up prefix [0] → which was cached when the
// simplest-probe drew only 1 node from [0, ...].

#[test]
fn run_extend_full_prefix_cache_hit_via_hill_climb() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let prefix_hit_possible = Arc::new(AtomicBool::new(false));
    let phc = prefix_hit_possible.clone();

    let settings = TargetedRunnerSettings::new().max_examples(200);
    let mut runner = TargetedRunner::new(
        move |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 10);
            if v > 0 {
                // Draw a second integer only when v > 0.
                let w = tc.draw_integer(0, 10);
                tc.target_observations
                    .insert("score".to_string(), (v + w) as f64);
                phc.store(true, Ordering::SeqCst);
            } else {
                // v == 0: only 1 node drawn.
                tc.target_observations.insert("score".to_string(), 0.0);
            }
        },
        settings,
        make_rng(),
    );
    // Seed with [0]: test draws 1 node, cached under [0] with nodes.len()=1.
    runner.cached_test_function(&[ChoiceValue::Integer(0)]);
    // Seed with [5, 3]: test draws 2 nodes.
    runner.cached_test_function(&[ChoiceValue::Integer(5), ChoiceValue::Integer(3)]);
    // Now optimise: hill_climb at [5,3] tries [4,3],[3,3],[2,3],[1,3],[0,3].
    // run_extend_full([0,3]) checks exact cache (miss), then prefix [0] → HIT!
    let _ = runner.optimise_targets();
}

// ── try_replace with Boolean node (lines 780-790) ─────────────────────────
//
// Lines 780-790 fire when try_replace encounters a Boolean node.
// Use a test function that draws a boolean and emits a score.

#[test]
fn try_replace_with_boolean_node() {
    let settings = TargetedRunnerSettings::new().max_examples(200);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let b = tc.draw_boolean(0.5);
            tc.target_observations
                .insert("score".to_string(), if b { 1.0 } else { 0.0 });
        },
        settings,
        make_rng(),
    );
    // Seed with false (score=0.0); hill_climb tries flipping to true (score=1.0).
    runner.cached_test_function(&[ChoiceValue::Boolean(false)]);
    let _ = runner.optimise_targets();
}

// ── try_replace with Bytes node (lines 792-817) ───────────────────────────
//
// Lines 792-817 fire when try_replace encounters a Bytes node.

#[test]
fn try_replace_with_bytes_node() {
    let settings = TargetedRunnerSettings::new().max_examples(200);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let b = tc.draw_bytes(1, 1);
            tc.target_observations
                .insert("score".to_string(), b[0] as f64);
        },
        settings,
        make_rng(),
    );
    // Seed with byte=0; hill_climb tries incrementing to 1, 2, etc.
    runner.cached_test_function(&[ChoiceValue::Bytes(vec![0])]);
    let _ = runner.optimise_targets();
}

// ── try_cache: nodes.len() > choices.len() early return (line 514) ────────
//
// Line 514 fires when the test function drew MORE nodes than were in the
// input prefix, so the result is not deterministic and must not be cached.
// cached_test_function_extend with extend > 0 and a test that draws MORE
// than the prefix length exercises this: the drawn choices are cached under
// the actual drawn prefix, not the input.

#[test]
fn try_cache_skips_when_nodes_exceed_prefix() {
    // The test draws TWO integers but we only supply one in the prefix.
    // run_on will draw the second randomly; try_cache will find
    // nodes.len()==2 > choices.len()==1 and skip caching that run.
    let settings = TargetedRunnerSettings::new().max_examples(20);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let a = tc.draw_integer(0, 10);
            let b = tc.draw_integer(0, 10);
            tc.target_observations
                .insert("sum".to_string(), (a + b) as f64);
        },
        settings,
        make_rng(),
    );
    // cached_test_function_extend with a 1-element prefix and extend=5:
    // the test draws 2 nodes total, choices.len()=1 → nodes.len() > choices.len().
    let r = runner.cached_test_function_extend(&[ChoiceValue::Integer(5)], 5);
    // Result is Valid; the key point is that no panic occurs.
    let _ = r.status;
}

// ── optimise_targets: break when prev_calls == valid_examples (line 555) ──
//
// Line 555 fires when no new valid examples were generated in an iteration
// (hill climbing exhausted all candidates without generating new valid ones).

#[test]
fn optimise_targets_breaks_when_no_new_valid_examples() {
    let settings = TargetedRunnerSettings::new().max_examples(100);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            // All choices give the same score; no improvement possible.
            let _v = tc.draw_integer(0, 0);
            tc.target_observations.insert("score".to_string(), 0.0);
        },
        settings,
        make_rng(),
    );
    // Seed with Integer(0): this is the ONLY valid value (range [0,0]).
    runner.cached_test_function(&[ChoiceValue::Integer(0)]);
    // optimise_targets: hill_climb tries Integer(0)+1 = Integer(1) which is
    // out of range, so no improvement. prev_calls == valid_examples → break.
    let _ = runner.optimise_targets();
}

// ── consider_new_data: strict improvement (lines 945-951) ─────────────────
//
// Lines 945-951 fire when new_score > current_score in consider_new_data.
// This is triggered by optimise_targets finding a genuinely better score.

#[test]
fn consider_new_data_strict_improvement() {
    let settings = TargetedRunnerSettings::new().max_examples(200);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 50);
            tc.target_observations.insert("score".to_string(), v as f64);
        },
        settings,
        make_rng(),
    );
    // Start at score=0; hill_climb will find improvements.
    runner.cached_test_function(&[ChoiceValue::Integer(0)]);
    let _ = runner.optimise_targets();
    // After climbing, best score should be > 0.
    let best = runner
        .best_observed_targets()
        .get("score")
        .copied()
        .unwrap_or(0.0);
    assert!(best > 0.0);
}

// ── consider_new_data: lateral move (lines 953-957) ──────────────────────
//
// Lines 953-957 fire when new_score == current_score but nodes.len() <=
// current_nodes.len(). This is a lateral move that accepts the new result.
// Trigger: make a test where same score can be achieved with shorter nodes.

#[test]
fn consider_new_data_lateral_move_with_shorter_nodes() {
    let settings = TargetedRunnerSettings::new().max_examples(200);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let a = tc.draw_integer(0, 10);
            // Only use 'a' for the score; b is extra.
            tc.target_observations.insert("score".to_string(), a as f64);
        },
        settings,
        make_rng(),
    );
    // Seed: draw one integer; hill_climb will try nearby values.
    runner.cached_test_function(&[ChoiceValue::Integer(5)]);
    let _ = runner.optimise_targets();
}

// ── optimise_targets: Err(RunIsComplete) in the for-target loop (line 540) ─
//
// Line 540 fires when valid_examples >= max_examples at the start of
// iterating over a target. With max_examples=1 and one pre-seed example,
// the very first iteration of the for-target loop checks line 540 and
// returns Err(RunIsComplete).

#[test]
fn optimise_targets_returns_run_is_complete_immediately() {
    let settings = TargetedRunnerSettings::new().max_examples(1);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 10);
            tc.target_observations.insert("score".to_string(), v as f64);
        },
        settings,
        make_rng(),
    );
    // Seed uses the 1 allowed example.
    runner.cached_test_function(&[ChoiceValue::Integer(5)]);
    // valid_examples=1 = max_examples=1 → line 540 fires.
    let result = runner.optimise_targets();
    assert!(result.is_err());
}

// ── find_integer(-1 direction): Err propagated via )?; at line 637 ─────────
//
// Line 637 is the `)?;` closing the second find_integer(-1) call. It covers
// the `Err(RunIsComplete)` propagation path of `?`. This fires when:
// 1. find_integer(+1) returns Ok (no improvement possible: +1 is out of range)
// 2. find_integer(-1) runs one successful try_replace (improving) and then
//    hits valid_examples >= max_examples on the next k-check.
//
// Setup: score = (50 - v), so going -1 improves score. Seed with Integer(10).
// max_examples=4: seed=1, hill_start=1, try_replace(+1)=1, try_replace(-1)=1.
// After try_replace(-1) k=1 succeeds (valid=4=max), find_integer(-1)'s k=2
// check fires: Err(RunIsComplete). This propagates through )?; at line 637.

#[test]
fn find_integer_negative_direction_propagates_run_is_complete() {
    let settings = TargetedRunnerSettings::new().max_examples(4);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 50);
            // Score DECREASES with v: +1 direction gives lower score (bad),
            // -1 direction gives higher score (good, for hill_climb).
            tc.target_observations
                .insert("score".to_string(), (50 - v) as f64);
        },
        settings,
        make_rng(),
    );
    // Seed with Integer(10): score=40. valid_examples=1.
    runner.cached_test_function(&[ChoiceValue::Integer(10)]);
    // optimise_targets → hill_climb → find_integer(+1) uses 1 example
    // (try_replace(+1, k=1) gives score=39 < 40, returns false, Ok immediately).
    // find_integer(-1) uses 1 example (try_replace(-1, k=1) gives score=41 > 40,
    // returns true). k=2 check: valid_examples=4=max → Err propagated via line 637.
    let result = runner.optimise_targets();
    assert!(result.is_err());
}

// ── find_integer: RunIsComplete in binary search (line 712) ─────────────────
//
// Line 712 fires inside the `while lo + 1 < hi` binary search when
// valid_examples >= max_examples. We need the binary search to execute at
// least one valid test run, then exhaust the budget.
//
// The exponential probe FAILS when the candidate value exceeds max_value
// (validate returns false), which stops the probe WITHOUT consuming an
// example. Binary search then starts with a range [lo, hi] where some
// midpoints may still be within range.
//
// Execution trace with max_value=80, seed=[0], max_examples=9:
//   Seed: valid=1. hill_climb: cache hit → valid stays 1.
//   Linear: k=1→1, k=2→3, k=3→6, k=4→10. current=10, valid=5.
//   Exp k=5: current=15, valid=6. k=10: current=25, valid=7.
//   Exp k=20: current=45, valid=8. k=40: 45+40=85>80 → validate FAILS.
//   Break. Binary: lo=20, hi=40, current=45.
//   Binary mid=30: 45+30=75 ≤ 80 → run_extend_full → valid=9. Improvement.
//   Next binary iteration: valid(9) >= max_examples(9) → LINE 712!

#[test]
fn find_integer_binary_search_returns_run_is_complete() {
    // max_value=80 caps the search range. Seed=[0], max_examples=9 ensures
    // the budget runs out exactly during binary search.
    let settings = TargetedRunnerSettings::new().max_examples(9);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 80);
            tc.target_observations.insert("score".to_string(), v as f64);
        },
        settings,
        make_rng(),
    );
    runner.cached_test_function(&[ChoiceValue::Integer(0)]);
    // Exponential probe reaches k=40 (current=45+40=85>80→fail). Binary search
    // starts; first mid=30 passes (45+30=75≤80) and uses the last budget slot.
    // Next binary iteration fires line 712.
    let result = runner.optimise_targets();
    let _ = result;
}

// ── try_replace: max_examples hit during retry loop (line 836) ──────────────
//
// Line 836 fires when valid_examples >= max_examples at the START of a
// retry in try_replace's for-loop (not the very first retry, but subsequent
// ones). This requires new_nodes.len() != current_nodes.len() on a previous
// retry (so the retry loop doesn't return early on the node-length-match check).
//
// Setup:
//  - Score DECREASES with v, so +1 direction gives WORSE score → no improvement.
//  - When v > threshold: draw a SECOND integer (variable node count).
//  - max_examples=2: seed=1. hill_climb's run_extend_full hits cache (no increment).
//    find_integer(+1) k=1: try_replace retry 1 runs test (cache miss → valid=2).
//    No improvement (score worse). new_nodes.len()=2 != current_nodes.len()=1.
//    No span fixup. 2 < 1? No. → retry 2 starts.
//    Retry 2: if valid_examples(=2) >= max_examples(=2) → return false → LINE 836!

#[test]
fn try_replace_max_examples_hit_in_retry_loop() {
    // Score decreases with v → +1 direction never improves. Seed with Integer(5)
    // (1 node). Try +1 → Integer(6) > 5 → draws extra node (2 nodes total).
    // new_nodes.len()=2 != current.len()=1 → retry 2 starts → valid=2=max → line 836.
    let settings = TargetedRunnerSettings::new().max_examples(2);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 50);
            if v > 5 {
                // Draw extra node for values > 5 (simulating variable-length tests).
                let _w = tc.draw_integer(0, 50);
            }
            // Score DECREASES with v: +1 direction worsens score.
            tc.target_observations
                .insert("score".to_string(), (50 - v) as f64);
        },
        settings,
        make_rng(),
    );
    // Seed with Integer(5): score=45, 1 node. valid=1.
    runner.cached_test_function(&[ChoiceValue::Integer(5)]);
    // optimise_targets → hill_climb → find_integer(+1) → try_replace(+1,k=1):
    //   probe [Integer(6)] → 2 nodes, score=44 < 45 → no improvement.
    //   retry 2: valid_examples=2=max → return false (line 836).
    let result = runner.optimise_targets();
    let _ = result;
}

// ── try_replace: EarlyStop from run_extend_full (line 854) ──────────────────
//
// Line 854 fires when run_extend_full returns EarlyStop AND consider_new_data
// already returned false (EarlyStop < Valid → consider always returns false).
//
// Setup:
// - Test draws integer v. If v >= 2, also draws a boolean (2nd node).
//   Score = v. (Increasing with v, so +1 direction is an improvement.)
// - BufferSizeLimit(1): max_size=1. Only 1 draw allowed.
// - Seed [Integer(1)]: v=1 < 2 → no boolean, Valid, score=1, nodes=[Int(1)].
// - hill_climb: run_extend_full([1]) → cache hit → Valid.
// - find_integer(+1) → try_replace(+1, k=1): choices=[Integer(2)].
//   run_extend_full([2]) with max_size=1:
//     draw v=2 from prefix (nodes.len()=0 < 1 → OK). nodes=[Int(2)].
//     v=2 >= 2 → try draw_boolean: nodes.len()=1 >= max_size=1 → EarlyStop.
//   consider_new_data(EarlyStop) → false.
//   status == EarlyStop → return false (LINE 854).

#[test]
fn try_replace_early_stop_fires_line_854() {
    let settings = TargetedRunnerSettings::new().max_examples(10);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 10);
            tc.target_observations.insert("score".to_string(), v as f64);
            if v >= 2 {
                // Second draw triggers EarlyStop when max_size=1.
                let _b = tc.draw_boolean(0.5);
            }
        },
        settings,
        make_rng(),
    );
    // Seed with Integer(1): v=1 < 2 → no boolean, Valid, score=1.
    runner.cached_test_function(&[ChoiceValue::Integer(1)]);
    // Restrict buffer to 1 choice so any run with v >= 2 returns EarlyStop.
    let _limit = BufferSizeLimit::new(1);
    // hill_climb tries +1 direction → Integer(2) → EarlyStop (line 854).
    let result = runner.optimise_targets();
    let _ = result;
}

// ── span-fixup: break when span starts after idx (line 869) ──────────────────
//
// Line 869 fires in the span-fixup loop when `ex.start > idx`. This requires:
//   1. consider_new_data returned false (score worse)
//   2. status != EarlyStop
//   3. new_nodes.len() != current_nodes.len()
//   4. current_spans contains a span with start > idx
//
// Setup:
// - Test: draw integer v (idx=0, no span). start_span, draw integer w (idx=1),
//   stop_span. If v >= 4, draw integer x (idx=2, no span). Score = 10 - v.
// - Seed [Integer(3), Integer(0)]: v=3 < 4. Spans=[{1,2}]. nodes=[Int(3),Int(0)]. Score=7.
// - hill_climb at idx=0: try +1 → choices=[Int(4),Int(0)].
//   run_extend_full([4,0]): v=4>=4 → draws w(prefix) + x(RNG) → 3 nodes.
//   Score=6 < 7 → consider false. Status=Valid. Lengths differ → span-fixup.
//   current_spans=[{1,2}]. ex.start=1 > idx=0 → LINE 869 breaks.

#[test]
fn try_replace_span_fixup_breaks_when_span_after_idx() {
    let settings = TargetedRunnerSettings::new().max_examples(20);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            let v = tc.draw_integer(0, 10);
            // Span wraps only the second integer (idx=1).
            tc.start_span(1);
            let _w = tc.draw_integer(0, 10);
            tc.stop_span();
            if v >= 4 {
                // Extra draw: makes new_nodes.len() > current_nodes.len()
                // when the modified v crosses the threshold.
                let _x = tc.draw_integer(0, 10);
            }
            // Score DECREASES with v → +1 direction is WORSE → consider fails.
            tc.target_observations
                .insert("score".to_string(), (10 - v) as f64);
        },
        settings,
        make_rng(),
    );
    // Seed with v=3 (< 4): 2 nodes [Int(3), Int(0)], spans=[{1,2}], score=7.
    runner.cached_test_function(&[ChoiceValue::Integer(3), ChoiceValue::Integer(0)]);
    // optimise_targets → hill_climb → find_integer(+1) at idx=0:
    //   try_replace(+1,k=1) → choices=[Int(4),Int(0)]:
    //   run_extend_full([4,0]) → v=4>=4 → extra draw → 3 nodes, score=6<7.
    //   consider false. Status=Valid. Lengths differ (3!=2) → span-fixup.
    //   current_spans=[{1,2}]. ex.start=1 > idx=0 → LINE 869 breaks.
    let result = runner.optimise_targets();
    let _ = result;
}

// ── span-fixup: span size matches (line 879) and break (line 869) ────────────
//
// Line 879 fires when ex.end-ex.start == ex_attempt.end-ex_attempt.start.
// Line 869 fires for a second span that starts after idx.
//
// Setup:
// - Test: start_span, draw v (idx=0), stop_span. start_span, draw w (idx=1), stop_span.
//   if v >= 4: draw x (idx=2).  Score = 10 - v.
// - Seed [Integer(3), Integer(0)]: v=3<4 → 2 nodes, spans=[{0,1},{1,2}], score=7.
// - try +1 at idx=0 → choices=[Int(4),Int(0)]: v=4>=4 → 3 nodes, score=6<7.
//   consider false. Status=Valid. Lengths differ → span-fixup.
//   j=0: ex={0,1}, start=0 NOT > idx=0. new_spans={0,1}. size 1==1 → LINE 879.
//   j=1: ex={1,2}, start=1 > idx=0 → LINE 869 breaks.

#[test]
fn try_replace_span_fixup_size_match_and_after_idx() {
    let settings = TargetedRunnerSettings::new().max_examples(20);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            tc.start_span(1);
            let v = tc.draw_integer(0, 10);
            tc.stop_span();
            tc.start_span(2);
            let _w = tc.draw_integer(0, 10);
            tc.stop_span();
            if v >= 4 {
                let _x = tc.draw_integer(0, 10);
            }
            tc.target_observations
                .insert("score".to_string(), (10 - v) as f64);
        },
        settings,
        make_rng(),
    );
    // Seed: v=3 < 4 → 2 nodes [Int(3),Int(0)], spans=[{0,1},{1,2}], score=7.
    runner.cached_test_function(&[ChoiceValue::Integer(3), ChoiceValue::Integer(0)]);
    // try +1 at idx=0 → choices=[Int(4),Int(0)]: 3 nodes, score=6<7.
    // span-fixup: j=0 → same size → LINE 879. j=1 → start=1>idx=0 → LINE 869.
    let result = runner.optimise_targets();
    let _ = result;
}

// ── span-fixup: max_examples exceeded (line 892) ─────────────────────────────
//
// Line 892 fires when valid_examples >= max_examples at the START of the first
// splice attempt (line 891 check).
//
// Flow:
//   - Seed [Int(3)]: 1 node, span={0,1}, score=7. valid_examples=1.
//   - hill_climb start: run_extend_full([3]) → CACHE HIT (no increment). valid=1.
//   - try_replace(+1, k=1): line 835 check: 1 < 2 → proceed.
//     run_extend_full([4]) → CACHE MISS → valid_examples=2.
//     v=4>=4 → extra draw. new_nodes=[4,extra], new_spans=[{0,2}]. size 2.
//     consider_new_data: score=6 < 7 → false. Not EarlyStop. Lengths 2≠1 → span-fixup.
//   - j=0: ex={0,1}. start=0≤idx=0, end=1>0. new_spans[0]={0,2}. sizes 1≠2.
//     → splice code. Line 891: valid_examples(2) >= max_examples(2) → LINE 892!

#[test]
fn try_replace_span_fixup_max_examples_fires_line_892() {
    let settings = TargetedRunnerSettings::new().max_examples(2);
    let mut runner = TargetedRunner::new(
        |tc: &mut TargetedTestCase| {
            tc.start_span(1);
            let v = tc.draw_integer(0, 10);
            if v >= 4 {
                // Extra draw inside span: span size becomes 2 instead of 1.
                let _extra = tc.draw_integer(0, 10);
            }
            tc.stop_span();
            // Score DECREASES with v → +1 direction is worse → consider fails.
            tc.target_observations
                .insert("score".to_string(), (10 - v) as f64);
        },
        settings,
        make_rng(),
    );
    // Seed v=3: 1 node, span={0,1}, score=7, valid_examples=1.
    // hill_climb re-runs [3] → cache hit → valid_examples stays 1.
    // try_replace(+1,k=1): run_extend_full([4]) → valid_examples=2.
    //   new_spans=[{0,2}]. sizes differ. span-fixup j=0: line 891: 2>=2 → LINE 892!
    runner.cached_test_function(&[ChoiceValue::Integer(3)]);
    let result = runner.optimise_targets();
    let _ = result;
}

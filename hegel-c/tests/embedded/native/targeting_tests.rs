use super::*;
use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{BytesChoice, FloatChoice};

fn integer_node(value: i128, min_value: i128, max_value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(min_value),
            max_value: BigInt::from(max_value),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn float_node(value: f64) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Float(FloatChoice {
            min_value: f64::NEG_INFINITY,
            max_value: f64::INFINITY,
            allow_nan: false,
            allow_infinity: true,
            smallest_nonzero_magnitude: 5e-324,
        }),
        ChoiceValue::Float(value),
        false,
    )
}

fn bytes_node(value: Vec<u8>, min_size: usize, max_size: usize) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Bytes(BytesChoice { min_size, max_size }),
        ChoiceValue::Bytes(value),
        false,
    )
}

#[test]
fn targeting_state_starts_empty() {
    let state = TargetingState::new();
    assert!(state.is_empty());
    assert_eq!(state.best_score("anything"), None);
}

#[test]
fn targeting_state_records_first_observation() {
    let mut state = TargetingState::new();
    let choices = vec![ChoiceValue::Integer(BigInt::from(7))];
    let obs = std::collections::HashMap::from([("score".to_string(), 1.5)]);
    state.record(&choices, &obs);
    assert!(!state.is_empty());
    assert_eq!(state.best_score("score"), Some(1.5));
}

#[test]
fn targeting_state_overwrites_only_on_strict_improvement() {
    let mut state = TargetingState::new();
    let choices_a = vec![ChoiceValue::Integer(BigInt::from(1))];
    let choices_b = vec![ChoiceValue::Integer(BigInt::from(2))];
    state.record(
        &choices_a,
        &std::collections::HashMap::from([("s".to_string(), 1.0)]),
    );
    state.record(
        &choices_b,
        &std::collections::HashMap::from([("s".to_string(), 1.0)]),
    );
    assert_eq!(state.best_score("s"), Some(1.0));
    state.record(
        &choices_b,
        &std::collections::HashMap::from([("s".to_string(), 0.5)]),
    );
    assert_eq!(state.best_score("s"), Some(1.0));
    state.record(
        &choices_b,
        &std::collections::HashMap::from([("s".to_string(), 2.0)]),
    );
    assert_eq!(state.best_score("s"), Some(2.0));
}

#[test]
fn targeting_state_tracks_multiple_labels_independently() {
    let mut state = TargetingState::new();
    let choices = vec![ChoiceValue::Integer(BigInt::from(0))];
    state.record(
        &choices,
        &std::collections::HashMap::from([("a".to_string(), 1.0), ("b".to_string(), 2.0)]),
    );
    assert_eq!(state.best_score("a"), Some(1.0));
    assert_eq!(state.best_score("b"), Some(2.0));
    assert_eq!(state.best_score("c"), None);
}

#[test]
fn schedule_fires_at_first_threshold() {
    let mut s = TargetingSchedule::new(100);
    assert!(!s.should_fire(49));
    assert!(s.should_fire(50));
    assert!(!s.should_fire(50));
}

#[test]
fn schedule_re_fires_at_subsequent_thresholds() {
    let mut s = TargetingSchedule::new(100);
    assert!(s.should_fire(50));
    assert!(!s.should_fire(99));
    assert!(s.should_fire(100));
}

#[test]
fn schedule_for_small_max_examples_never_fires_in_range() {
    let mut s = TargetingSchedule::new(1);
    assert!(!s.should_fire(1));
}

#[test]
fn is_climbable_accepts_integer_float_boolean_bytes() {
    let int_node = integer_node(0, 0, 10);
    let float_node = float_node(0.0);
    let bool_node = ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(true),
        false,
    );
    let bytes_node = bytes_node(vec![0], 0, 8);
    for node in [&int_node, &float_node, &bool_node, &bytes_node] {
        assert!(
            is_climbable(&node.value, &node.kind),
            "expected climbable: {node:?}"
        );
    }
}

#[test]
fn is_climbable_rejects_strings() {
    use crate::native::core::StringChoice;
    use crate::native::intervalsets::IntervalSet;
    let sc = StringChoice {
        intervals: IntervalSet::new(vec![(0x20, 0x7E)]).into(),
        min_size: 0,
        max_size: 10,
    };
    assert!(!is_climbable(
        &ChoiceValue::String(vec![b'a' as u32]),
        &ChoiceKind::String(sc),
    ));
}

#[test]
fn is_climbable_returns_false_for_value_and_kind_mismatch() {
    let int_kind = ChoiceKind::Integer(IntegerChoice {
        min_value: BigInt::from(0),
        max_value: BigInt::from(10),
        shrink_towards: BigInt::from(0),
    });
    assert!(!is_climbable(&ChoiceValue::Bytes(vec![0]), &int_kind));
}

#[test]
fn step_choice_integer_adds_delta_within_range() {
    let node = integer_node(5, 0, 100);
    assert_eq!(
        step_choice(&node, 3),
        Some(ChoiceValue::Integer(BigInt::from(8)))
    );
    assert_eq!(
        step_choice(&node, -5),
        Some(ChoiceValue::Integer(BigInt::from(0)))
    );
}

#[test]
fn step_choice_integer_returns_none_when_out_of_range() {
    let node = integer_node(5, 0, 10);
    assert_eq!(step_choice(&node, 100), None);
    assert_eq!(step_choice(&node, -10), None);
}

#[test]
fn step_choice_float_adds_delta_as_f64() {
    let node = float_node(1.5);
    match step_choice(&node, 2) {
        Some(ChoiceValue::Float(f)) => assert!((f - 3.5).abs() < f64::EPSILON),
        other => panic!("expected Float(3.5), got {other:?}"),
    }
}

#[test]
fn step_choice_boolean_only_steps_by_one() {
    use crate::native::core::choices::BooleanChoice;
    let node = ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(false),
        false,
    );
    assert_eq!(step_choice(&node, 1), Some(ChoiceValue::Boolean(true)));
    assert_eq!(step_choice(&node, -1), Some(ChoiceValue::Boolean(false)));
    assert_eq!(step_choice(&node, 0), Some(ChoiceValue::Boolean(false)));
    assert_eq!(step_choice(&node, 2), None);
    assert_eq!(step_choice(&node, -3), None);
}

#[test]
fn step_choice_bytes_adds_big_endian_and_pads() {
    let node = bytes_node(vec![0x00, 0x01], 0, 8);
    assert_eq!(
        step_choice(&node, 1),
        Some(ChoiceValue::Bytes(vec![0x00, 0x02]))
    );
    assert_eq!(
        step_choice(&node, 256),
        Some(ChoiceValue::Bytes(vec![0x01, 0x01]))
    );
}

#[test]
fn step_choice_bytes_returns_none_on_negative_result() {
    let node = bytes_node(vec![0x01], 0, 8);
    assert_eq!(step_choice(&node, -10), None);
}

#[test]
fn step_choice_bytes_handles_zero_after_step() {
    let node = bytes_node(vec![0x01], 0, 8);
    assert_eq!(step_choice(&node, -1), Some(ChoiceValue::Bytes(vec![0x00])));
}

#[test]
fn step_choice_bytes_handles_values_wider_than_128_bits() {
    let mut value = vec![0x01u8; 17];
    let node = bytes_node(value.clone(), 0, 32);
    value[16] = 0x02;
    assert_eq!(step_choice(&node, 1), Some(ChoiceValue::Bytes(value)));
}

#[test]
fn step_choice_bytes_handles_high_bit_of_16_byte_value() {
    let mut value = vec![0x00u8; 16];
    value[0] = 0x80;
    let node = bytes_node(value.clone(), 0, 32);
    value[15] = 0x01;
    assert_eq!(step_choice(&node, 1), Some(ChoiceValue::Bytes(value)));
}

#[test]
fn step_choice_bytes_returns_none_when_overflows_max_size() {
    let node = bytes_node(vec![0xFF], 0, 1);
    assert_eq!(step_choice(&node, 1), None);
}

#[test]
fn step_choice_rejects_mismatched_value_and_kind() {
    use crate::native::core::choices::BooleanChoice;
    let node = ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Integer(BigInt::from(0)),
        false,
    );
    assert_eq!(step_choice(&node, 1), None);
}

use crate::backend::{DataSource, Failure, TestCaseResult};
use crate::native::test_runner::Engine;
use std::collections::HashMap as StdHashMap;

/// A drawn boolean, or `Err(())` if the case overran / was aborted.
fn draw_bool(ds: &dyn DataSource) -> Result<bool, ()> {
    ds.generate_boolean(0.5, None).map_err(|_| ())
}

/// A drawn integer in `[min, max]`, or `Err(())` if the case overran.
fn draw_int(ds: &dyn DataSource, min: i64, max: i64) -> Result<i64, ()> {
    match ds.generate_integer(&BigInt::from(min), &BigInt::from(max)) {
        Ok(v) => Ok(v.to_i64().unwrap()),
        Err(_) => Err(()),
    }
}

fn interesting() -> TestCaseResult {
    TestCaseResult::Interesting(Failure {
        origin: "Panic at <targeting-test>".to_string(),
        reproduce_blob: None,
        comments: Vec::new(),
    })
}

fn run_optimise<F>(start: Vec<ChoiceValue>, start_score: f64, mut body: F)
where
    F: FnMut(&dyn DataSource) -> TestCaseResult,
{
    let mut run_case = move |ds: Box<dyn DataSource + Send + Sync>| {
        let result = body(&*ds);
        ds.mark_complete(&result);
    };
    let settings = crate::Settings::new().database(None).seed(Some(0xc0ffee));
    let mut engine = Engine::new(&settings, None, &mut run_case);
    engine
        .targeting
        .record(&start, &StdHashMap::from([("".to_string(), start_score)]));

    let mut optimiser = Optimiser {
        engine: &mut engine,
        max_valid: 10_000,
        max_calls: 100_000,
    };
    optimiser.optimise_targets();
}

/// Drives `hill_climb`'s resize-restart branch and the already-examined
/// skip that follows it: an integer at a non-trailing position controls a
/// downstream loop, so a successful `find_integer` step grows the choice
/// count, the next outer iteration sees `current_nodes.len() != prev_len`
/// and resets `i`, and walking back down hits indices that are still in
/// `nodes_examined` from before the resize.
#[test]
fn hill_climb_resize_restart_and_already_examined_skip() {
    let start = vec![
        ChoiceValue::Boolean(false),
        ChoiceValue::Integer(BigInt::from(2)),
        ChoiceValue::Integer(BigInt::from(2)),
        ChoiceValue::Boolean(false),
        ChoiceValue::Boolean(false),
    ];
    run_optimise(start, 4.0, |ds| {
        let body = || -> Result<TestCaseResult, ()> {
            draw_bool(ds)?;
            let m = draw_int(ds, 0, 20)?;
            let n = draw_int(ds, 0, 20)?;
            for _ in 0..n {
                draw_bool(ds)?;
            }
            ds.target_observation((m + n) as f64, "").unwrap();
            Ok(TestCaseResult::Valid)
        };
        body().unwrap_or(TestCaseResult::Overrun)
    });
}

/// Drives `try_replace`'s `!strict && grew` rejection: a non-trailing
/// boolean controls a downstream loop, and a constant target score makes
/// any flip a lateral move. Flipping `false → true` adds three integer
/// draws to the body, which `try_replace` rejects as a length-growing
/// lateral move.
#[test]
fn hill_climb_rejects_lateral_grow() {
    let start = vec![ChoiceValue::Boolean(false), ChoiceValue::Boolean(false)];
    run_optimise(start, 1.0, |ds| {
        let body = || -> Result<TestCaseResult, ()> {
            draw_bool(ds)?;
            let big = draw_bool(ds)?;
            if big {
                for _ in 0..3 {
                    draw_int(ds, 0, 10)?;
                }
            }
            ds.target_observation(1.0, "").unwrap();
            Ok(TestCaseResult::Valid)
        };
        body().unwrap_or(TestCaseResult::Overrun)
    });
}

/// Drives `try_replace`'s `trial.status < Status::Valid` rejection: an
/// assumption rules out a specific integer value, so any `find_integer`
/// probe that lands on it comes back with `Status::Invalid` and gets
/// short-circuited.
#[test]
fn hill_climb_rejects_invalid_trial_status() {
    let start = vec![ChoiceValue::Integer(BigInt::from(6))];
    run_optimise(start, -1.0, |ds| {
        let n = match draw_int(ds, 0, 20) {
            Ok(n) => n,
            Err(()) => return TestCaseResult::Overrun,
        };
        if n == 7 {
            return TestCaseResult::Invalid;
        }
        ds.target_observation(-((n - 7).saturating_abs() as f64), "")
            .unwrap();
        TestCaseResult::Valid
    });
}

/// Drives `hill_climb`'s `trial.status < Status::Valid` early-return when
/// the *initial* replay of `start_choices` itself comes back non-Valid:
/// the recorded "best" is `n = 7`, which the body rejects.
#[test]
fn hill_climb_returns_zero_when_initial_replay_invalid() {
    let start = vec![ChoiceValue::Integer(BigInt::from(7))];
    run_optimise(start, 0.0, |ds| {
        let n = match draw_int(ds, 0, 20) {
            Ok(n) => n,
            Err(()) => return TestCaseResult::Overrun,
        };
        if n == 7 {
            return TestCaseResult::Invalid;
        }
        ds.target_observation(n as f64, "").unwrap();
        TestCaseResult::Valid
    });
}

/// Drives `run_trial`'s `Status::Interesting` branch — the bug-found path
/// where targeting promotes a perturbation into the `interesting` map for
/// the surrounding shrinker to pick up. Starting from `n = 6` (score `-1`),
/// `find_integer` probes `n = 7` in the +1 direction; the body reports
/// INTERESTING there.
#[test]
fn run_trial_records_interesting_result_into_ctx() {
    let start = vec![ChoiceValue::Integer(BigInt::from(6))];
    run_optimise(start, -1.0, |ds| {
        let n = match draw_int(ds, 0, 20) {
            Ok(n) => n,
            Err(()) => return TestCaseResult::Overrun,
        };
        if n == 7 {
            return interesting();
        }
        ds.target_observation(-((n - 7).saturating_abs() as f64), "")
            .unwrap();
        TestCaseResult::Valid
    });
}

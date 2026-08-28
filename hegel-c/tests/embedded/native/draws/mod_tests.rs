use super::*;
use crate::native::core::{ManyState, NativeTestCase};
use crate::native::rng::EngineRng;

fn float_spec(width: u32, min_value: f64, max_value: f64) -> FloatSpec {
    FloatSpec {
        width,
        min_value,
        max_value,
        allow_nan: false,
        allow_infinity: false,
        exclude_min: false,
        exclude_max: false,
        smallest_nonzero_magnitude: f64::MIN_POSITIVE,
    }
}

#[test]
fn width_32_float_bounds_must_be_f32_representable() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(3)).unwrap();

    for (min_value, max_value, bad) in [
        (1.0f64.next_up(), 2.0, "min_value"),
        (0.0, 1.0f64.next_up(), "max_value"),
        (f64::MAX, f64::MAX, "min_value"),
    ] {
        let err = generate_float(&mut ntc, &float_spec(32, min_value, max_value)).unwrap_err();
        let EngineError::InvalidArgument(msg) = err else {
            panic!("expected InvalidArgument, got {err:?}");
        };
        assert!(
            msg.contains(bad) && msg.contains("width 32"),
            "unexpected message: {msg}"
        );
    }

    let v = generate_float(&mut ntc, &float_spec(32, 0.5, 2.0)).unwrap();
    assert!((0.5..=2.0).contains(&v));

    let mut inf_spec = float_spec(32, f64::NEG_INFINITY, f64::INFINITY);
    inf_spec.allow_infinity = true;
    generate_float(&mut ntc, &inf_spec).unwrap();

    let v = generate_float(&mut ntc, &float_spec(64, 1.0f64.next_up(), 2.0)).unwrap();
    assert!((1.0f64.next_up()..=2.0).contains(&v));
}

#[test]
fn narrow_to_f32_keeps_overflowing_finite_draws_finite() {
    let snm = f64::from(f32::from_bits(1));
    assert_eq!(
        narrow_to_f32(f64::NEG_INFINITY, f64::INFINITY, snm, 1.5),
        1.5
    );
    for raw in [1e300, -1e300, f64::MAX, f64::from(f32::MAX) * 2.0] {
        let v = narrow_to_f32(f64::NEG_INFINITY, f64::INFINITY, snm, raw);
        assert!(v.is_finite(), "{raw} narrowed to non-finite {v}");
        assert!(v.abs() <= f64::from(f32::MAX));
        assert_eq!(v, f64::from(v as f32));
    }
    let v = narrow_to_f32(0.0, f64::INFINITY, snm, 1e300);
    assert!(v.is_finite() && v >= 0.0);
}

#[test]
fn many_reject_marks_invalid_when_cannot_reach_min_size() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(1)).unwrap();
    let mut state = ManyState::new(6, Some(10));
    state.count = 5;
    state.rejections = 9;

    let result = many_reject(&mut ntc, &mut state);
    assert!(
        result.is_err(),
        "expected StopTest once rejections overflow"
    );
    assert_eq!(ntc.status(), Some(Status::Invalid));
}

#[test]
fn many_more_respects_fixed_and_bounded_sizes() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(2)).unwrap();
    let mut fixed = ManyState::new(3, Some(3));
    let mut count = 0;
    while many_more(&mut ntc, &mut fixed).unwrap() {
        count += 1;
    }
    assert_eq!(count, 3);

    let mut bounded = ManyState::new(1, Some(4));
    let mut count = 0;
    while many_more(&mut ntc, &mut bounded).unwrap() {
        count += 1;
    }
    assert!((1..=4).contains(&count));
}

#[test]
fn recursion_branch_probability_pins_the_expected_leaf_count_to_the_budget() {
    for k in [1.3, 2.0, 3.0, 4.0, 10.0] {
        for max_leaves in [8u64, 100, 1_000_000] {
            let p = recursion_branch_probability(k, max_leaves);
            assert!(p > 0.0 && p < 1.0 / k);
            let expected = (1.0 - p) / (1.0 - k * p);
            let budget = max_leaves as f64;
            assert!(
                (expected - budget).abs() < budget * 1e-9,
                "arity {k}, max_leaves {max_leaves}: E[L] = {expected} != {budget}"
            );
        }
    }
}

#[test]
fn recursion_branch_probability_decreases_with_arity_and_grows_with_budget() {
    for arity in 2..10 {
        let k = arity as f64;
        assert!(recursion_branch_probability(k + 1.0, 100) < recursion_branch_probability(k, 100));
    }
    assert!(recursion_branch_probability(2.0, 100) < recursion_branch_probability(2.0, 1000));
}

#[test]
fn recursion_branch_probability_clamps_tiny_budgets_to_two_leaves() {
    let floor = recursion_branch_probability(2.0, 2);
    assert!(floor > 0.0);
    assert_eq!(recursion_branch_probability(2.0, 0), floor);
    assert_eq!(recursion_branch_probability(2.0, 1), floor);
}

#[test]
fn recursion_branch_probability_stays_finite_at_a_maximal_budget() {
    let p = recursion_branch_probability(2.0, u64::MAX);
    assert!(p > 0.49 && p < 0.5);
}

#[test]
fn recursion_branch_probability_caps_chain_like_arities() {
    assert_eq!(
        recursion_branch_probability(1.0, 100),
        RECURSION_MAX_BRANCH_PROBABILITY
    );
    assert_eq!(
        recursion_branch_probability(0.5, 100),
        RECURSION_MAX_BRANCH_PROBABILITY
    );
    assert_eq!(
        recursion_branch_probability(1.01, 100),
        RECURSION_MAX_BRANCH_PROBABILITY
    );
    assert!(recursion_branch_probability(1.3, 100) < RECURSION_MAX_BRANCH_PROBABILITY);
}

#[test]
fn recursion_pricing_never_moves_for_a_binary_branch_function() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(7)).unwrap();
    let mut state = new_recursion_state(32, 100, ntc.span_depth());
    let priced = state.branch_probability;
    assert_eq!(priced, recursion_branch_probability(2.0, 100));

    let mut pending = Vec::from([0u64]);
    let mut leaves = 0;
    while let Some(depth) = pending.pop() {
        if recursion_branch(&mut ntc, &mut state, depth).unwrap() {
            pending.push(depth + 1);
            pending.push(depth + 1);
        } else {
            assert!(state.count_leaf());
            leaves += 1;
        }
    }
    assert!(leaves >= 1);
    assert!(recursion_finish(&mut ntc, &mut state).unwrap());
    assert_eq!(state.branch_probability, priced);
    assert_eq!(state.reprices, 0);
    assert_eq!(state.closed_children, 2 * state.closed_branches);
}

#[test]
fn recursion_finish_reprices_a_chain_heavy_value_and_accepts_the_redraw() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(11)).unwrap();
    let base = ntc.span_depth();
    let mut state = new_recursion_state(32, 100, base);
    let priced = state.branch_probability;

    for depth in 0..6 {
        state.observe_decision(depth);
        state.observe_branch(depth);
    }
    state.observe_decision(6);
    state.leaves = 1;
    ntc.start_span(17);

    assert!(!recursion_finish(&mut ntc, &mut state).unwrap());
    assert_eq!(ntc.span_depth(), base);
    assert_eq!(state.leaves, 0);
    assert_eq!(state.reprices, 1);
    assert!(state.branch_probability > priced);
    assert_eq!(state.closed_branches, 6);
    assert_eq!(state.closed_children, 6);

    let repriced = state.branch_probability;
    for depth in 0..6 {
        state.observe_decision(depth);
        state.observe_branch(depth);
    }
    state.observe_decision(6);
    state.leaves = 1;
    assert!(recursion_finish(&mut ntc, &mut state).unwrap());
    assert_eq!(state.reprices, 1);
    assert_eq!(state.branch_probability, repriced);
}

#[test]
fn recursion_finish_stops_repricing_at_the_cap() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(13)).unwrap();
    let mut state = new_recursion_state(32, 100, ntc.span_depth());

    for round in 0..RECURSION_MAX_REPRICES {
        state.observe_decision(0);
        state.observe_branch(0);
        state.observe_decision(1);
        state.leaves = 1;
        let accepted = recursion_finish(&mut ntc, &mut state).unwrap();
        assert!(!accepted, "round {round} should have repriced");
    }
    assert_eq!(state.reprices, RECURSION_MAX_REPRICES);

    state.observe_decision(0);
    state.observe_branch(0);
    state.observe_decision(1);
    state.leaves = 1;
    assert!(recursion_finish(&mut ntc, &mut state).unwrap());
    assert_eq!(state.reprices, RECURSION_MAX_REPRICES);
}

#[test]
fn recursion_finish_accepts_a_branchless_value_at_first_sight() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(17)).unwrap();
    let mut state = new_recursion_state(32, 100, ntc.span_depth());
    let priced = state.branch_probability;
    state.observe_decision(0);
    state.leaves = 1;
    assert!(recursion_finish(&mut ntc, &mut state).unwrap());
    assert_eq!(state.branch_probability, priced);
    assert_eq!(state.reprices, 0);
}

#[test]
fn recursion_retry_discards_partial_observations_and_lowers_the_price() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(19)).unwrap();
    let base = ntc.span_depth();
    let mut state = new_recursion_state(32, 100, base);
    let priced = state.branch_probability;

    state.observe_decision(0);
    state.observe_branch(0);
    state.observe_decision(1);
    state.observe_branch(1);
    state.leaves = 3;
    ntc.start_span(17);

    recursion_retry(&mut ntc, &mut state).unwrap();
    assert_eq!(ntc.span_depth(), base);
    assert_eq!(state.leaves, 0);
    assert_eq!(state.attempt, 1);
    assert!(state.open_branches.is_empty());
    assert_eq!(state.closed_branches, 0);
    assert!(state.branch_probability < priced);
    assert_eq!(
        state.branch_probability,
        recursion_branch_probability(3.0, 100)
    );
}

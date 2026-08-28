use super::*;
use crate::native::core::{ManyState, NativeTestCase, RecursionState};
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

/// A recursion state with a chosen target, as if `new_recursion_state`
/// had drawn `target`, without consuming any draws from `ntc`.
fn recursion_state(
    max_depth: u64,
    max_leaves: u64,
    target: u64,
    ntc: &NativeTestCase,
) -> RecursionState {
    let mut state = RecursionState {
        max_depth,
        max_leaves,
        attempt: 0,
        leaves: 0,
        base_span_depth: ntc.span_depth(),
        target,
        branch_probability: 0.0,
        closed_children: 0,
        closed_branches: 0,
        open_branches: Vec::new(),
        reprices: 0,
    };
    state.branch_probability = recursion_priced_probability(&state);
    state
}

/// Evaluate a solved profile's expected leaf count independently of the
/// solver's own incremental bookkeeping: start from the tail fixed point
/// and fold one level of growth per depth up to the root.
fn profile_expectation(profile: &RecursionProfile, arity: f64, tail_probability: f64) -> f64 {
    let mut f = (1.0 - tail_probability) / (1.0 - tail_probability * arity);
    for depth in (0..=profile.boundary_depth).rev() {
        let p = if depth < profile.boundary_depth {
            RECURSION_MAX_BRANCH_PROBABILITY
        } else {
            profile.boundary_probability
        };
        f = (1.0 - p) + p * arity * f;
    }
    f
}

#[test]
fn recursion_profile_solves_the_target_expectation_exactly() {
    for arity in [1.3, 1.5, 2.0, 3.0, 10.0] {
        let tail = recursion_branch_probability(arity, 2);
        let profile = recursion_profile(50.0, arity, tail, 32);
        assert_eq!(profile.expected_leaves, 50.0);
        assert!(profile.boundary_probability >= tail);
        assert!(profile.boundary_probability <= RECURSION_MAX_BRANCH_PROBABILITY);
        let checked = profile_expectation(&profile, arity, tail);
        assert!(
            (checked - 50.0).abs() < 1e-6,
            "arity {arity}: E[L] = {checked} != 50"
        );
    }
}

#[test]
fn recursion_profile_caps_growth_at_the_depth_limit() {
    let profile = recursion_profile(1000.0, 2.0, 0.0, 3);
    assert_eq!(profile.boundary_depth, 3);
    assert_eq!(
        profile.boundary_probability,
        RECURSION_MAX_BRANCH_PROBABILITY
    );
    assert!(profile.expected_leaves > 1.0);
    assert!(profile.expected_leaves < 1000.0);
}

#[test]
fn recursion_profile_plateaus_on_a_chain_like_grammar() {
    let profile = recursion_profile(100.0, 1.0, 0.0, u64::MAX);
    assert_eq!(profile.boundary_depth, 0);
    assert_eq!(
        profile.boundary_probability,
        RECURSION_MAX_BRANCH_PROBABILITY
    );
    assert!((profile.expected_leaves - 1.0).abs() < 1e-9);
}

#[test]
fn recursion_profile_stops_searching_at_the_horizon() {
    let profile = recursion_profile(1_000_000.0, 1.055, 0.0, u64::MAX);
    assert_eq!(profile.boundary_depth, RECURSION_PROFILE_HORIZON);
    assert_eq!(
        profile.boundary_probability,
        RECURSION_MAX_BRANCH_PROBABILITY
    );
    assert!(profile.expected_leaves < 1_000_000.0);
}

#[test]
fn recursion_profile_of_a_leaf_only_depth_limit_expects_one_leaf() {
    let profile = recursion_profile(64.0, 2.0, 0.0, 0);
    assert_eq!(profile.expected_leaves, 1.0);
}

#[test]
fn recursion_profile_uses_the_tail_when_it_already_meets_the_target() {
    let tail = recursion_branch_probability(2.0, 2);
    let profile = recursion_profile(2.0, 2.0, tail, 32);
    assert_eq!(profile.boundary_depth, 0);
    assert_eq!(profile.boundary_probability, tail);
    assert!((profile.expected_leaves - 2.0).abs() < 1e-9);
}

#[test]
fn new_recursion_state_draws_a_spread_of_targets() {
    let mut single = 0;
    let mut large = 0;
    for seed in 0..200 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let state = new_recursion_state(&mut ntc, 32, 100).unwrap();
        assert!(state.target == 1 || (2..=100).contains(&state.target));
        if state.target == 1 {
            single += 1;
        }
        if state.target >= 90 {
            large += 1;
        }
    }
    assert!((20..=90).contains(&single), "{single} single-leaf targets");
    assert!(large >= 1, "no large targets in 200 draws");
}

#[test]
fn new_recursion_state_with_a_tiny_budget_aims_for_a_single_leaf() {
    for max_leaves in [0, 1] {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(23)).unwrap();
        let state = new_recursion_state(&mut ntc, 32, max_leaves).unwrap();
        assert_eq!(state.target, 1);
        assert_eq!(
            state.branch_probability,
            recursion_branch_probability(2.0, 2)
        );
    }
}

#[test]
fn recursion_pricing_never_moves_for_a_binary_branch_function() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(7)).unwrap();
    let mut state = recursion_state(32, 100, 1, &ntc);
    let priced = state.branch_probability;
    assert_eq!(priced, recursion_branch_probability(2.0, 2));

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
fn recursion_target_steering_drives_a_binary_tree_toward_its_target() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(29)).unwrap();
    let mut state = recursion_state(32, 100, 64, &ntc);

    let leaves = 'drawn: loop {
        let mut pending = Vec::from([0u64]);
        let mut leaves = 0u64;
        while let Some(depth) = pending.pop() {
            if recursion_branch(&mut ntc, &mut state, depth).unwrap() {
                pending.push(depth + 1);
                pending.push(depth + 1);
            } else if state.count_leaf() {
                leaves += 1;
            } else {
                recursion_retry(&mut ntc, &mut state).unwrap();
                continue 'drawn;
            }
        }
        if recursion_finish(&mut ntc, &mut state).unwrap() {
            break leaves;
        }
    };
    assert!((1..=100).contains(&leaves));
    assert!(
        leaves.saturating_mul(2) >= core::cmp::max(1, state.target >> state.attempt)
            || state.reprices == RECURSION_MAX_REPRICES
    );
}

#[test]
fn recursion_finish_reprices_a_chain_heavy_value_and_accepts_the_redraw() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(11)).unwrap();
    let base = ntc.span_depth();
    let mut state = recursion_state(32, 100, 1, &ntc);
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
fn recursion_finish_reprices_an_undersized_value_with_a_reachable_target() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(31)).unwrap();
    let base = ntc.span_depth();
    let mut state = recursion_state(32, 100, 64, &ntc);
    state.closed_children = 8;
    state.closed_branches = 4;
    state.leaves = 5;
    ntc.start_span(17);

    assert!(!recursion_finish(&mut ntc, &mut state).unwrap());
    assert_eq!(ntc.span_depth(), base);
    assert_eq!(state.leaves, 0);
    assert_eq!(state.reprices, 1);

    state.leaves = 40;
    assert!(recursion_finish(&mut ntc, &mut state).unwrap());
    assert_eq!(state.reprices, 1);
}

#[test]
fn recursion_finish_accepts_an_undersized_value_whose_target_is_unreachable() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(37)).unwrap();
    let mut state = recursion_state(32, 100, 64, &ntc);
    for depth in 0..6 {
        state.observe_decision(depth);
        state.observe_branch(depth);
    }
    state.observe_decision(6);
    state.leaves = 1;
    assert!(recursion_finish(&mut ntc, &mut state).unwrap());
    assert_eq!(state.reprices, 0);
}

#[test]
fn recursion_finish_stops_repricing_at_the_cap() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(13)).unwrap();
    let mut state = recursion_state(32, 100, 1, &ntc);

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
    let mut state = recursion_state(32, 100, 1, &ntc);
    let priced = state.branch_probability;
    state.observe_decision(0);
    state.leaves = 1;
    assert!(recursion_finish(&mut ntc, &mut state).unwrap());
    assert_eq!(state.branch_probability, priced);
    assert_eq!(state.reprices, 0);
}

#[test]
fn recursion_retry_discards_partial_observations_and_halves_the_target() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(19)).unwrap();
    let base = ntc.span_depth();
    let mut state = recursion_state(32, 100, 40, &ntc);
    let priced = state.branch_probability;
    assert_eq!(priced, recursion_branch_probability(2.0, 40));

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
        recursion_branch_probability(3.0, 20)
    );
}

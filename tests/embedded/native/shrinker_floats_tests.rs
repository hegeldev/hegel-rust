use super::*;
use crate::native::bignum::BigInt;
use crate::native::core::Spans;
use crate::native::core::choices::IntegerChoice;
use crate::native::shrinker::Shrinker;

// ── redistribute_pair: cand_j out-of-range ────────────────────────────────
//
// `find_integer` short-circuits via `return false` when `build_value` rejects
// a candidate for either side. Walked organically the (Float, Int) and
// (Int, Float) integration tests in `tests/test_shrink_quality/mixed_types.rs`
// already hit the `cand_i` side; this test covers the `cand_j` side too by
// constructing a Float-Int pair where `find_integer` raises the integer past
// its `max_value`.

fn float_node(value: f64, min: f64, max: f64) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Float(FloatChoice {
            min_value: min,
            max_value: max,
            allow_nan: false,
            allow_infinity: false,
            smallest_nonzero_magnitude: 5e-324,
        }),
        ChoiceValue::Float(value),
        false,
    )
}

fn int_node(value: i128, min: i128, max: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(min),
            max_value: BigInt::from(max),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

#[test]
fn redistribute_pair_below_shrink_target_uses_raise_left_direction() {
    // `v_i` starts negative (below its shrink target of `0.0`), so
    // `redistribute_pair` picks `Direction::RaiseLeftLowerRight`: raise the
    // first node toward 0, lower the second. The integration tests in
    // `tests/test_shrink_quality/mixed_types.rs` only exercise the
    // `LowerLeftRaiseRight` direction (both sides positive); without a
    // deterministic case here this branch's coverage depends on the
    // boundary-biased random sampler happening to draw a negative value.
    let initial = vec![
        float_node(-3.0, -100.0, 100.0),
        float_node(5.0, -100.0, 100.0),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.redistribute_numeric_pairs().unwrap();
    let (a, b) = match (
        &shrinker.current_nodes[0].value,
        &shrinker.current_nodes[1].value,
    ) {
        (ChoiceValue::Float(a), ChoiceValue::Float(b)) => (*a, *b),
        _ => unreachable!(),
    };
    assert!(a > -3.0, "v_i did not move up from -3.0 (got {a})");
    assert!(b < 5.0, "v_j did not move down from 5.0 (got {b})");
}

#[test]
fn redistribute_pair_bails_when_int_candidate_leaves_validate_range() {
    let initial = vec![float_node(3.0, -100.0, 100.0), int_node(2, 1, 10)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.redistribute_numeric_pairs().unwrap();
    // Engine stayed within validate bounds despite the accepting test_fn.
    match (
        &shrinker.current_nodes[0].value,
        &shrinker.current_nodes[1].value,
    ) {
        (ChoiceValue::Float(_), ChoiceValue::Integer(n)) => {
            assert!((1..=10).contains(&i128::try_from(n).unwrap()));
        }
        _ => unreachable!(),
    }
}

// ── shrink_floats: NaN canonicalization (stepped accept path) ─────────────
//
// When a Float node holds a NaN value, `shrink_floats` tries to replace it
// with `f64::MAX` and then `f64::INFINITY`. If the test predicate accepts
// one of those candidates, the loop sets `stepped = true` and breaks.
//
// Driving this directly with a constructed shrinker makes the path
// deterministic — random integration tests hit it only when the boundary
// sampler happens to find NaN before any other interesting value.

#[test]
fn shrink_floats_canonicalizes_nan_to_finite_when_predicate_admits() {
    let initial = vec![ChoiceNode::new(
        ChoiceKind::Float(FloatChoice {
            min_value: f64::NEG_INFINITY,
            max_value: f64::INFINITY,
            allow_nan: true,
            allow_infinity: true,
            smallest_nonzero_magnitude: 5e-324,
        }),
        ChoiceValue::Float(f64::NAN),
        false,
    )];
    // Predicate: accept NaN, infinity, or `f64::MAX`. The canonicalization
    // tries `f64::MAX` first and accepts it.
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: crate::native::shrinker::ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let interesting = nodes.iter().all(|n| match &n.value {
                    ChoiceValue::Float(f) => f.is_nan() || f.is_infinite() || *f == f64::MAX,
                    _ => false,
                });
                (interesting, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_floats().unwrap();
    // After canonicalization the node holds `f64::MAX` (first accepted
    // candidate in the iteration order) rather than the original NaN.
    match shrinker.current_nodes[0].value {
        ChoiceValue::Float(f) => assert_eq!(f, f64::MAX),
        _ => unreachable!(),
    }
}

// ── as_integer_ratio ──────────────────────────────────────────────────────

#[test]
fn as_integer_ratio_recovers_simple_terminating_decimal() {
    // 0.5 = 1 / 2
    assert_eq!(as_integer_ratio(0.5), Some((1, 2)));
    // 1.5 = 3 / 2
    assert_eq!(as_integer_ratio(1.5), Some((3, 2)));
    // Integer values: denominator 1.
    assert_eq!(as_integer_ratio(2.0), Some((2, 1)));
    assert_eq!(as_integer_ratio(1024.0), Some((1024, 1)));
}

#[test]
fn as_integer_ratio_subnormal_decomposes_with_huge_denominator() {
    // The smallest positive subnormal has biased_exp == 0 and mantissa 1.
    // Its denominator is 2^1074, which overflows u128 — we expect `None`
    // back, but the early `biased_exp == 0` branch must still run to
    // compute the numerator/exponent pair before the overflow check trips.
    let smallest_subnormal = f64::from_bits(1);
    assert_eq!(as_integer_ratio(smallest_subnormal), None);
}

#[test]
fn as_integer_ratio_huge_value_overflows_to_none() {
    // f64::MAX has a numerator that, after shifting, exceeds u128. The
    // checked_shl on line 46 returns None.
    assert_eq!(as_integer_ratio(f64::MAX), None);
}

/// Cover the negative branch of `is_neg` ternary inside
/// `shrink_floats`'s shift_right + shrink_by_multiples chain
/// (`floats.rs:235`).  Requires a very-large-magnitude *negative*
/// float so the |v| >= MAX_PRECISE_INTEGER branch fires and the
/// shrink_by_multiples loop negates each candidate.  Bounded
/// `min_value` so `lo` computes finitely and the inner `attempt <
/// lo` check doesn't short-circuit before the negation runs.
#[test]
fn shrink_floats_negative_large_magnitude_uses_is_neg_branch() {
    let initial = vec![float_node(-1e18, -1e20, 0.0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                // Interesting iff first node is < -1.0 and finite.
                let interesting =
                    matches!(nodes[0].value, ChoiceValue::Float(v) if v < -1.0 && v.is_finite());
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_floats().unwrap();
    match shrinker.current_nodes[0].value {
        ChoiceValue::Float(v) => assert!(v < -1.0 && v.is_finite()),
        _ => unreachable!(),
    }
}

/// Regression for the negative-bound `shrink_by_multiples` step.
/// Starting from a huge-magnitude negative float with a predicate that
/// admits everything `<= -3.0`, `shift_right` halves the magnitude
/// until it overshoots to `-4.0` (because `-2.0` is rejected, but
/// `-4.0` is accepted).  The follow-up `shrink_by_multiples(2)` /
/// `(1)` then needs to peel the last unit off the magnitude to land
/// on the exact predicate boundary at `-3.0`.  Before the fix, that
/// loop was a no-op for `is_neg=true` (the `lo` bound was computed
/// from `fc.min_value` instead of `fc.max_value`), so the shrinker
/// stopped at `-4.0`.
#[test]
fn shrink_floats_negative_shrink_by_multiples_reaches_predicate_boundary() {
    // 2^60 is well past `MAX_PRECISE_INTEGER` so the
    // |v| >= MAX_PRECISE_INTEGER branch fires.
    let v0 = -(1i64 << 60) as f64;
    let initial = vec![float_node(v0, -(1i128 << 61) as f64, -1.0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = matches!(
                    nodes[0].value,
                    ChoiceValue::Float(v) if v <= -3.0 && v.is_finite()
                );
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_floats().unwrap();
    match shrinker.current_nodes[0].value {
        ChoiceValue::Float(v) => assert_eq!(v, -3.0),
        _ => unreachable!(),
    }
}

// ── Float.run_step ports: Integer delegation, grid bijection, ladder order ──

/// Integer-valued floats delegate to the full Integer.shrink move set —
/// `mask_high_bits` in particular. The predicate keeps the low byte fixed
/// at 0x77, which neither the lex-index bisection nor a plain binary
/// search can maintain; masking the high bits of 0x30077 reaches 0x77
/// directly, exactly as in Hypothesis's `Float.run_step` delegation.
#[test]
fn shrink_floats_delegates_integer_valued_to_integer_shrink() {
    let start = 0x30077 as f64;
    let initial = vec![float_node(start, 0.0, 1e9)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(&ChoiceValue::Float(f))
                        if f.fract() == 0.0 && f >= 0.0 && (f as u64) & 0xFF == 0x77
                );
                (ok, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_floats().unwrap();
    assert_eq!(
        shrinker.current_nodes[0].value,
        ChoiceValue::Float(0x77 as f64),
        "mask_high_bits should reach 119.0 from 196727.0"
    );
}

/// The precision-drop ladder runs least-precise-first (Python's
/// `for p in range(10)`), so the very first candidate that actually
/// executes for 2.75 is `floor(2.75) == 2.0` — not the most-precise
/// roundings, which a reversed ladder would try first.
#[test]
fn shrink_floats_precision_ladder_tries_least_precise_first() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let executed: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
    let executed_clone = executed.clone();
    let initial = vec![float_node(2.75, 0.5, 100.0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                if let Some(&ChoiceValue::Float(f)) = nodes.first().map(|n| &n.value) {
                    executed_clone.borrow_mut().push(f);
                }
                // Only the starting value is interesting: every candidate
                // is executed and rejected, exposing the probe order.
                let ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(&ChoiceValue::Float(f)) if f == 2.75
                );
                (ok, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_floats().unwrap();
    // The first executed candidate is the fixed `simplest()` prelude
    // (1.0 for this range); the ladder follows. Its first probe must be
    // the least-precise rounding floor(2.75) = 2.0, and in particular it
    // must come before the more-precise p = 1 rounding 2.5.
    let executed = executed.borrow();
    assert_eq!(
        executed.get(1),
        Some(&2.0),
        "least-precise rounding (p = 0) must lead the ladder: {executed:?}"
    );
    let pos_2 = executed.iter().position(|&f| f == 2.0).unwrap();
    let pos_2_5 = executed.iter().position(|&f| f == 2.5).unwrap();
    assert!(
        pos_2 < pos_2_5,
        "p = 0 must be probed before p = 1: {executed:?}"
    );
}

// ── float-grid bijection ─────────────────────────────────────────────────

#[test]
fn float_position_bijection_round_trips() {
    for f in [
        0.0,
        1.0,
        MAX_PRECISE_INTEGER,
        MAX_PRECISE_INTEGER + 2.0,
        1.5 * (1u64 << 60) as f64,
        f64::MAX,
    ] {
        assert_eq!(
            position_to_float(float_to_position(f)),
            f,
            "round trip failed for {f}"
        );
    }
}

#[test]
fn float_position_adjacent_positions_are_adjacent_floats() {
    // Above MAX_PRECISE_INTEGER, decrementing the position by one steps to
    // the previous representable float (Python's `_float_to_position`
    // contract), not to an unrepresentable integer that rounds back.
    for f in [
        MAX_PRECISE_INTEGER + 2.0,
        (1u64 << 60) as f64,
        1.5 * (1u64 << 60) as f64,
        f64::MAX,
    ] {
        let pos = float_to_position(f);
        let down = position_to_float(pos - 1);
        assert_eq!(down, next_down_f64_for_test(f), "position step for {f}");
    }
    // At and below the boundary, positions are just the integer values.
    assert_eq!(float_to_position(MAX_PRECISE_INTEGER), 1u128 << 53);
    assert_eq!(float_to_position(5.7), 5);
}

/// std `next_down` equivalent for the assertion above (positive finite
/// inputs only).
fn next_down_f64_for_test(f: f64) -> f64 {
    f64::from_bits(f.to_bits() - 1)
}

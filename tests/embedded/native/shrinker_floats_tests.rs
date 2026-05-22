use super::*;
use crate::native::core::Spans;
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
    ChoiceNode {
        kind: ChoiceKind::Float(FloatChoice {
            min_value: min,
            max_value: max,
            allow_nan: false,
            allow_infinity: false,
        }),
        value: ChoiceValue::Float(value),
        was_forced: false,
    }
}

fn int_node(value: i128, min: i128, max: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: min,
            max_value: max,
            shrink_towards: 0,
        }),
        value: ChoiceValue::Integer(value),
        was_forced: false,
    }
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
    shrinker.redistribute_numeric_pairs();
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
    shrinker.redistribute_numeric_pairs();
    // Engine stayed within validate bounds despite the accepting test_fn.
    match (
        &shrinker.current_nodes[0].value,
        &shrinker.current_nodes[1].value,
    ) {
        (ChoiceValue::Float(_), ChoiceValue::Integer(n)) => {
            assert!((1..=10).contains(n));
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
    let initial = vec![ChoiceNode {
        kind: ChoiceKind::Float(FloatChoice {
            min_value: f64::NEG_INFINITY,
            max_value: f64::INFINITY,
            allow_nan: true,
            allow_infinity: true,
        }),
        value: ChoiceValue::Float(f64::NAN),
        was_forced: false,
    }];
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
    shrinker.shrink_floats();
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
    shrinker.shrink_floats();
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
    shrinker.shrink_floats();
    match shrinker.current_nodes[0].value {
        ChoiceValue::Float(v) => assert_eq!(v, -3.0),
        _ => unreachable!(),
    }
}

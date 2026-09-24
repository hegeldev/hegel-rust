use super::*;
use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::Spans;
use crate::native::core::choices::IntegerChoice;
use crate::native::shrinker::Shrinker;
use alloc::boxed::Box;
use alloc::vec;

fn float_node(value: f64, min: f64, max: f64) -> ChoiceNode {
    ChoiceNode::float(
        FloatChoice {
            min_value: min,
            max_value: max,
            allow_nan: false,
            allow_infinity: false,
            smallest_nonzero_magnitude: 5e-324,
        },
        value,
        false,
    )
}

fn int_node(value: i128, min: i128, max: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(min),
            max_value: BigInt::from(max),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
        false,
    )
}

#[test]
fn redistribute_pair_below_shrink_target_uses_raise_left_direction() {
    let initial = vec![
        float_node(-3.0, -100.0, 100.0),
        float_node(5.0, -100.0, 100.0),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.redistribute_numeric_pairs()).unwrap();
    let (a, b) = match (
        &shrinker.current_nodes[0].value(),
        &shrinker.current_nodes[1].value(),
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
        Box::new(|run: ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.redistribute_numeric_pairs()).unwrap();
    match (
        &shrinker.current_nodes[0].value(),
        &shrinker.current_nodes[1].value(),
    ) {
        (ChoiceValue::Float(_), ChoiceValue::Integer(n)) => {
            assert!((1..=10).contains(&i128::try_from(n).unwrap()));
        }
        _ => unreachable!(),
    }
}

#[test]
fn shrink_floats_canonicalizes_nan_to_finite_when_predicate_admits() {
    let initial = vec![ChoiceNode::float(
        FloatChoice {
            min_value: f64::NEG_INFINITY,
            max_value: f64::INFINITY,
            allow_nan: true,
            allow_infinity: true,
            smallest_nonzero_magnitude: 5e-324,
        },
        f64::NAN,
        false,
    )];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: crate::native::shrinker::ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let interesting = nodes.iter().all(|n| match &n.value() {
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
    drive_no_yield(shrinker.shrink_floats()).unwrap();
    match shrinker.current_nodes[0].value() {
        ChoiceValue::Float(f) => assert_eq!(f, f64::MAX),
        _ => unreachable!(),
    }
}

#[test]
fn as_integer_ratio_recovers_simple_terminating_decimal() {
    assert_eq!(as_integer_ratio(0.5).unwrap(), Some((1, 2)));
    assert_eq!(as_integer_ratio(1.5).unwrap(), Some((3, 2)));
    assert_eq!(as_integer_ratio(2.0).unwrap(), Some((2, 1)));
    assert_eq!(as_integer_ratio(1024.0).unwrap(), Some((1024, 1)));
}

#[test]
fn as_integer_ratio_subnormal_decomposes_with_huge_denominator() {
    let smallest_subnormal = f64::from_bits(1);
    assert_eq!(as_integer_ratio(smallest_subnormal).unwrap(), None);
}

#[test]
fn as_integer_ratio_huge_value_overflows_to_none() {
    assert_eq!(as_integer_ratio(f64::MAX).unwrap(), None);
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
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let interesting =
                    matches!(nodes[0].value(), ChoiceValue::Float(v) if v < -1.0 && v.is_finite());
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.shrink_floats()).unwrap();
    match shrinker.current_nodes[0].value() {
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
    let v0 = -(1i64 << 60) as f64;
    let initial = vec![float_node(v0, -(1i128 << 61) as f64, -1.0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = matches!(
                    nodes[0].value(),
                    ChoiceValue::Float(v) if v <= -3.0 && v.is_finite()
                );
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.shrink_floats()).unwrap();
    match shrinker.current_nodes[0].value() {
        ChoiceValue::Float(v) => assert_eq!(v, -3.0),
        _ => unreachable!(),
    }
}

#[test]
fn shrink_floats_reaches_the_simplest_nonzero_value_of_a_bounded_range() {
    let initial = vec![float_node(7e-166, -1e-165, 1e-165)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let nonzero = matches!(nodes[0].value(), ChoiceValue::Float(v) if v != 0.0);
                (nonzero, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.shrink_floats()).unwrap();
    let (fc, v) = shrinker.current_nodes[0].data.as_float().unwrap();
    assert_eq!(v, 2f64.powi(-549));
    assert_eq!(v, fc.simplest_nonzero().unwrap());
}

fn scaling_shrinker(
    initial: Vec<ChoiceNode>,
    fails: impl Fn(&[ChoiceNode]) -> bool + Send + 'static,
) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (fails(nodes), nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    )
}

fn int_at(nodes: &[ChoiceNode], i: usize) -> i128 {
    match nodes[i].value() {
        ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
        _ => unreachable!(),
    }
}

fn float_at(nodes: &[ChoiceNode], i: usize) -> f64 {
    match nodes[i].value() {
        ChoiceValue::Float(f) => f,
        _ => unreachable!(),
    }
}

#[test]
fn scale_numeric_pairs_raises_the_later_integer_to_its_bound_keeping_the_product() {
    let mut shrinker = scaling_shrinker(
        vec![
            int_node(425_000, 0, 1_000_000_000_000),
            int_node(425, -1000, 1000),
        ],
        |nodes| int_at(nodes, 0) * int_at(nodes, 1) >= 180_625_000,
    );
    drive_no_yield(shrinker.scale_numeric_pairs()).unwrap();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 180_625);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 1000);
}

#[test]
fn scale_numeric_pairs_doubles_the_later_integer_when_its_bound_is_rejected() {
    let mut shrinker = scaling_shrinker(
        vec![
            int_node(425_000, 0, 1_000_000_000_000),
            int_node(425, -1000, 1000),
        ],
        |nodes| int_at(nodes, 0) * int_at(nodes, 1) >= 180_625_000 && int_at(nodes, 1) <= 900,
    );
    drive_no_yield(shrinker.scale_numeric_pairs()).unwrap();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 212_500);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 850);
}

#[test]
fn scale_numeric_pairs_rounds_the_earlier_integer_up_when_truncation_is_rejected() {
    let mut shrinker = scaling_shrinker(vec![int_node(7, 0, 100), int_node(3, 0, 10)], |nodes| {
        int_at(nodes, 0) * int_at(nodes, 1) >= 21
    });
    drive_no_yield(shrinker.scale_numeric_pairs()).unwrap();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 3);
    assert_eq!(int_at(&shrinker.current_nodes, 1), 10);
}

#[test]
fn scale_numeric_pairs_mirrors_a_pair_whose_earlier_integer_is_below_its_target() {
    let mut shrinker =
        scaling_shrinker(vec![int_node(-6, -10, 10), int_node(7, -10, 10)], |nodes| {
            int_at(nodes, 0) * int_at(nodes, 1) == -42
        });
    drive_no_yield(shrinker.scale_numeric_pairs()).unwrap();
    assert_eq!(int_at(&shrinker.current_nodes, 0), 6);
    assert_eq!(int_at(&shrinker.current_nodes, 1), -7);
}

#[test]
fn scale_numeric_pairs_raises_the_later_float_to_its_bound() {
    let from = 2f64.powi(1001);
    let to = -(f64::MAX - 2f64.powi(1000));
    let mut shrinker = scaling_shrinker(
        vec![
            float_node(from, -f64::MAX, f64::MAX),
            float_node(to, -f64::MAX, f64::MAX),
        ],
        |nodes| (float_at(nodes, 0) - float_at(nodes, 1)).is_infinite(),
    );
    drive_no_yield(shrinker.scale_numeric_pairs()).unwrap();
    assert!(float_at(&shrinker.current_nodes, 0) < from);
    assert_eq!(float_at(&shrinker.current_nodes, 1), -f64::MAX);
}

#[test]
fn scale_numeric_pairs_doubles_the_later_float_when_its_bound_is_rejected() {
    let mut shrinker = scaling_shrinker(
        vec![float_node(8.0, -1e6, 1e6), float_node(2.0, -1e6, 1e6)],
        |nodes| float_at(nodes, 0) * float_at(nodes, 1) >= 15.9 && float_at(nodes, 1) <= 5.0,
    );
    drive_no_yield(shrinker.scale_numeric_pairs()).unwrap();
    assert_eq!(float_at(&shrinker.current_nodes, 0), 4.0);
    assert_eq!(float_at(&shrinker.current_nodes, 1), 4.0);
}

#[test]
fn scale_numeric_pairs_mirrors_a_negative_earlier_float() {
    let mut shrinker = scaling_shrinker(
        vec![float_node(-8.0, -1e6, 1e6), float_node(2.0, -1e6, 1e6)],
        |nodes| float_at(nodes, 0) * float_at(nodes, 1) == -16.0 && float_at(nodes, 1).abs() <= 5.0,
    );
    drive_no_yield(shrinker.scale_numeric_pairs()).unwrap();
    assert_eq!(float_at(&shrinker.current_nodes, 0), 4.0);
    assert_eq!(float_at(&shrinker.current_nodes, 1), -4.0);
}

#[test]
fn scale_numeric_pairs_skips_mixed_pairs_and_draws_at_their_target() {
    let mut shrinker = scaling_shrinker(
        vec![
            int_node(5, 0, 10),
            float_node(2.0, -10.0, 10.0),
            int_node(0, 0, 10),
            float_node(0.0, -10.0, 10.0),
        ],
        |_| true,
    );
    drive_no_yield(shrinker.scale_numeric_pairs()).unwrap();
    assert_eq!(shrinker.calls, 0);
}

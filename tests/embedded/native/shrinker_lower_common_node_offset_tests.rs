//! Unit tests for `Shrinker::lower_common_node_offset`.

use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128, shrink_towards: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(i128::MIN),
            max_value: BigInt::from(i128::MAX),
            shrink_towards: BigInt::from(shrink_towards),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn int_value(node: &ChoiceNode) -> i128 {
    match &node.value {
        ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
        _ => unreachable!(),
    }
}

#[test]
fn lower_common_node_offset_noop_when_fewer_than_two_changes() {
    let initial = vec![int_node(5, 0), int_node(5, 0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    // No changes at all → set is empty → return immediately.
    shrinker.lower_common_node_offset().unwrap();
    assert!(shrinker.changed_nodes().is_empty());

    // Touch just one node.
    shrinker
        .consider(&[int_node(3, 0), int_node(5, 0)])
        .unwrap();
    assert_eq!(shrinker.changed_nodes().len(), 1);
    shrinker.lower_common_node_offset().unwrap();
    // One-element set: pass should leave it alone (short-circuits at
    // `len <= 1`).
    assert_eq!(int_value(&shrinker.current_nodes[0]), 3);
    assert_eq!(int_value(&shrinker.current_nodes[1]), 5);
}

#[test]
fn lower_common_node_offset_collapses_zig_zag_pair() {
    // Classic zig-zag: `abs(m - n) > 1` predicate keeps m and n locked at
    // distance > 1.  The pass should find the common offset (here 99) and
    // drive both toward 0 in a single shot.
    let initial = vec![int_node(100, 0), int_node(101, 0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let m = match &nodes[0].value {
                    ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
                    _ => unreachable!(),
                };
                let n = match &nodes[1].value {
                    ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
                    _ => unreachable!(),
                };
                (
                    m.abs_diff(n) == 1 && m >= 1 && n >= 1,
                    nodes.to_vec(),
                    Spans::new(),
                )
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    // Touch both nodes so the change set has cardinality 2.
    shrinker
        .consider(&[int_node(50, 0), int_node(51, 0)])
        .unwrap();
    assert_eq!(shrinker.changed_nodes().len(), 2);
    shrinker.lower_common_node_offset().unwrap();
    // Predicate accepts (1, 2): m = 1, n = 2, diff = 1.
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(int_value(&shrinker.current_nodes[1]), 2);
    // Change set cleared.
    assert!(shrinker.changed_nodes().is_empty());
}

#[test]
fn lower_common_node_offset_handles_negative_shrink_target() {
    // Both nodes shrink toward `-10`; current values are -5 and -7
    // (distances 5 and 3, offset 3).  Predicate accepts whenever the
    // values stay within radius 5 of -10 and differ by exactly 2.
    let initial = vec![int_node(-5, -10), int_node(-7, -10)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let a = int_value(&nodes[0]);
                let b = int_value(&nodes[1]);
                let ok = a.abs_diff(b) == 2 && a.abs_diff(-10) <= 5 && b.abs_diff(-10) <= 5;
                (ok, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker
        .consider(&[int_node(-9, -10), int_node(-11, -10)])
        .unwrap();
    assert_eq!(shrinker.changed_nodes().len(), 2);
    shrinker.lower_common_node_offset().unwrap();
    // Closest valid pair: distances (1, -1) under shrink_towards -10
    // ⇒ values (-9, -11) — but we want them as close to -10 as possible
    // with diff 2.  The algorithm probes the common offset down.
    let (a, b) = (
        int_value(&shrinker.current_nodes[0]),
        int_value(&shrinker.current_nodes[1]),
    );
    assert_eq!(a.abs_diff(b), 2);
    // Both nodes ended at the minimum distance permitted by the
    // predicate.
    assert!(a.abs_diff(-10) <= 1);
    assert!(b.abs_diff(-10) <= 1);
}

#[test]
fn lower_common_node_offset_skips_non_integer_nodes() {
    use crate::native::core::choices::{BooleanChoice, FloatChoice};
    let bool_node = ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(true),
        false,
    );
    let float_node = ChoiceNode::new(
        ChoiceKind::Float(FloatChoice {
            min_value: f64::NEG_INFINITY,
            max_value: f64::INFINITY,
            allow_nan: false,
            allow_infinity: false,
            smallest_nonzero_magnitude: 5e-324,
        }),
        ChoiceValue::Float(3.0),
        false,
    );
    let initial = vec![int_node(5, 0), bool_node, float_node, int_node(7, 0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    // Manufacture a multi-index change set including the bool and float.
    shrinker
        .consider(&[
            int_node(3, 0),
            ChoiceNode::new(
                ChoiceKind::Boolean(BooleanChoice),
                ChoiceValue::Boolean(false),
                false,
            ),
            ChoiceNode::new(
                ChoiceKind::Float(FloatChoice {
                    min_value: f64::NEG_INFINITY,
                    max_value: f64::INFINITY,
                    allow_nan: false,
                    allow_infinity: false,
                    smallest_nonzero_magnitude: 5e-324,
                }),
                ChoiceValue::Float(0.0),
                false,
            ),
            int_node(2, 0),
        ])
        .unwrap();
    assert!(shrinker.changed_nodes().len() >= 3);
    // Only the two integer nodes participate in lowering.  After the
    // consider() above, distances from shrink_towards (0) are 3 and 2 —
    // their common offset is 2.  Lowering the offset to zero keeps the
    // residual `[1, 0]`, so the values end at 1 and 0.
    shrinker.lower_common_node_offset().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(int_value(&shrinker.current_nodes[3]), 0);
}

fn bytes_node(value: Vec<u8>) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Bytes(crate::native::core::choices::BytesChoice {
            min_size: 0,
            max_size: 1_000_000,
        }),
        ChoiceValue::Bytes(value),
        false,
    )
}

#[test]
fn index_passes_skip_sequence_nodes_without_blowup() {
    let initial = vec![bytes_node(vec![7u8; 300]), int_node(5, 0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.lower_and_bump().unwrap();
    shrinker.try_shortening_via_increment().unwrap();
    shrinker.mutate_and_shrink().unwrap();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::Bytes(v) => assert_eq!(v.len(), 300, "long value should be left untouched"),
        other => panic!("expected bytes, got {other:?}"),
    }
}

/// Offset lowering must fire *during* the value-minimization passes
/// (Hypothesis runs it after every successful `try_shrinking_nodes`),
/// not only as a separately scheduled pass: with `|m - n| <= 1` and
/// m ~ n ~ 100_000, individual minimization can only zig-zag down by
/// ~2 per accepted shrink, so the run-global 500-improvement budget is
/// exhausted long before zero if the pass scheduler never gets a turn.
/// The linked nodes sit five positions apart, outside
/// `lower_integers_together`'s 3-node pairing window, so the common
/// offset is the only mechanism that can collapse them.
#[test]
fn linked_integers_collapse_within_the_minimize_pass() {
    use crate::native::core::choices::BooleanChoice;
    let bool_node = || {
        ChoiceNode::new(
            ChoiceKind::Boolean(BooleanChoice),
            ChoiceValue::Boolean(false),
            false,
        )
    };
    // Distinct values so `shrink_duplicates` can't pair them either.
    let initial = vec![
        int_node(100_000, 0),
        bool_node(),
        bool_node(),
        bool_node(),
        bool_node(),
        int_node(100_001, 0),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let ok = nodes.len() >= 6
                    && matches!(
                        (&nodes[0].value, &nodes[5].value),
                        (ChoiceValue::Integer(m), ChoiceValue::Integer(n))
                            if {
                                let m = i128::try_from(m).unwrap();
                                let n = i128::try_from(n).unwrap();
                                // Floor at 500 so the all-zero candidate
                                // (zero_choices solves symmetric cases in
                                // one shot) is not interesting.
                                m.abs_diff(n) <= 1 && m >= 500 && n >= 500
                            }
                    );
                (ok, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink();
    assert_eq!(
        (
            int_value(&shrinker.current_nodes[0]),
            int_value(&shrinker.current_nodes[5])
        ),
        (500, 500),
        "the linked pair must collapse to (500, 500) within the improvement budget"
    );
}

/// Distances beyond u128 can't go through the Integer move set; the pass
/// skips such pairs and leaves them to the per-node passes.
#[test]
fn lower_common_node_offset_skips_offsets_beyond_u128() {
    let huge = BigInt::from(BigInt::from(2).magnitude().pow(200));
    let huge_node = |v: &BigInt| {
        ChoiceNode::new(
            ChoiceKind::Integer(IntegerChoice {
                min_value: -(&huge) * BigInt::from(2),
                max_value: (&huge) * BigInt::from(2),
                shrink_towards: BigInt::from(0),
            }),
            ChoiceValue::Integer(v.clone()),
            false,
        )
    };
    let v1 = &huge + BigInt::from(1);
    let v2 = &huge + BigInt::from(2);
    let initial = vec![huge_node(&v1), huge_node(&v2)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    // Mark both nodes changed so the pass engages.
    shrinker
        .consider(&[huge_node(&(&v1 - BigInt::from(1))), huge_node(&v2)])
        .unwrap();
    shrinker
        .consider(&[
            huge_node(&(&v1 - BigInt::from(1))),
            huge_node(&(&v2 - BigInt::from(1))),
        ])
        .unwrap();
    assert_eq!(shrinker.changed_nodes().len(), 2);
    shrinker.lower_common_node_offset().unwrap();
    // The offset (≈ 2^200) exceeds u128, so the pass backs off without
    // touching the values further.
    match &shrinker.current_nodes[0].value {
        ChoiceValue::Integer(v) => assert_eq!(*v, &v1 - BigInt::from(1)),
        _ => unreachable!(),
    }
}

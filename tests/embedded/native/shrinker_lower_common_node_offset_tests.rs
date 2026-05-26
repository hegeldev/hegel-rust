//! Unit tests for `Shrinker::lower_common_node_offset`.

use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128, shrink_towards: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: i128::MIN,
            max_value: i128::MAX,
            shrink_towards,
        }),
        value: ChoiceValue::Integer(value),
        was_forced: false,
    }
}

fn int_value(node: &ChoiceNode) -> i128 {
    match node.value {
        ChoiceValue::Integer(v) => v,
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
    shrinker.lower_common_node_offset();
    assert!(shrinker.changed_nodes().is_empty());

    // Touch just one node.
    shrinker.consider(&[int_node(3, 0), int_node(5, 0)]);
    assert_eq!(shrinker.changed_nodes().len(), 1);
    shrinker.lower_common_node_offset();
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
                let m = match nodes[0].value {
                    ChoiceValue::Integer(v) => v,
                    _ => unreachable!(),
                };
                let n = match nodes[1].value {
                    ChoiceValue::Integer(v) => v,
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
    shrinker.consider(&[int_node(50, 0), int_node(51, 0)]);
    assert_eq!(shrinker.changed_nodes().len(), 2);
    shrinker.lower_common_node_offset();
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
    shrinker.consider(&[int_node(-9, -10), int_node(-11, -10)]);
    assert_eq!(shrinker.changed_nodes().len(), 2);
    shrinker.lower_common_node_offset();
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
    let bool_node = ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(true),
        was_forced: false,
    };
    let float_node = ChoiceNode {
        kind: ChoiceKind::Float(FloatChoice {
            min_value: f64::NEG_INFINITY,
            max_value: f64::INFINITY,
            allow_nan: false,
            allow_infinity: false,
        }),
        value: ChoiceValue::Float(3.0),
        was_forced: false,
    };
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
    shrinker.consider(&[
        int_node(3, 0),
        ChoiceNode {
            kind: ChoiceKind::Boolean(BooleanChoice),
            value: ChoiceValue::Boolean(false),
            was_forced: false,
        },
        ChoiceNode {
            kind: ChoiceKind::Float(FloatChoice {
                min_value: f64::NEG_INFINITY,
                max_value: f64::INFINITY,
                allow_nan: false,
                allow_infinity: false,
            }),
            value: ChoiceValue::Float(0.0),
            was_forced: false,
        },
        int_node(2, 0),
    ]);
    assert!(shrinker.changed_nodes().len() >= 3);
    // Only the two integer nodes participate in lowering.  After the
    // consider() above, distances from shrink_towards (0) are 3 and 2 —
    // their common offset is 2.  Lowering the offset to zero keeps the
    // residual `[1, 0]`, so the values end at 1 and 0.
    shrinker.lower_common_node_offset();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(int_value(&shrinker.current_nodes[3]), 0);
}

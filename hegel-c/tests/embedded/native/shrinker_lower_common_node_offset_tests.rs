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
    shrinker.lower_common_node_offset().unwrap();
    assert!(shrinker.changed_nodes().is_empty());

    shrinker
        .consider(&[int_node(3, 0), int_node(5, 0)])
        .unwrap();
    assert_eq!(shrinker.changed_nodes().len(), 1);
    shrinker.lower_common_node_offset().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 3);
    assert_eq!(int_value(&shrinker.current_nodes[1]), 5);
}

#[test]
fn lower_common_node_offset_collapses_zig_zag_pair() {
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
    shrinker
        .consider(&[int_node(50, 0), int_node(51, 0)])
        .unwrap();
    assert_eq!(shrinker.changed_nodes().len(), 2);
    shrinker.lower_common_node_offset().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(int_value(&shrinker.current_nodes[1]), 2);
    assert!(shrinker.changed_nodes().is_empty());
}

#[test]
fn lower_common_node_offset_handles_negative_shrink_target() {
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
    let (a, b) = (
        int_value(&shrinker.current_nodes[0]),
        int_value(&shrinker.current_nodes[1]),
    );
    assert_eq!(a.abs_diff(b), 2);
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

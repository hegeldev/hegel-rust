use std::sync::Arc;

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, CloneRecord, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(100),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(value),
        false,
    )
}

fn clone_node(children: Vec<ChoiceNode>) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Clone,
        ChoiceValue::Clone(Arc::new(CloneRecord::from_run(
            children,
            Vec::new(),
            Vec::new(),
        ))),
        false,
    )
}

fn child_nodes_of(node: &ChoiceNode) -> &[ChoiceNode] {
    let ChoiceValue::Clone(record) = &node.value else {
        panic!("expected a clone node, got {node:?}");
    };
    record.realized_nodes().unwrap()
}

fn int_value(node: &ChoiceNode) -> i128 {
    match &node.value {
        ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
        _ => panic!("expected an integer node, got {node:?}"),
    }
}

/// Interesting iff the node at `path` (indices through nested clone nodes,
/// final index into the innermost stream) is an integer >= `min`.
fn int_at_path_at_least(nodes: &[ChoiceNode], path: &[usize], min: i128) -> bool {
    let (&last, dirs) = path.split_last().unwrap();
    let mut current = nodes;
    for &d in dirs {
        let Some(node) = current.get(d) else {
            return false;
        };
        let ChoiceValue::Clone(record) = &node.value else {
            return false;
        };
        let Some(children) = record.realized_nodes() else {
            return false;
        };
        current = children;
    }
    match current.get(last).map(|n| &n.value) {
        Some(ChoiceValue::Integer(v)) => *v >= BigInt::from(min),
        _ => false,
    }
}

#[test]
fn nested_shrink_minimizes_values_inside_clone_nodes() {
    let initial = vec![clone_node(vec![int_node(47), bool_node(true)])];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (
                int_at_path_at_least(nodes, &[0, 0], 10),
                nodes.to_vec(),
                Spans::new(),
            ),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.shrink());
    assert_eq!(shrinker.current_nodes.len(), 1);
    let child = child_nodes_of(&shrinker.current_nodes[0]);
    assert_eq!(child.len(), 1);
    assert_eq!(int_value(&child[0]), 10);
}

#[test]
fn nested_shrink_recurses_into_clones_inside_clones() {
    let initial = vec![clone_node(vec![
        int_node(30),
        clone_node(vec![int_node(20), bool_node(true)]),
    ])];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (
                int_at_path_at_least(nodes, &[0, 1, 0], 5),
                nodes.to_vec(),
                Spans::new(),
            ),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.shrink());
    assert_eq!(shrinker.current_nodes.len(), 1);
    let child = child_nodes_of(&shrinker.current_nodes[0]);
    assert_eq!(child.len(), 2);
    assert_eq!(int_value(&child[0]), 0);
    let inner = child_nodes_of(&child[1]);
    assert_eq!(inner.len(), 1);
    assert_eq!(int_value(&inner[0]), 5);
}

#[test]
fn unconstrained_clone_nodes_are_deleted_outright() {
    let initial = vec![
        int_node(3),
        clone_node(vec![int_node(9), bool_node(true), int_node(2)]),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (
                int_at_path_at_least(nodes, &[0], 3),
                nodes.to_vec(),
                Spans::new(),
            ),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.shrink());
    assert_eq!(shrinker.current_nodes.len(), 1);
    assert_eq!(int_value(&shrinker.current_nodes[0]), 3);
}

#[test]
fn nested_shrink_tolerates_runs_that_drop_the_clone_node() {
    let initial = vec![clone_node(vec![int_node(12)])];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                if int_at_path_at_least(nodes, &[0, 0], 10) {
                    (true, nodes.to_vec(), Spans::new())
                } else {
                    (false, Vec::new(), Spans::new())
                }
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.shrink());
    let child = child_nodes_of(&shrinker.current_nodes[0]);
    assert_eq!(int_value(&child[0]), 10);
}

#[test]
fn values_only_clone_records_are_left_alone() {
    let initial = vec![ChoiceNode::new(
        ChoiceKind::Clone,
        ChoiceValue::Clone(Arc::new(CloneRecord::from_values(vec![
            ChoiceValue::Boolean(true),
        ]))),
        false,
    )];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial.clone(),
        Spans::new(),
    );
    drive_no_yield(shrinker.shrink_clone_streams()).unwrap();
    assert_eq!(shrinker.calls, 0);
    assert_eq!(shrinker.current_nodes, initial);
}

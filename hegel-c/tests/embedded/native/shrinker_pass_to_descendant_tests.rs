//! Unit tests for `Shrinker::pass_to_descendant`.

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Span, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(i128::MIN),
            max_value: BigInt::from(i128::MAX),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn lab(start: usize, end: usize, label: &str) -> Span {
    Span {
        start,
        end,
        label: label.to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    }
}

#[test]
fn pass_to_descendant_replaces_outer_with_inner_same_label() {
    let initial = vec![
        int_node(1),
        int_node(2),
        int_node(3),
        int_node(4),
        int_node(5),
    ];
    let mut spans = Spans::new();
    spans.push(lab(0, 5, "tree"));
    spans.push(lab(2, 4, "tree"));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.pass_to_descendant()).unwrap();

    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![3, 4]);
}

#[test]
fn pass_to_descendant_skips_different_labels() {
    let initial = vec![int_node(1), int_node(2), int_node(3)];
    let mut spans = Spans::new();
    spans.push(lab(0, 3, "outer"));
    spans.push(lab(1, 2, "inner"));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.pass_to_descendant()).unwrap();
    assert_eq!(shrinker.current_nodes.len(), 3);
}

#[test]
fn pass_to_descendant_skips_equal_length_descendant() {
    let initial = vec![int_node(7), int_node(8)];
    let mut spans = Spans::new();
    spans.push(lab(0, 2, "tree"));
    spans.push(lab(0, 2, "tree"));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.pass_to_descendant()).unwrap();
    assert_eq!(shrinker.current_nodes.len(), 2);
}

#[test]
fn pass_to_descendant_handles_multiple_descendants() {
    let initial = vec![
        int_node(1),
        int_node(2),
        int_node(3),
        int_node(4),
        int_node(5),
        int_node(6),
    ];
    let mut spans = Spans::new();
    spans.push(lab(0, 6, "tree"));
    spans.push(lab(1, 4, "tree"));
    spans.push(lab(2, 3, "tree"));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (nodes.len() <= 2, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.pass_to_descendant()).unwrap();
    assert!(shrinker.current_nodes.len() <= 2);
}

#[test]
fn pass_to_descendant_safe_when_indices_outrange_after_shrink() {
    let initial = vec![int_node(1), int_node(2)];
    let mut spans = Spans::new();
    spans.push(lab(0, 5, "tree"));
    spans.push(lab(1, 3, "tree"));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.pass_to_descendant()).unwrap();
    assert_eq!(shrinker.current_nodes.len(), 2);
}

//! Unit tests for `Shrinker::reorder_spans`.
//!
//! Hypothesis reference: `shrinker.py:1810-1855`.

use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Span, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: i128::MIN,
            max_value: i128::MAX,
            shrink_towards: 0,
        }),
        value: ChoiceValue::Integer(value),
        was_forced: false,
    }
}

fn sib(start: usize, end: usize, label: &str, parent: Option<usize>) -> Span {
    Span {
        start,
        end,
        label: label.to_string(),
        depth: 0,
        parent,
        discarded: false,
    }
}

#[test]
fn reorder_spans_sorts_same_label_siblings() {
    // Two single-int sibling spans under no parent.  After sorting, the
    // smaller integer should come first.
    let initial = vec![int_node(3), int_node(1)];
    let mut spans = Spans::new();
    spans.push(sib(0, 1, "item", None));
    spans.push(sib(1, 2, "item", None));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    shrinker.reorder_spans();
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => *v,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![1, 3]);
}

#[test]
fn reorder_spans_skips_singleton_groups() {
    // A single sibling under each label — nothing to permute.
    let initial = vec![int_node(7), int_node(3)];
    let mut spans = Spans::new();
    spans.push(sib(0, 1, "a", None));
    spans.push(sib(1, 2, "b", None));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    shrinker.reorder_spans();
    // Order unchanged.
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => *v,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![7, 3]);
}

#[test]
fn reorder_spans_handles_multi_node_siblings() {
    // Three same-label siblings, each spanning two nodes.  After
    // reordering, the lex-smallest (1,9) comes first, then (3,5), then
    // (7,2).
    let initial = vec![
        int_node(7),
        int_node(2),
        int_node(3),
        int_node(5),
        int_node(1),
        int_node(9),
    ];
    let mut spans = Spans::new();
    spans.push(sib(0, 2, "pair", None));
    spans.push(sib(2, 4, "pair", None));
    spans.push(sib(4, 6, "pair", None));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    shrinker.reorder_spans();
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => *v,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![1, 9, 3, 5, 7, 2]);
}

#[test]
fn reorder_spans_safe_with_stale_endpoints() {
    // The closure returns shorter actual_nodes than the candidate; the
    // recorded span endpoints point past the end of current_nodes.
    let initial = vec![int_node(5), int_node(3)];
    let mut spans = Spans::new();
    spans.push(sib(0, 5, "wide", None));
    spans.push(sib(5, 10, "wide", None));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    shrinker.reorder_spans();
    // No panic; nothing changed.
    assert_eq!(shrinker.current_nodes.len(), 2);
}

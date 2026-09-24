//! Unit tests for `Shrinker::reorder_spans`.

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceNode, ChoiceValue, Span, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(i128::MIN),
            max_value: BigInt::from(i128::MAX),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
        false,
    )
}

fn sib(start: usize, end: usize, label: u64, parent: Option<usize>) -> Span {
    Span {
        start,
        end,
        label,
        depth: 0,
        parent,
        discarded: false,
    }
}

#[test]
fn reorder_spans_sorts_same_label_siblings() {
    let initial = vec![int_node(3), int_node(1)];
    let mut spans = Spans::new();
    spans.push(sib(0, 1, 1, None));
    spans.push(sib(1, 2, 1, None));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.reorder_spans()).unwrap();
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value() {
            ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![1, 3]);
}

#[test]
fn reorder_spans_stops_when_deadline_passed() {
    use core::time::Duration;

    use crate::sys::Instant;
    let initial = vec![int_node(3), int_node(1)];
    let mut spans = Spans::new();
    spans.push(sib(0, 1, 1, None));
    spans.push(sib(1, 2, 1, None));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    shrinker.deadline = Some(Instant::now().unwrap() - Duration::from_secs(1));
    assert!(drive_no_yield(shrinker.reorder_spans()).is_err());
    assert!(shrinker.timed_out);
}

#[test]
fn reorder_spans_skips_singleton_groups() {
    let initial = vec![int_node(7), int_node(3)];
    let mut spans = Spans::new();
    spans.push(sib(0, 1, 1, None));
    spans.push(sib(1, 2, 2, None));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.reorder_spans()).unwrap();
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value() {
            ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![7, 3]);
}

#[test]
fn reorder_spans_handles_multi_node_siblings() {
    let initial = vec![
        int_node(7),
        int_node(2),
        int_node(3),
        int_node(5),
        int_node(1),
        int_node(9),
    ];
    let mut spans = Spans::new();
    spans.push(sib(0, 2, 1, None));
    spans.push(sib(2, 4, 1, None));
    spans.push(sib(4, 6, 1, None));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.reorder_spans()).unwrap();
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value() {
            ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![1, 9, 3, 5, 7, 2]);
}

#[test]
fn reorder_spans_safe_with_stale_endpoints() {
    let initial = vec![int_node(5), int_node(3)];
    let mut spans = Spans::new();
    spans.push(sib(0, 5, 1, None));
    spans.push(sib(5, 10, 1, None));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.reorder_spans()).unwrap();
    assert_eq!(shrinker.current_nodes.len(), 2);
}

#[test]
fn reorder_spans_survives_spans_shrinking_between_label_groups() {
    let initial = vec![int_node(3), int_node(1), int_node(9), int_node(7)];
    let mut spans = Spans::new();
    spans.push(sib(0, 1, 1, None));
    spans.push(sib(1, 2, 1, None));
    spans.push(sib(2, 3, 2, None));
    spans.push(sib(3, 4, 2, None));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let mut new_spans = Spans::new();
                new_spans.push(sib(0, 1, 3, None));
                new_spans.push(sib(1, 2, 3, None));
                new_spans.push(sib(2, 3, 3, None));
                (true, nodes.to_vec(), new_spans)
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.reorder_spans()).unwrap();
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value() {
            ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![1, 3, 9, 7]);
}

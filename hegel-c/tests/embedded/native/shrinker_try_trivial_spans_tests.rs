//! Unit tests for `Shrinker::try_trivial_spans`.

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

fn forced_int_node(value: i128) -> ChoiceNode {
    let mut n = int_node(value);
    n.was_forced = true;
    n
}

fn span(start: usize, end: usize) -> Span {
    Span {
        start,
        end,
        label: "test".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    }
}

#[test]
fn try_trivial_spans_zeroes_non_forced_children() {
    let initial = vec![int_node(7), int_node(8), int_node(9)];
    let mut spans = Spans::new();
    spans.push(span(0, 3));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let s = {
                    let mut s = Spans::new();
                    s.push(span(0, nodes.len()));
                    s
                };
                (true, nodes.to_vec(), s)
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.try_trivial_spans()).unwrap();
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![0, 0, 0]);
}

#[test]
fn try_trivial_spans_preserves_forced_children() {
    let initial = vec![int_node(7), forced_int_node(8), int_node(9)];
    let mut spans = Spans::new();
    spans.push(span(0, 3));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let s = {
                    let mut s = Spans::new();
                    s.push(span(0, nodes.len()));
                    s
                };
                (true, nodes.to_vec(), s)
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.try_trivial_spans()).unwrap();
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![0, 8, 0]);
}

#[test]
fn try_trivial_spans_skips_already_trivial_span() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    let calls = Arc::new(AtomicU32::new(0));
    let calls_inside = Arc::clone(&calls);

    let initial = vec![int_node(0), int_node(0)];
    let mut spans = Spans::new();
    spans.push(span(0, 2));

    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| {
            calls_inside.fetch_add(1, Ordering::Relaxed);
            match run {
                ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
                ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
            }
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.try_trivial_spans()).unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn try_trivial_spans_handles_oversized_span_end() {
    let initial = vec![int_node(5)];
    let mut spans = Spans::new();
    spans.push(span(0, 99));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.try_trivial_spans()).unwrap();
    assert_eq!(shrinker.current_nodes.len(), 1);
    match &shrinker.current_nodes[0].value {
        ChoiceValue::Integer(v) => assert_eq!(i128::try_from(v).unwrap(), 5),
        _ => unreachable!(),
    }
}

#[test]
fn try_trivial_spans_retries_with_realised_span_content() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    let initial = vec![int_node(5), int_node(5), int_node(7)];
    let mut spans = Spans::new();
    spans.push(span(0, 2));

    let call_count = Arc::new(AtomicU32::new(0));
    let cc = Arc::clone(&call_count);
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let n = cc.fetch_add(1, Ordering::Relaxed);
                if n == 0 {
                    let realised = vec![int_node(2), nodes[2].clone()];
                    let mut s = Spans::new();
                    s.push(span(0, 1));
                    (false, realised, s)
                } else {
                    let mut s = Spans::new();
                    s.push(span(0, nodes.len().saturating_sub(1)));
                    (true, nodes.to_vec(), s)
                }
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.try_trivial_spans()).unwrap();
    assert_eq!(call_count.load(Ordering::Relaxed), 2);
    assert_eq!(shrinker.current_nodes.len(), 2);
    match (
        &shrinker.current_nodes[0].value,
        &shrinker.current_nodes[1].value,
    ) {
        (ChoiceValue::Integer(a), ChoiceValue::Integer(b)) => {
            assert_eq!(i128::try_from(a.clone()).unwrap(), 2);
            assert_eq!(i128::try_from(b.clone()).unwrap(), 7);
        }
        _ => unreachable!(),
    }
}

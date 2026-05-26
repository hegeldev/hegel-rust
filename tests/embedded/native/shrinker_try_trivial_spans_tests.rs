//! Unit tests for `Shrinker::try_trivial_spans`.

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
        Box::new(|run| match run {
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
    shrinker.try_trivial_spans();
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => *v,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![0, 0, 0]);
}

#[test]
fn try_trivial_spans_preserves_forced_children() {
    // The middle node is forced; even when we zero the span, it stays at 8.
    let initial = vec![int_node(7), forced_int_node(8), int_node(9)];
    let mut spans = Spans::new();
    spans.push(span(0, 3));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
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
    shrinker.try_trivial_spans();
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => *v,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![0, 8, 0]);
}

#[test]
fn try_trivial_spans_skips_already_trivial_span() {
    // Both children are already at their simplest; the pass shouldn't run
    // any test cases.  We assert by counting closure invocations.
    use std::cell::Cell;
    use std::rc::Rc;
    let calls = Rc::new(Cell::new(0u32));
    let calls_inside = Rc::clone(&calls);

    let initial = vec![int_node(0), int_node(0)];
    let mut spans = Spans::new();
    spans.push(span(0, 2));

    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| {
            calls_inside.set(calls_inside.get() + 1);
            match run {
                ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
                ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
            }
        }),
        initial,
        spans,
    );
    shrinker.try_trivial_spans();
    // No test invocations: the span was trivial up front.
    assert_eq!(calls.get(), 0);
}

#[test]
fn try_trivial_spans_handles_oversized_span_end() {
    // A pathological span whose `end > nodes.len()` (e.g. inherited from
    // a previous shrink) must not panic; the pass should skip it.
    let initial = vec![int_node(5)];
    let mut spans = Spans::new();
    spans.push(span(0, 99));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    shrinker.try_trivial_spans();
    // No change — the oversized span was skipped.
    assert_eq!(shrinker.current_nodes.len(), 1);
    match shrinker.current_nodes[0].value {
        ChoiceValue::Integer(v) => assert_eq!(v, 5),
        _ => unreachable!(),
    }
}

#[test]
fn try_trivial_spans_retries_with_realised_span_content() {
    // First attempt: simplify span 0 → predicate rejects (uninteresting).
    // Closure also reports a different actual realisation (shorter span
    // content) — the pass should splice that realised content back in
    // and retry, this time succeeding.
    use std::cell::Cell;
    use std::rc::Rc;

    let initial = vec![int_node(5), int_node(5), int_node(7)];
    let mut spans = Spans::new();
    spans.push(span(0, 2));

    let call_count = Rc::new(Cell::new(0u32));
    let cc = Rc::clone(&call_count);
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let n = cc.get();
                cc.set(n + 1);
                if n == 0 {
                    // First attempt: simplest span → reject.  Pretend the
                    // run "actually" produced a single-node span.
                    let realised = vec![int_node(2), nodes[2].clone()];
                    let mut s = Spans::new();
                    s.push(span(0, 1));
                    (false, realised, s)
                } else {
                    // Retry attempt: prefix + realised replacement + suffix.
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
    shrinker.try_trivial_spans();
    assert_eq!(call_count.get(), 2);
    // Retry succeeded — the spliced sequence is now the current target.
    assert_eq!(shrinker.current_nodes.len(), 2);
    match (
        &shrinker.current_nodes[0].value,
        &shrinker.current_nodes[1].value,
    ) {
        (ChoiceValue::Integer(a), ChoiceValue::Integer(b)) => {
            assert_eq!(*a, 2);
            assert_eq!(*b, 7);
        }
        _ => unreachable!(),
    }
}

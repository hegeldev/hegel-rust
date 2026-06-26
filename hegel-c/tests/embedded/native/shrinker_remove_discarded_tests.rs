//! Unit tests for `Shrinker::remove_discarded`.

use crate::native::bignum::BigInt;
use std::cell::RefCell;
use std::rc::Rc;

use crate::native::core::choices::{BooleanChoice, IntegerChoice};
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

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(value),
        false,
    )
}

fn span(start: usize, end: usize, discarded: bool) -> Span {
    Span {
        start,
        end,
        label: "test".to_string(),
        depth: 0,
        parent: None,
        discarded,
    }
}

type BoxedTestFn = Box<dyn FnMut(ShrinkRun) -> (bool, Vec<ChoiceNode>, Spans) + 'static>;

/// Closure that always accepts and returns its candidate verbatim with
/// the supplied spans.
fn accepting_test_fn(spans_after: Spans) -> BoxedTestFn {
    let cell: Rc<RefCell<Spans>> = Rc::new(RefCell::new(spans_after));
    Box::new(move |run| match run {
        ShrinkRun::Full(nodes) => (true, nodes.to_vec(), cell.borrow().clone()),
        ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
    })
}

#[test]
fn remove_discarded_returns_true_when_no_discards() {
    let initial = vec![int_node(7), int_node(8)];
    let mut spans = Spans::new();
    spans.push(span(0, 2, false));
    let mut shrinker = Shrinker::with_probe(accepting_test_fn(Spans::new()), initial, spans);
    assert!(shrinker.remove_discarded().unwrap());
    assert_eq!(shrinker.current_nodes.len(), 2);
}

#[test]
fn remove_discarded_skips_zero_length_discarded_span() {
    let initial = vec![int_node(7)];
    let mut spans = Spans::new();
    spans.push(span(0, 0, true));
    let mut shrinker = Shrinker::with_probe(accepting_test_fn(Spans::new()), initial, spans);
    assert!(shrinker.remove_discarded().unwrap());
    assert_eq!(shrinker.current_nodes.len(), 1);
}

#[test]
fn remove_discarded_deletes_a_single_discarded_region() {
    let initial = vec![int_node(1), int_node(2), int_node(3), int_node(4)];
    let mut spans = Spans::new();
    spans.push(span(0, 4, false));
    spans.push(span(1, 3, true));

    let mut shrinker = Shrinker::with_probe(accepting_test_fn(Spans::new()), initial, spans);
    assert!(shrinker.remove_discarded().unwrap());
    assert_eq!(shrinker.current_nodes.len(), 2);
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![1, 4]);
}

#[test]
fn remove_discarded_deletes_non_overlapping_regions_in_reverse() {
    let initial = vec![
        int_node(1),
        int_node(2),
        int_node(3),
        int_node(4),
        int_node(5),
        int_node(6),
        int_node(7),
    ];
    let mut spans = Spans::new();
    spans.push(span(0, 7, false));
    spans.push(span(1, 3, true));
    spans.push(span(5, 7, true));

    let mut shrinker = Shrinker::with_probe(accepting_test_fn(Spans::new()), initial, spans);
    assert!(shrinker.remove_discarded().unwrap());
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![1, 4, 5]);
}

#[test]
fn remove_discarded_skips_nested_discarded_spans() {
    let initial = vec![
        int_node(0),
        int_node(1),
        int_node(2),
        int_node(3),
        int_node(4),
        int_node(5),
    ];
    let mut spans = Spans::new();
    spans.push(span(0, 6, false));
    spans.push(span(1, 5, true));
    spans.push(span(2, 3, true));

    let mut shrinker = Shrinker::with_probe(accepting_test_fn(Spans::new()), initial, spans);
    assert!(shrinker.remove_discarded().unwrap());
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![0, 5]);
}

#[test]
fn remove_discarded_returns_false_when_consider_rejects() {
    let initial = vec![bool_node(true), bool_node(true)];
    let mut spans = Spans::new();
    spans.push(span(0, 2, false));
    spans.push(span(0, 2, true));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (false, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    assert!(!shrinker.remove_discarded().unwrap());
    assert_eq!(shrinker.current_nodes.len(), 2);
}

#[test]
fn remove_discarded_iterates_when_new_target_still_has_discards() {
    let initial = vec![int_node(1), int_node(2), int_node(3), int_node(4)];
    let mut spans = Spans::new();
    spans.push(span(0, 4, false));
    spans.push(span(2, 4, true));

    let next_spans = {
        let mut s = Spans::new();
        s.push(span(0, 2, true));
        s
    };
    let mut after_first_call = false;
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let spans = if !after_first_call {
                    after_first_call = true;
                    next_spans.clone()
                } else {
                    Spans::new()
                };
                (true, nodes.to_vec(), spans)
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    assert!(shrinker.remove_discarded().unwrap());
    assert_eq!(shrinker.current_nodes.len(), 0);
}

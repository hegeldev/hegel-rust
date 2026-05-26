//! Unit tests for `Shrinker::remove_discarded`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::native::core::choices::{BooleanChoice, IntegerChoice};
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

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(value),
        was_forced: false,
    }
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
    assert!(shrinker.remove_discarded());
    assert_eq!(shrinker.current_nodes.len(), 2);
}

#[test]
fn remove_discarded_skips_zero_length_discarded_span() {
    // `span.end > span.start` is required — empty discarded spans never
    // contribute to the deletion list (the `ex.choice_count > 0` guard).
    let initial = vec![int_node(7)];
    let mut spans = Spans::new();
    spans.push(span(0, 0, true)); // zero-length discarded
    let mut shrinker = Shrinker::with_probe(accepting_test_fn(Spans::new()), initial, spans);
    assert!(shrinker.remove_discarded());
    assert_eq!(shrinker.current_nodes.len(), 1);
}

#[test]
fn remove_discarded_deletes_a_single_discarded_region() {
    // current_nodes = [a, b, c, d]; spans = [0..4 root, 1..3 discarded].
    // Expect remove_discarded to attempt deletion of indices 1..3,
    // leaving [a, d].
    let initial = vec![int_node(1), int_node(2), int_node(3), int_node(4)];
    let mut spans = Spans::new();
    spans.push(span(0, 4, false));
    spans.push(span(1, 3, true));

    let mut shrinker = Shrinker::with_probe(accepting_test_fn(Spans::new()), initial, spans);
    assert!(shrinker.remove_discarded());
    // Closure now returns no discards, so the next loop iteration finds
    // an empty list and exits.
    assert_eq!(shrinker.current_nodes.len(), 2);
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => *v,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![1, 4]);
}

#[test]
fn remove_discarded_deletes_non_overlapping_regions_in_reverse() {
    // Two disjoint discarded regions: [1..3) and [5..7).  Both should be
    // deleted in a single attempt, in reverse order so earlier deletions
    // don't shift the later indices.
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
    assert!(shrinker.remove_discarded());
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => *v,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![1, 4, 5]);
}

#[test]
fn remove_discarded_skips_nested_discarded_spans() {
    // The outer span (1..5) is discarded; the inner span (2..3) is also
    // discarded but lies entirely inside the outer one — the outer
    // deletion subsumes it, and the `ex.start >= discarded[-1][-1]`
    // guard skips the inner.
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
    assert!(shrinker.remove_discarded());
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => *v,
            _ => unreachable!(),
        })
        .collect();
    // 1..5 removed; 0 and 5 survive.
    assert_eq!(values, vec![0, 5]);
}

#[test]
fn remove_discarded_returns_false_when_consider_rejects() {
    // When the test_fn returns false (uninteresting), `consider` returns
    // false and `remove_discarded` returns false — signalling that the
    // discarded data is structurally required.
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
    assert!(!shrinker.remove_discarded());
    // The original sequence is preserved.
    assert_eq!(shrinker.current_nodes.len(), 2);
}

#[test]
fn remove_discarded_iterates_when_new_target_still_has_discards() {
    // First call: closure reports the new run still has a discarded
    // region.  remove_discarded should loop once more, delete that, and
    // then exit when the closure clears the discards.
    let initial = vec![int_node(1), int_node(2), int_node(3), int_node(4)];
    let mut spans = Spans::new();
    spans.push(span(0, 4, false));
    spans.push(span(2, 4, true));

    // Switching spans table: first call leaves [0..2 discarded], second
    // call clears.
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
    assert!(shrinker.remove_discarded());
    // First call removed [2..4); second iteration found [0..2) still
    // marked discarded and removed it too — leaving nothing.
    assert_eq!(shrinker.current_nodes.len(), 0);
}

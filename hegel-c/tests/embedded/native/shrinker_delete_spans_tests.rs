//! Tests for `Shrinker::delete_spans`.
//!
//! Covers:
//! * A span whose own extent can't be deleted (trailing spanless choices
//!   misalign the remainder) is removed by the widened attempt that extends
//!   to the next span's start.
//! * The final span widens to the end of the sequence when no span follows.
//! * Skips: spans stale against the current nodes, extents already
//!   attempted, and attempts that would delete every choice.
//! * A single-choice span gets one extent: with the spanless choice before
//!   it, or else widened to the next span.
//! * A rejected deletion is retried with the choice after the enclosing span
//!   nudged: an integer one down or up, a boolean flipped, a string one
//!   character shorter; a float is left alone.
//! * The attempted-extent memory resets after an accepted improvement.

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::{BooleanChoice, FloatChoice, IntegerChoice, StringChoice};
use crate::native::core::{ChoiceNode, ChoiceValue, Span, Spans};
use crate::native::intervalsets::IntervalSet;
use crate::native::shrinker::{ShrinkRun, Shrinker};
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(i128::MAX),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
        false,
    )
}

fn span(start: usize, end: usize) -> Span {
    Span {
        start,
        end,
        label: 1,
        depth: 0,
        parent: None,
        discarded: false,
    }
}

fn values(nodes: &[ChoiceNode]) -> Vec<i128> {
    nodes
        .iter()
        .map(|n| match n.value() {
            ChoiceValue::Integer(v) => i128::try_from(&v).unwrap(),
            other => panic!("unexpected choice value {other:?}"),
        })
        .collect()
}

/// Two four-choice "rounds", each a three-choice span plus one spanless
/// trailing choice. A sequence is interesting when its length is a whole
/// number of rounds and the last round is all twos, so deleting the first
/// span's extent alone misaligns the rounds and only the widened four-choice
/// deletion is accepted.
#[test]
fn widened_deletion_removes_a_span_with_its_trailing_choices() {
    let initial = vec![
        int_node(1),
        int_node(1),
        int_node(1),
        int_node(1),
        int_node(2),
        int_node(2),
        int_node(2),
        int_node(2),
    ];
    let mut initial_spans = Spans::new();
    initial_spans.push(span(0, 3));
    initial_spans.push(span(4, 7));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let vals = values(nodes);
                let interesting = !vals.is_empty()
                    && vals.len() % 4 == 0
                    && vals[vals.len() - 4..].iter().all(|&v| v == 2);
                let mut spans = Spans::new();
                if nodes.len() >= 3 {
                    spans.push(span(0, 3));
                }
                (interesting, nodes.to_vec(), spans)
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        initial_spans,
    );

    drive_no_yield(shrinker.delete_spans()).unwrap();
    assert_eq!(values(&shrinker.current_nodes), [2, 2, 2, 2]);
}

#[test]
fn stale_narrow_and_duplicate_spans_cost_no_test_calls() {
    let initial = vec![int_node(1), int_node(1), int_node(1)];
    let mut initial_spans = Spans::new();
    initial_spans.push(span(0, 2));
    initial_spans.push(span(0, 2));
    initial_spans.push(span(1, 2));
    initial_spans.push(span(0, 7));

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_in_probe = Arc::clone(&calls);
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                calls_in_probe.fetch_add(1, Ordering::Relaxed);
                (false, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        initial_spans,
    );

    drive_no_yield(shrinker.delete_spans()).unwrap();
    // The first span costs one call for its own extent; its widened extent
    // covers the whole sequence and is skipped. The duplicate, the
    // single-choice span, and the stale span cost nothing.
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(values(&shrinker.current_nodes), [1, 1, 1]);
}

/// After an accepted deletion the attempted-extent memory resets: the same
/// numeric extent describes different choices in the shrunk sequence, so it
/// is tried again rather than skipped.
#[test]
fn attempted_extents_reset_after_an_improvement() {
    let initial = vec![
        int_node(1),
        int_node(1),
        int_node(2),
        int_node(2),
        int_node(3),
    ];
    let mut initial_spans = Spans::new();
    initial_spans.push(span(0, 2));
    initial_spans.push(span(2, 4));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let vals = values(nodes);
                let interesting = vals.last() == Some(&3);
                let mut spans = Spans::new();
                if nodes.len() >= 2 {
                    spans.push(span(0, 2));
                    spans.push(span(0, 2));
                }
                (interesting, nodes.to_vec(), spans)
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        initial_spans,
    );

    drive_no_yield(shrinker.delete_spans()).unwrap();
    // Extent (0, 2) is deleted, the memory resets, and the refreshed
    // extent (0, 2), now holding the twos, is deleted in the same sweep.
    assert_eq!(values(&shrinker.current_nodes), [3]);
}

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::boolean(BooleanChoice { p: 0.5 }, value, false)
}

fn float_node(value: f64) -> ChoiceNode {
    ChoiceNode::float(
        FloatChoice {
            min_value: 0.0,
            max_value: 10.0,
            allow_nan: false,
            allow_infinity: false,
            smallest_nonzero_magnitude: 5e-324,
        },
        value,
        false,
    )
}

fn string_node(value: &str) -> ChoiceNode {
    ChoiceNode::string(
        StringChoice {
            intervals: IntervalSet::new(vec![(b'a' as u32, b'z' as u32)])
                .unwrap()
                .into(),
            min_size: 0,
            max_size: 16,
        },
        value.chars().map(|c| c as u32).collect(),
        false,
    )
}

/// A list drawn as `[continue bit, element]* , stop bit` followed by one
/// more choice, the way a collection followed by a field is recorded.
fn list_then(elements: &[i128], follower: ChoiceNode) -> Vec<ChoiceNode> {
    let mut nodes = Vec::new();
    for &v in elements {
        nodes.push(bool_node(true));
        nodes.push(int_node(v));
    }
    nodes.push(bool_node(false));
    nodes.push(follower);
    nodes
}

/// Read `nodes` back as a list and its follower, or `None` when the choices
/// do not line up (a boolean where an element should be, and so on).
fn decode_list(nodes: &[ChoiceNode]) -> Option<(Vec<i128>, &ChoiceNode)> {
    let mut elements = Vec::new();
    let mut i = 0;
    loop {
        let ChoiceValue::Boolean(more) = nodes.get(i)?.value() else {
            return None;
        };
        i += 1;
        if !more {
            break;
        }
        let ChoiceValue::Integer(v) = nodes.get(i)?.value() else {
            return None;
        };
        elements.push(i128::try_from(&v).unwrap());
        i += 1;
    }
    (i + 1 == nodes.len()).then(|| (elements, &nodes[i]))
}

/// The spans the engine records for `list_then`: the list, one span per
/// element inside it, and the follower's own span.
fn list_spans(nodes: &[ChoiceNode]) -> Spans {
    let mut spans = Spans::new();
    let Some((elements, _)) = decode_list(nodes) else {
        return spans;
    };
    let list_end = 2 * elements.len() + 1;
    spans.push(span(0, list_end));
    for k in 0..elements.len() {
        spans.push(Span {
            parent: Some(0),
            depth: 1,
            ..span(2 * k + 1, 2 * k + 2)
        });
    }
    spans.push(span(list_end, list_end + 1));
    spans
}

fn list_shrinker(
    initial: Vec<ChoiceNode>,
    interesting: impl Fn(&[i128], &ChoiceNode) -> bool + Send + 'static,
) -> Shrinker<'static> {
    let initial_spans = list_spans(&initial);
    Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let is_interesting = decode_list(nodes)
                    .is_some_and(|(elements, follower)| interesting(&elements, follower));
                (is_interesting, nodes.to_vec(), list_spans(nodes))
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        initial_spans,
    )
}

fn integer_of(node: &ChoiceNode) -> i128 {
    match node.value() {
        ChoiceValue::Integer(v) => i128::try_from(&v).unwrap(),
        other => panic!("unexpected choice value {other:?}"),
    }
}

#[test]
fn deleting_an_element_lowers_an_index_drawn_after_the_list() {
    let mut shrinker = list_shrinker(list_then(&[0, 1], int_node(1)), |elements, index| {
        let i = usize::try_from(integer_of(index)).unwrap();
        i < elements.len() && elements[i] != 0
    });
    drive_no_yield(shrinker.delete_spans()).unwrap();
    let (elements, index) = decode_list(&shrinker.current_nodes).unwrap();
    assert_eq!((elements, integer_of(index)), (vec![1], 0));
}

#[test]
fn deleting_an_element_raises_a_count_drawn_after_the_list() {
    let mut shrinker = list_shrinker(list_then(&[0], int_node(4)), |elements, k| {
        elements.len() as i128 + integer_of(k) >= 5
    });
    drive_no_yield(shrinker.delete_spans()).unwrap();
    let (elements, k) = decode_list(&shrinker.current_nodes).unwrap();
    assert_eq!((elements, integer_of(k)), (vec![], 5));
}

#[test]
fn deleting_an_element_flips_a_parity_flag_drawn_after_the_list() {
    let mut shrinker = list_shrinker(list_then(&[0, 1], bool_node(true)), |elements, even| {
        even.value() == ChoiceValue::Boolean(elements.len() % 2 == 0)
            && elements.iter().any(|&v| v != 0)
    });
    drive_no_yield(shrinker.delete_spans()).unwrap();
    let (elements, even) = decode_list(&shrinker.current_nodes).unwrap();
    assert_eq!(
        (elements, even.value()),
        (vec![1], ChoiceValue::Boolean(false))
    );
}

#[test]
fn deleting_an_element_shortens_a_string_drawn_after_the_list() {
    let mut shrinker = list_shrinker(list_then(&[0, 1], string_node("ab")), |elements, s| {
        matches!(s.value(), ChoiceValue::String(cps) if cps.len() == elements.len())
            && elements.iter().any(|&v| v != 0)
    });
    drive_no_yield(shrinker.delete_spans()).unwrap();
    let (elements, s) = decode_list(&shrinker.current_nodes).unwrap();
    assert_eq!(
        (elements, s.value()),
        (vec![1], ChoiceValue::String(vec![b'a' as u32]))
    );
}

#[test]
fn a_float_drawn_after_the_list_is_not_nudged() {
    let initial = list_then(&[0, 1], float_node(2.0));
    let mut shrinker = list_shrinker(initial.clone(), |elements, f| {
        f.value() == ChoiceValue::Float(elements.len() as f64) && elements.iter().any(|&v| v != 0)
    });
    drive_no_yield(shrinker.delete_spans()).unwrap();
    assert_eq!(shrinker.current_nodes, initial);
}

#[test]
fn a_deletion_with_nothing_after_the_enclosing_span_is_not_retried() {
    let initial = vec![
        bool_node(true),
        int_node(0),
        bool_node(true),
        int_node(1),
        bool_node(false),
    ];
    let mut spans = Spans::new();
    spans.push(span(0, 5));
    spans.push(Span {
        parent: Some(0),
        depth: 1,
        ..span(1, 2)
    });
    spans.push(Span {
        parent: Some(0),
        depth: 1,
        ..span(3, 4)
    });
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_in_probe = Arc::clone(&calls);
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                calls_in_probe.fetch_add(1, Ordering::Relaxed);
                (false, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial.clone(),
        spans,
    );
    drive_no_yield(shrinker.delete_spans()).unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert_eq!(shrinker.current_nodes, initial);
}

#[test]
fn a_single_choice_span_at_the_start_widens_to_the_next_span() {
    let initial = vec![int_node(1), int_node(1), int_node(1)];
    let mut spans = Spans::new();
    spans.push(span(0, 1));
    spans.push(span(2, 3));
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_in_probe = Arc::clone(&calls);
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                calls_in_probe.fetch_add(1, Ordering::Relaxed);
                (false, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    drive_no_yield(shrinker.delete_spans()).unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

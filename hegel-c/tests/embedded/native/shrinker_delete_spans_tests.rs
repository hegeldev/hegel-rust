//! Tests for `Shrinker::delete_spans`.
//!
//! Covers:
//! * A span whose own extent can't be deleted (trailing spanless choices
//!   misalign the remainder) is removed by the widened attempt that extends
//!   to the next span's start.
//! * The final span widens to the end of the sequence when no span follows.
//! * Skips: spans stale against the current nodes, spans narrower than two
//!   choices, extents already attempted, and attempts that would delete
//!   every choice.
//! * The attempted-extent memory resets after an accepted improvement.

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceNode, ChoiceValue, Span, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};
use alloc::boxed::Box;
use alloc::string::ToString;
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
        label: "block".to_string(),
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

//! Tests for `Shrinker::delete_span_runs` and the deletion-first phase of
//! `Shrinker::shrink`.
//!
//! Covers:
//! * A run of deletable sibling spans goes in a handful of calls, grown by
//!   doubling and narrowed by bisection, where one call per span would
//!   cost as many calls as spans.
//! * A run stops at its parent's boundary: the siblings under one parent
//!   never join those under another, and the last sibling's extent ends
//!   at its own end.
//! * Stale, empty and whole-sequence extents cost no test calls.
//! * `shrink` deletes to a fixed point before its other passes, and a halt
//!   during that phase ends the shrink where it stands.

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceNode, ChoiceValue, Span, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

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

fn span(start: usize, end: usize, depth: u32, parent: Option<usize>) -> Span {
    Span {
        start,
        end,
        label: 1,
        depth,
        parent,
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

/// Spans of two choices each over `nodes`, all siblings at depth 1 under a
/// root span at depth 0.
fn step_spans(nodes: usize) -> Spans {
    let mut spans = Spans::new();
    spans.push(span(0, nodes, 0, None));
    for i in (0..nodes).step_by(2) {
        spans.push(span(i, i + 2, 1, Some(0)));
    }
    spans
}

/// Forty two-choice steps, interesting while a step ending in seven
/// remains. The thirty-nine steps before it go as one run: doubling to
/// twenty, failing at forty (the whole sequence) and bisecting to
/// thirty-nine.
#[test]
fn a_run_of_sibling_steps_is_deleted_in_a_handful_of_calls() {
    let mut initial = Vec::new();
    for _ in 0..39 {
        initial.push(int_node(1));
        initial.push(int_node(0));
    }
    initial.push(int_node(1));
    initial.push(int_node(7));
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_in_probe = Arc::clone(&calls);
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                calls_in_probe.fetch_add(1, Ordering::Relaxed);
                let vals = values(nodes);
                let interesting = vals.len() % 2 == 0 && vals.chunks(2).any(|s| s[1] == 7);
                (interesting, nodes.to_vec(), step_spans(nodes.len()))
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        step_spans(80),
    );
    drive_no_yield(shrinker.delete_span_runs()).unwrap();
    assert_eq!(values(&shrinker.current_nodes), [1, 7]);
    assert!(
        calls.load(Ordering::Relaxed) < 16,
        "{} calls",
        calls.load(Ordering::Relaxed)
    );
}

/// Two parents at depth 1, each with children at depth 2, over a sequence
/// that is interesting while it ends in nine and is not two choices long.
/// Neither parent goes whole, so the runs are tried among the children:
/// the first parent's two children go one at a time, since deleting both
/// leaves two choices, and the probe past them is refused without a call,
/// as the next span belongs to the other parent. The survivors are each
/// tried on their own, the last to its own end.
#[test]
fn a_run_stops_at_its_parents_boundary() {
    let initial = vec![int_node(1), int_node(2), int_node(5), int_node(9)];
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_in_probe = Arc::clone(&calls);
    let layout = |len: usize| {
        let mut spans = Spans::new();
        spans.push(span(0, len, 0, None));
        if len > 2 {
            spans.push(span(0, len - 2, 1, Some(0)));
            for i in 0..len - 2 {
                spans.push(span(i, i + 1, 2, Some(1)));
            }
        }
        let parent = spans.len();
        spans.push(span(len - 2, len, 1, Some(0)));
        spans.push(span(len - 2, len - 1, 2, Some(parent)));
        spans.push(span(len - 1, len, 2, Some(parent)));
        spans
    };
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                calls_in_probe.fetch_add(1, Ordering::Relaxed);
                let vals = values(nodes);
                let interesting = vals.last() == Some(&9) && vals.len() != 2;
                (interesting, nodes.to_vec(), layout(nodes.len()))
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        layout(4),
    );
    drive_no_yield(shrinker.delete_span_runs()).unwrap();
    assert_eq!(values(&shrinker.current_nodes), [2, 5, 9]);
    assert_eq!(calls.load(Ordering::Relaxed), 8);
}

#[test]
fn stale_empty_and_whole_sequence_extents_cost_no_test_calls() {
    let initial = vec![int_node(1), int_node(1), int_node(1)];
    let mut initial_spans = Spans::new();
    initial_spans.push(span(0, 3, 0, None));
    initial_spans.push(span(1, 1, 1, Some(0)));
    initial_spans.push(span(1, 7, 1, Some(0)));
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
    drive_no_yield(shrinker.delete_span_runs()).unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert_eq!(values(&shrinker.current_nodes), [1, 1, 1]);
}

/// The deletion-first phase alone takes six steps down to the one that
/// matters, and a budget of one call ends the shrink there.
#[test]
fn shrink_deletes_first_and_a_halt_in_that_phase_ends_it() {
    for max_calls in [None, Some(1)] {
        let mut initial = Vec::new();
        for _ in 0..5 {
            initial.push(int_node(1));
            initial.push(int_node(0));
        }
        initial.push(int_node(1));
        initial.push(int_node(7));
        let mut shrinker = Shrinker::with_probe(
            Box::new(move |run: ShrinkRun<'_>| match run {
                ShrinkRun::Full(nodes) => {
                    let vals = values(nodes);
                    let interesting = vals.len() % 2 == 0 && vals.chunks(2).any(|s| s[1] == 7);
                    (interesting, nodes.to_vec(), step_spans(nodes.len()))
                }
                ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
            }),
            initial,
            step_spans(12),
        );
        shrinker.max_calls = max_calls;
        drive_no_yield(shrinker.shrink()).unwrap();
        let expected: &[i128] = if max_calls.is_none() {
            &[0, 7]
        } else {
            &[1, 0, 1, 0, 1, 0, 1, 0, 1, 7]
        };
        assert_eq!(
            values(&shrinker.current_nodes),
            expected,
            "max_calls {max_calls:?}"
        );
    }
}

#[test]
fn shrink_reduces_coarsely_only_after_deleting() {
    let initial = vec![
        int_node(1),
        int_node(2),
        int_node(1),
        int_node(3),
        int_node(1),
        int_node(2),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let vals = values(nodes);
                let interesting = vals.len() % 2 == 0 && vals.chunks(2).any(|s| s[1] == 3);
                (interesting, nodes.to_vec(), step_spans(nodes.len()))
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        step_spans(6),
    );
    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&lines);
    shrinker.set_debug(move |line| sink.lock().unwrap().push(line.to_string()));
    drive_no_yield(shrinker.shrink()).unwrap();
    assert_eq!(values(&shrinker.current_nodes), [0, 3]);
    let lines = lines.lock().unwrap();
    let deletion = lines
        .iter()
        .position(|l| l.starts_with("Deletion first: "))
        .unwrap();
    let coarse = lines
        .iter()
        .position(|l| l.starts_with("Coarse reduction: "))
        .unwrap();
    assert!(deletion < coarse, "{lines:?}");
    assert!(lines[coarse].ends_with(" 2 choices left"), "{lines:?}");
}

#[test]
fn a_pair_of_steps_is_deleted_when_neither_can_go_alone() {
    let initial = vec![
        int_node(1),
        int_node(2),
        int_node(1),
        int_node(2),
        int_node(7),
    ];
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                counter.fetch_add(1, Ordering::Relaxed);
                let vals = values(nodes);
                let ones = vals.iter().filter(|&&v| v == 1).count();
                let twos = vals.iter().filter(|&&v| v == 2).count();
                let interesting = vals.last() == Some(&7) && ones == twos;
                (interesting, nodes.to_vec(), step_spans(nodes.len()))
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        step_spans(5),
    );
    drive_no_yield(shrinker.delete_span_runs()).unwrap();
    assert_eq!(values(&shrinker.current_nodes), [7]);
    assert!(
        calls.load(Ordering::Relaxed) <= 8,
        "{}",
        calls.load(Ordering::Relaxed)
    );
}

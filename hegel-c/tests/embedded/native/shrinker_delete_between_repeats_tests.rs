//! Unit tests for `Shrinker::delete_between_repeats`.

use std::sync::Arc;

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::{BooleanChoice, FloatChoice, IntegerChoice};
use crate::native::core::{
    BytesChoice, ChoiceNode, ChoiceValueRef, RealizedStream, Spans, StringChoice,
};
use crate::native::intervalsets::IntervalSet;
use crate::native::shrinker::{ShrinkRun, Shrinker};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(100),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
        false,
    )
}

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::boolean(BooleanChoice { p: 0.5 }, value, false)
}

fn clone_node(children: Vec<ChoiceNode>) -> ChoiceNode {
    ChoiceNode::clone_stream(
        Arc::new(RealizedStream::new(children, Vec::new(), Vec::new())),
        false,
    )
}

fn int_value(node: &ChoiceNode) -> Option<i128> {
    match node.data.value_ref() {
        ChoiceValueRef::Integer(v) => i128::try_from(v.clone()).ok(),
        _ => None,
    }
}

fn bool_value(node: &ChoiceNode) -> Option<bool> {
    match node.data.value_ref() {
        ChoiceValueRef::Boolean(v) => Some(v),
        _ => None,
    }
}

/// Parse `nodes` as the choice-sequence shape of a collection of
/// fixed-width elements: repeat { boolean true, `width` integers },
/// terminated by a boolean false. Returns the elements, or None if the
/// nodes don't have that shape.
fn parse_chunks(nodes: &[ChoiceNode], width: usize) -> Option<Vec<Vec<i128>>> {
    let mut chunks = Vec::new();
    let mut rest = nodes;
    loop {
        let (gate, tail) = rest.split_first()?;
        if !bool_value(gate)? {
            return tail.is_empty().then_some(chunks);
        }
        if tail.len() < width {
            return None;
        }
        let (chunk, tail) = tail.split_at(width);
        chunks.push(chunk.iter().map(int_value).collect::<Option<_>>()?);
        rest = tail;
    }
}

fn chunk_shrinker(initial: Vec<ChoiceNode>, width: usize) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = parse_chunks(nodes, width)
                    .is_some_and(|chunks| chunks.iter().any(|c| c[width - 1] == 7));
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    )
}

/// Each element costs 11 choices (a gate plus ten digits), wider than any
/// window `delete_chunks` proposes, but the minimized elements repeat each
/// other, so gate-to-gate deletions remove them whole.
#[test]
fn deletes_wide_elements_between_repeated_grams() {
    let width = 10;
    let mut initial = Vec::new();
    for chunk in 0..4 {
        initial.push(bool_node(true));
        for i in 0..width {
            let last_of_failing_chunk = chunk == 3 && i == width - 1;
            initial.push(int_node(if last_of_failing_chunk { 7 } else { 0 }));
        }
    }
    initial.push(bool_node(false));

    let mut shrinker = chunk_shrinker(initial, width);
    drive_no_yield(shrinker.delete_between_repeats()).unwrap();

    let mut expected = vec![0i128; width];
    expected[width - 1] = 7;
    assert_eq!(
        parse_chunks(&shrinker.current_nodes, width),
        Some(vec![expected])
    );
}

/// A run of one repeated value only has occurrences at distance 1, which
/// the pass skips, so it proposes nothing. The three-node sequence also
/// exercises skipping gram sizes longer than the sequence.
#[test]
fn adjacent_repeats_propose_nothing() {
    let initial = vec![int_node(0), int_node(0), int_node(0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial.clone(),
        Spans::new(),
    );
    drive_no_yield(shrinker.delete_between_repeats()).unwrap();
    assert_eq!(shrinker.calls, 0);
    assert_eq!(shrinker.current_nodes, initial);
}

/// Equal values under different constraints share a fingerprint but are
/// not repeats, so they propose nothing.
#[test]
fn equal_values_with_different_constraints_are_not_repeats() {
    let narrow = ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(10),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(5),
        false,
    );
    let initial = vec![int_node(5), bool_node(true), narrow, bool_node(false)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial.clone(),
        Spans::new(),
    );
    drive_no_yield(shrinker.delete_between_repeats()).unwrap();
    assert_eq!(shrinker.calls, 0);
    assert_eq!(shrinker.current_nodes, initial);
}

/// Clone nodes are opaque values to this pass: two clones with different
/// payloads still count as repeats, so the region between them is proposed.
#[test]
fn clones_with_different_payloads_are_repeats() {
    let initial = vec![
        clone_node(vec![int_node(1)]),
        int_node(3),
        clone_node(vec![int_node(2), int_node(9)]),
        int_node(42),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = nodes.last().and_then(int_value) == Some(42);
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.delete_between_repeats()).unwrap();
    assert_eq!(shrinker.current_nodes.len(), 2);
    let survivor = shrinker.current_nodes[0].data.as_clone().unwrap();
    assert_eq!(survivor.nodes().len(), 2);
}

/// Float, bytes, and string values all participate in fingerprints: a
/// repeated heterogeneous block is deleted like any other repeat.
#[test]
fn fingerprints_cover_all_value_kinds() {
    let float = || {
        ChoiceNode::float(
            FloatChoice {
                min_value: 0.0,
                max_value: 100.0,
                allow_nan: false,
                allow_infinity: false,
                smallest_nonzero_magnitude: 5e-324,
            },
            2.5,
            false,
        )
    };
    let bytes = || {
        ChoiceNode::bytes(
            BytesChoice {
                min_size: 0,
                max_size: 8,
            },
            vec![1, 2],
            false,
        )
    };
    let string = || {
        ChoiceNode::string(
            StringChoice {
                intervals: IntervalSet::new(vec![(97, 122)]).unwrap().into(),
                min_size: 0,
                max_size: 8,
            },
            vec![97, 98],
            false,
        )
    };
    let initial = vec![
        float(),
        bytes(),
        string(),
        float(),
        bytes(),
        string(),
        int_node(42),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = nodes.last().and_then(int_value) == Some(42);
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.delete_between_repeats()).unwrap();
    assert_eq!(shrinker.current_nodes.len(), 1);
}

/// An accepted attempt can come back much shorter than the candidate (the
/// test body exits early), leaving later proposals out of range. They are
/// skipped rather than sliced out of bounds.
#[test]
fn stale_proposals_beyond_the_new_length_are_skipped() {
    let initial = vec![
        int_node(9),
        int_node(1),
        int_node(2),
        int_node(9),
        int_node(1),
        int_node(2),
        int_node(9),
        int_node(42),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = nodes.last().and_then(int_value) == Some(42);
                let actual = if interesting && nodes.len() < 8 {
                    vec![int_node(42)]
                } else {
                    nodes.to_vec()
                };
                (interesting, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.delete_between_repeats()).unwrap();
    assert_eq!(shrinker.calls, 1);
    assert_eq!(shrinker.current_nodes.len(), 1);
}

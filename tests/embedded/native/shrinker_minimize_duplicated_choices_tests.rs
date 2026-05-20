//! Unit tests for the generalised `shrink_duplicates` /
//! `minimize_duplicated_choices` (Step 7).
//!
//! Hypothesis reference: `shrinker.py:1379-1406`.

use crate::native::core::choices::{BooleanChoice, BytesChoice, FloatChoice, StringChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::intervalsets::IntervalSet;
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(value),
        was_forced: false,
    }
}

fn float_node(value: f64) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Float(FloatChoice {
            min_value: f64::NEG_INFINITY,
            max_value: f64::INFINITY,
            allow_nan: false,
            allow_infinity: false,
        }),
        value: ChoiceValue::Float(value),
        was_forced: false,
    }
}

fn bytes_node(value: Vec<u8>) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Bytes(BytesChoice {
            min_size: 0,
            max_size: 16,
        }),
        value: ChoiceValue::Bytes(value),
        was_forced: false,
    }
}

fn string_node(value: Vec<u32>) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::String(StringChoice {
            intervals: IntervalSet::new(vec![(0, 0x10FFFF)]),
            min_size: 0,
            max_size: 16,
        }),
        value: ChoiceValue::String(value),
        was_forced: false,
    }
}

fn accepting_shrinker(initial: Vec<ChoiceNode>) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    )
}

#[test]
fn shrink_duplicates_collapses_paired_booleans_to_false() {
    let mut shrinker = accepting_shrinker(vec![bool_node(true), bool_node(true)]);
    shrinker.shrink_duplicates();
    for n in &shrinker.current_nodes {
        match n.value {
            ChoiceValue::Boolean(b) => assert!(!b),
            _ => unreachable!(),
        }
    }
}

#[test]
fn shrink_duplicates_collapses_paired_floats_to_zero() {
    let mut shrinker = accepting_shrinker(vec![float_node(3.5), float_node(3.5)]);
    shrinker.shrink_duplicates();
    for n in &shrinker.current_nodes {
        match n.value {
            ChoiceValue::Float(v) => assert_eq!(v, 0.0),
            _ => unreachable!(),
        }
    }
}

#[test]
fn shrink_duplicates_collapses_paired_bytes_to_empty() {
    let mut shrinker =
        accepting_shrinker(vec![bytes_node(vec![1, 2, 3]), bytes_node(vec![1, 2, 3])]);
    shrinker.shrink_duplicates();
    for n in &shrinker.current_nodes {
        match &n.value {
            ChoiceValue::Bytes(b) => assert!(b.is_empty()),
            _ => unreachable!(),
        }
    }
}

#[test]
fn shrink_duplicates_collapses_paired_strings_to_empty() {
    let mut shrinker = accepting_shrinker(vec![
        string_node(vec![b'a' as u32, b'b' as u32]),
        string_node(vec![b'a' as u32, b'b' as u32]),
    ]);
    shrinker.shrink_duplicates();
    for n in &shrinker.current_nodes {
        match &n.value {
            ChoiceValue::String(s) => assert!(s.is_empty()),
            _ => unreachable!(),
        }
    }
}

#[test]
fn shrink_duplicates_leaves_solo_nodes_alone() {
    // Single non-duplicate of each kind — the generalised pass shouldn't
    // change them.  Predicate accepts everything; only the simplest-step
    // could fire, but each group has only one member.
    let mut shrinker =
        accepting_shrinker(vec![bool_node(true), float_node(3.0), bytes_node(vec![5])]);
    shrinker.shrink_duplicates();
    match shrinker.current_nodes[0].value {
        ChoiceValue::Boolean(b) => assert!(b),
        _ => unreachable!(),
    }
    match shrinker.current_nodes[1].value {
        ChoiceValue::Float(v) => assert_eq!(v, 3.0),
        _ => unreachable!(),
    }
    match &shrinker.current_nodes[2].value {
        ChoiceValue::Bytes(b) => assert_eq!(b, &vec![5]),
        _ => unreachable!(),
    }
}

#[test]
fn shrink_duplicates_keeps_distinct_values_separate() {
    // Three booleans, only two of them duplicates.  The duplicates
    // should be lowered together; the third value should be left alone.
    let mut shrinker = accepting_shrinker(vec![bool_node(true), bool_node(false), bool_node(true)]);
    shrinker.shrink_duplicates();
    // After shrink: the two trues went to false, the original false
    // stayed.  Result: [false, false, false].
    for n in &shrinker.current_nodes {
        match n.value {
            ChoiceValue::Boolean(b) => assert!(!b),
            _ => unreachable!(),
        }
    }
}

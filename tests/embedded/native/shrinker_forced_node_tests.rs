//! Regression tests asserting that every shrink pass must skip
//! `was_forced=true` nodes. We gate at the top-level node loop of
//! each pass.

use crate::native::core::choices::{BooleanChoice, BytesChoice, FloatChoice, IntegerChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128, was_forced: bool) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: i128::MIN + 1,
            max_value: i128::MAX,
            shrink_towards: 0,
        }),
        value: ChoiceValue::Integer(value),
        was_forced,
    }
}

fn float_node(value: f64, was_forced: bool) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Float(FloatChoice {
            min_value: f64::NEG_INFINITY,
            max_value: f64::INFINITY,
            allow_nan: false,
            allow_infinity: false,
        }),
        value: ChoiceValue::Float(value),
        was_forced,
    }
}

fn bool_node(value: bool, was_forced: bool) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(value),
        was_forced,
    }
}

fn bytes_node(value: Vec<u8>, was_forced: bool) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Bytes(BytesChoice {
            min_size: 0,
            max_size: 16,
        }),
        value: ChoiceValue::Bytes(value),
        was_forced,
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

fn assert_integer_at(shrinker: &Shrinker<'_>, idx: usize, expected: i128) {
    match shrinker.current_nodes[idx].value {
        ChoiceValue::Integer(v) => assert_eq!(v, expected, "node {idx}"),
        _ => unreachable!(),
    }
}

#[test]
fn swap_integer_sign_skips_forced_node() {
    let mut shrinker = accepting_shrinker(vec![int_node(-10, true), int_node(-20, false)]);
    shrinker.swap_integer_sign();
    assert_integer_at(&shrinker, 0, -10);
}

#[test]
fn binary_search_integer_towards_zero_skips_forced_node() {
    let mut shrinker = accepting_shrinker(vec![int_node(1000, true), int_node(500, false)]);
    shrinker.binary_search_integer_towards_zero();
    assert_integer_at(&shrinker, 0, 1000);
}

#[test]
fn shrink_duplicates_skips_forced_member_of_group() {
    let mut shrinker = accepting_shrinker(vec![
        int_node(7, true),
        int_node(7, false),
        int_node(7, false),
    ]);
    shrinker.shrink_duplicates();
    assert_integer_at(&shrinker, 0, 7);
}

#[test]
fn redistribute_integers_skips_forced_node() {
    let mut shrinker = accepting_shrinker(vec![int_node(50, true), int_node(50, false)]);
    shrinker.redistribute_integers();
    assert_integer_at(&shrinker, 0, 50);
}

#[test]
fn shrink_bytes_skips_forced_node() {
    let mut shrinker = accepting_shrinker(vec![
        bytes_node(vec![9, 9, 9], true),
        bytes_node(vec![1, 2, 3], false),
    ]);
    shrinker.shrink_bytes();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::Bytes(b) => assert_eq!(b, &vec![9, 9, 9]),
        _ => unreachable!(),
    }
}

#[test]
fn shrink_floats_skips_forced_node() {
    let mut shrinker = accepting_shrinker(vec![float_node(123.5, true), float_node(7.5, false)]);
    shrinker.shrink_floats();
    match shrinker.current_nodes[0].value {
        ChoiceValue::Float(v) => assert_eq!(v, 123.5),
        _ => unreachable!(),
    }
}

#[test]
fn try_shortening_via_increment_skips_forced_node() {
    let mut shrinker = accepting_shrinker(vec![
        int_node(5, true),
        int_node(3, false),
        int_node(2, false),
    ]);
    shrinker.try_shortening_via_increment();
    assert_integer_at(&shrinker, 0, 5);
}

#[test]
fn mutate_and_shrink_skips_forced_node() {
    let mut shrinker = accepting_shrinker(vec![int_node(99, true), bool_node(true, false)]);
    shrinker.mutate_and_shrink();
    assert_integer_at(&shrinker, 0, 99);
}

#[test]
fn redistribute_numeric_pairs_skips_forced_integer() {
    // Starting from (15, 10) with the second node forced and predicate
    // `n1 + n2 > 20`, the redistribute_numeric_pairs pass should not
    // modify the forced node — so the shrink target stays at (15, 10)
    // and doesn't collapse to (11, 10).
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let sum: i128 = nodes
                    .iter()
                    .filter_map(|n| match n.value {
                        ChoiceValue::Integer(v) => Some(v),
                        _ => None,
                    })
                    .sum();
                (sum > 20, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![
            int_node(15, false),
            ChoiceNode {
                kind: ChoiceKind::Integer(IntegerChoice {
                    min_value: 0,
                    max_value: 100,
                    shrink_towards: 0,
                }),
                value: ChoiceValue::Integer(10),
                was_forced: true,
            },
        ],
        Spans::new(),
    );
    shrinker.redistribute_numeric_pairs();
    assert_integer_at(&shrinker, 0, 15);
    assert_integer_at(&shrinker, 1, 10);
}

#[test]
fn normalize_unicode_chars_skips_forced_node() {
    use crate::native::core::choices::StringChoice;
    use crate::native::intervalsets::IntervalSet;
    let forced_str = ChoiceNode {
        kind: ChoiceKind::String(StringChoice {
            intervals: IntervalSet::new(vec![(0, 0x10FFFF)]),
            min_size: 0,
            max_size: 16,
        }),
        // an accented character that would normally be normalised
        value: ChoiceValue::String(vec![0xE9]),
        was_forced: true,
    };
    let other = ChoiceNode {
        kind: ChoiceKind::String(StringChoice {
            intervals: IntervalSet::new(vec![(0, 0x10FFFF)]),
            min_size: 0,
            max_size: 16,
        }),
        value: ChoiceValue::String(vec![0xE9]),
        was_forced: false,
    };
    let mut shrinker = accepting_shrinker(vec![forced_str, other]);
    shrinker.normalize_unicode_chars();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::String(s) => assert_eq!(s, &vec![0xE9]),
        _ => unreachable!(),
    }
}

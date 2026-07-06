//! Regression tests asserting that every shrink pass must skip
//! `was_forced=true` nodes. We gate at the top-level node loop of
//! each pass.

use crate::native::bignum::BigInt;
use crate::native::core::choices::{BooleanChoice, BytesChoice, FloatChoice, IntegerChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128, was_forced: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(i128::MIN + 1),
            max_value: BigInt::from(i128::MAX),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        was_forced,
    )
}

fn float_node(value: f64, was_forced: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Float(FloatChoice {
            min_value: f64::NEG_INFINITY,
            max_value: f64::INFINITY,
            allow_nan: false,
            allow_infinity: false,
            smallest_nonzero_magnitude: 5e-324,
        }),
        ChoiceValue::Float(value),
        was_forced,
    )
}

fn bool_node(value: bool, was_forced: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(value),
        was_forced,
    )
}

fn bytes_node(value: Vec<u8>, was_forced: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Bytes(BytesChoice {
            min_size: 0,
            max_size: 16,
        }),
        ChoiceValue::Bytes(value),
        was_forced,
    )
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
    match &shrinker.current_nodes[idx].value {
        ChoiceValue::Integer(v) => {
            assert_eq!(i128::try_from(v.clone()).unwrap(), expected, "node {idx}")
        }
        _ => unreachable!(),
    }
}

#[test]
fn swap_integer_sign_skips_forced_node() {
    let mut shrinker = accepting_shrinker(vec![int_node(-10, true), int_node(-20, false)]);
    shrinker.swap_integer_sign().unwrap();
    assert_integer_at(&shrinker, 0, -10);
}

#[test]
fn binary_search_integer_towards_zero_skips_forced_node() {
    let mut shrinker = accepting_shrinker(vec![int_node(1000, true), int_node(500, false)]);
    shrinker.binary_search_integer_towards_zero().unwrap();
    assert_integer_at(&shrinker, 0, 1000);
}

#[test]
fn shrink_duplicates_skips_forced_member_of_group() {
    let mut shrinker = accepting_shrinker(vec![
        int_node(7, true),
        int_node(7, false),
        int_node(7, false),
    ]);
    shrinker.shrink_duplicates().unwrap();
    assert_integer_at(&shrinker, 0, 7);
}

#[test]
fn redistribute_integers_skips_forced_node() {
    let mut shrinker = accepting_shrinker(vec![int_node(50, true), int_node(50, false)]);
    shrinker.redistribute_integers().unwrap();
    assert_integer_at(&shrinker, 0, 50);
}

#[test]
fn shrink_bytes_skips_forced_node() {
    let mut shrinker = accepting_shrinker(vec![
        bytes_node(vec![9, 9, 9], true),
        bytes_node(vec![1, 2, 3], false),
    ]);
    shrinker.shrink_bytes().unwrap();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::Bytes(b) => assert_eq!(b, &vec![9, 9, 9]),
        _ => unreachable!(),
    }
}

#[test]
fn shrink_floats_skips_forced_node() {
    let mut shrinker = accepting_shrinker(vec![float_node(123.5, true), float_node(7.5, false)]);
    shrinker.shrink_floats().unwrap();
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
    shrinker.try_shortening_via_increment().unwrap();
    assert_integer_at(&shrinker, 0, 5);
}

#[test]
fn mutate_and_shrink_skips_forced_node() {
    let mut shrinker = accepting_shrinker(vec![int_node(99, true), bool_node(true, false)]);
    shrinker.mutate_and_shrink().unwrap();
    assert_integer_at(&shrinker, 0, 99);
}

#[test]
fn redistribute_numeric_pairs_skips_forced_integer() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let sum: i128 = nodes
                    .iter()
                    .filter_map(|n| match &n.value {
                        ChoiceValue::Integer(v) => Some(i128::try_from(v.clone()).unwrap()),
                        _ => None,
                    })
                    .sum();
                (sum > 20, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![
            int_node(15, false),
            ChoiceNode::new(
                ChoiceKind::Integer(IntegerChoice {
                    min_value: BigInt::from(0),
                    max_value: BigInt::from(100),
                    shrink_towards: BigInt::from(0),
                }),
                ChoiceValue::Integer(BigInt::from(10)),
                true,
            ),
        ],
        Spans::new(),
    );
    shrinker.redistribute_numeric_pairs().unwrap();
    assert_integer_at(&shrinker, 0, 15);
    assert_integer_at(&shrinker, 1, 10);
}

#[test]
fn normalize_unicode_chars_skips_forced_node() {
    use crate::native::core::choices::StringChoice;
    use crate::native::intervalsets::IntervalSet;
    let forced_str = ChoiceNode::new(
        ChoiceKind::String(StringChoice {
            intervals: IntervalSet::new(vec![(0, 0x10FFFF)]).into(),
            min_size: 0,
            max_size: 16,
        }),
        ChoiceValue::String(vec![0xE9]),
        true,
    );
    let other = ChoiceNode::new(
        ChoiceKind::String(StringChoice {
            intervals: IntervalSet::new(vec![(0, 0x10FFFF)]).into(),
            min_size: 0,
            max_size: 16,
        }),
        ChoiceValue::String(vec![0xE9]),
        false,
    );
    let mut shrinker = accepting_shrinker(vec![forced_str, other]);
    shrinker.normalize_unicode_chars().unwrap();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::String(s) => assert_eq!(s, &vec![0xE9]),
        _ => unreachable!(),
    }
}

/// Forced-guard fix: the forced-value guard in `consider` must apply ONLY to
/// same-length candidates. A deletion shifts every node after the cut, so
/// comparing `candidate[i]` to `current[i]` at a forced index past the cut is
/// meaningless and used to spuriously reject the deletion.
///
/// Here `current = [int(1), forced int(9), int(2)]`. Deleting index 0 yields
/// the shorter `[forced int(9), int(2)]`: the forced node shifts from index 1
/// to index 0, and the node now under the old forced index 1 (value 2) differs
/// from 9. The pre-fix guard rejected this without running; the fix must run it
/// and accept it (it is interesting and shortlex-smaller).
#[test]
fn consider_accepts_length_reducing_candidate_past_forced_node() {
    let mut shrinker = accepting_shrinker(vec![
        int_node(1, false),
        int_node(9, true),
        int_node(2, false),
    ]);
    let interesting = shrinker
        .consider(&[int_node(9, true), int_node(2, false)])
        .unwrap();
    assert!(
        interesting,
        "length-reducing candidate must not be pre-rejected by the forced guard"
    );
    assert_eq!(
        shrinker.current_nodes.len(),
        2,
        "the deletion should have been accepted as the new shrink target"
    );
}

/// Free-reject (Hypothesis `cached_test_function` port): a candidate that is
/// shortlex >= the current target can never improve it, so `consider` must
/// reject it WITHOUT running the test closure.
#[test]
fn consider_free_rejects_shortlex_larger_candidate_without_running() {
    use std::cell::Cell;
    use std::rc::Rc;

    let ran = Rc::new(Cell::new(false));
    let inner = Rc::clone(&ran);
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                inner.set(true);
                (true, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5, false)],
        Spans::new(),
    );
    let interesting = shrinker.consider(&[int_node(7, false)]).unwrap();
    assert!(!interesting, "shortlex-larger candidate must be rejected");
    assert!(
        !ran.get(),
        "shortlex-larger candidate must be free-rejected without running the test"
    );
}

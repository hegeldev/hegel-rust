//! Unit tests for the integer shrink passes' `shrink_towards` anchoring.
//!
//! Hypothesis shrinks the *distance* from `shrink_towards`, probing both
//! `shrink_towards + n` and `shrink_towards - n` (shrinker.py's
//! `minimize_individual_nodes`), so values on either side of a non-zero
//! target converge onto it. The legacy pass was anchored at zero and never
//! moved values lying between zero and the target.

use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node_st(value: i128, min: i128, max: i128, shrink_towards: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(min),
            max_value: BigInt::from(max),
            shrink_towards: BigInt::from(shrink_towards),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn int_value(node: &ChoiceNode) -> i128 {
    match &node.value {
        ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
        _ => unreachable!(),
    }
}

fn accept_all(initial: Vec<ChoiceNode>) -> Shrinker<'static> {
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
fn integer_shrink_walks_up_to_shrink_towards_from_below() {
    let mut shrinker = accept_all(vec![int_node_st(3, 0, 1000, 100)]);
    shrinker.binary_search_integer_towards_zero().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 100);
}

#[test]
fn integer_shrink_descends_to_shrink_towards_from_above() {
    let mut shrinker = accept_all(vec![int_node_st(977, 0, 1000, 100)]);
    shrinker.binary_search_integer_towards_zero().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 100);
}

#[test]
fn integer_shrink_probes_both_sides_of_target_for_nonmonotonic_predicates() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = int_value(&nodes[0]) % 2 != 0;
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node_st(3, 0, 1000, 100)],
        Spans::new(),
    );
    shrinker.binary_search_integer_towards_zero().unwrap();
    let v = int_value(&shrinker.current_nodes[0]);
    assert_eq!(
        (v - 100).unsigned_abs(),
        1,
        "expected distance 1 from target, got {v}"
    );
}

#[test]
fn integer_shrink_masks_high_bits_of_distance() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = int_value(&nodes[0]) % 256 == 0x77;
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node_st(0x1000077, 0, i64::MAX as i128, 0)],
        Spans::new(),
    );
    shrinker.binary_search_integer_towards_zero().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 0x77);
}

#[test]
fn redistribute_integers_moves_values_toward_shrink_towards() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let sum: i128 = nodes.iter().map(int_value).sum();
                (sum == 200, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node_st(3, 0, 1000, 100), int_node_st(197, 0, 1000, 100)],
        Spans::new(),
    );
    shrinker.redistribute_integers().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 100);
    assert_eq!(int_value(&shrinker.current_nodes[1]), 100);
}

#[test]
fn lower_and_bump_accepts_relative_bump() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let a = int_value(&nodes[0]);
                let b = int_value(&nodes[1]);
                let interesting = (a, b) == (5, 0) || (a, b) == (4, 1);
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node_st(5, 0, 10, 0), int_node_st(0, 0, 10, 0)],
        Spans::new(),
    );
    shrinker.lower_and_bump().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 4);
    assert_eq!(int_value(&shrinker.current_nodes[1]), 1);
}

//! Unit tests for the integer shrink passes' `shrink_towards` anchoring.
//!
//! Hypothesis shrinks the *distance* from `shrink_towards`, probing both
//! `shrink_towards + n` and `shrink_towards - n` (shrinker.py's
//! `minimize_individual_nodes`), so values on either side of a non-zero
//! target converge onto it. The legacy pass was anchored at zero and never
//! moved values lying between zero and the target.

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

fn int_node_st(value: i128, min: i128, max: i128, shrink_towards: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(min),
            max_value: BigInt::from(max),
            shrink_towards: BigInt::from(shrink_towards),
        },
        BigInt::from(value),
        false,
    )
}

fn int_value(node: &ChoiceNode) -> i128 {
    match &node.value() {
        ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
        _ => unreachable!(),
    }
}

fn accept_all(initial: Vec<ChoiceNode>) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
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
    drive_no_yield(shrinker.binary_search_integer_towards_zero()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 100);
}

#[test]
fn integer_shrink_descends_to_shrink_towards_from_above() {
    let mut shrinker = accept_all(vec![int_node_st(977, 0, 1000, 100)]);
    drive_no_yield(shrinker.binary_search_integer_towards_zero()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 100);
}

#[test]
fn integer_shrink_probes_both_sides_of_target_for_nonmonotonic_predicates() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = int_value(&nodes[0]) % 2 != 0;
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node_st(3, 0, 1000, 100)],
        Spans::new(),
    );
    drive_no_yield(shrinker.binary_search_integer_towards_zero()).unwrap();
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
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = int_value(&nodes[0]) % 256 == 0x77;
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node_st(0x1000077, 0, i64::MAX as i128, 0)],
        Spans::new(),
    );
    drive_no_yield(shrinker.binary_search_integer_towards_zero()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 0x77);
}

#[test]
fn redistribute_integers_moves_values_toward_shrink_towards() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let sum: i128 = nodes.iter().map(int_value).sum();
                (sum == 200, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node_st(3, 0, 1000, 100), int_node_st(197, 0, 1000, 100)],
        Spans::new(),
    );
    drive_no_yield(shrinker.redistribute_integers()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 100);
    assert_eq!(int_value(&shrinker.current_nodes[1]), 100);
}

#[test]
fn lower_and_bump_survives_an_adopted_candidate_shorter_than_the_node_index() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let values: Vec<i128> = nodes.iter().map(int_value).collect();
                if values == [0, 0] {
                    (true, Vec::new(), Spans::new())
                } else {
                    (false, nodes.to_vec(), Spans::new())
                }
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node_st(2, 0, 100, 0), int_node_st(0, 0, 100, 0)],
        Spans::new(),
    );
    drive_no_yield(shrinker.lower_and_bump()).unwrap();
    assert!(shrinker.current_nodes.is_empty());
}

#[test]
fn lower_and_bump_accepts_relative_bump() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
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
    drive_no_yield(shrinker.lower_and_bump()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 4);
    assert_eq!(int_value(&shrinker.current_nodes[1]), 1);
}

fn bool_value(node: &ChoiceNode) -> bool {
    match &node.value() {
        ChoiceValue::Boolean(b) => *b,
        _ => unreachable!(),
    }
}

/// A `one_of` whose pair branch `(false, true)` is interesting has a shorter
/// interesting branch, a lone `true`, but that branch needs the *second*
/// value of the pair: bumping the branch index and zeroing what follows gives
/// a lone `false`, so the pass must also try keeping the tail less the draw
/// the new branch no longer makes.
#[test]
fn try_shortening_via_increment_keeps_a_later_value_the_shorter_branch_needs() {
    let bool_node = |v: bool| ChoiceNode::boolean(BooleanChoice { p: 0.5 }, v, false);
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let used = if int_value(&nodes[0]) == 0 { 3 } else { 2 };
                if nodes.len() < used {
                    return (false, nodes.to_vec(), Spans::new());
                }
                let interesting = nodes[1..used].iter().any(bool_value);
                (interesting, nodes[..used].to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node_st(0, 0, 1, 0), bool_node(false), bool_node(true)],
        Spans::new(),
    );
    drive_no_yield(shrinker.try_shortening_via_increment()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(shrinker.current_nodes.len(), 2);
    assert!(bool_value(&shrinker.current_nodes[1]));
}

#[test]
fn integer_shrink_divides_the_distance_through_a_sparse_set_of_multiples() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let v = int_value(&nodes[0]);
                (v > 0 && v % 1000 == 0, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node_st(1_000_000_000, 0, 1_000_000_000, 0)],
        Spans::new(),
    );
    drive_no_yield(shrinker.binary_search_integer_towards_zero()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1000);
}

#[test]
fn integer_shrink_drops_to_the_trailing_zeros_of_a_prime_multiple() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let v = int_value(&nodes[0]);
                (v > 0 && v % 1000 == 0, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node_st(61_000, 0, 1_000_000_000, 0)],
        Spans::new(),
    );
    drive_no_yield(shrinker.binary_search_integer_towards_zero()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1000);
}

#[test]
fn integer_shrink_drops_to_the_trailing_zeros_of_a_prime_multiple_in_binary() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let v = int_value(&nodes[0]);
                (v > 0 && v % 1024 == 0, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node_st(61 * 1024, 0, 1_000_000_000, 0)],
        Spans::new(),
    );
    drive_no_yield(shrinker.binary_search_integer_towards_zero()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1024);
}

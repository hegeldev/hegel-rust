//! Unit tests for `Shrinker::try_shortening_via_increment`.

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

fn int_node(value: i128, max: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(max),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
        false,
    )
}

fn int_value(node: &ChoiceNode) -> i128 {
    match node.value() {
        ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
        _ => unreachable!(),
    }
}

fn int_values(shrinker: &Shrinker<'_>) -> Vec<i128> {
    shrinker.current_nodes.iter().map(int_value).collect()
}

/// A coin in `[0, 99]` gates a pick: while the coin is below 50 the test
/// draws the pick, then a value; from 50 up it draws the value alone. The
/// property fails for every input that completes, so the two-draw shape is
/// reachable only by raising the coin. A replay that runs out of choices
/// before the test is done overruns and is not interesting.
fn gate_then_pick(run: ShrinkRun<'_>) -> (bool, Vec<ChoiceNode>, Spans) {
    let ShrinkRun::Full(nodes) = run else {
        return (false, Vec::new(), Spans::new());
    };
    let Some(first) = nodes.first() else {
        return (false, Vec::new(), Spans::new());
    };
    let coin = int_value(first);
    let needed = if coin < 50 { 3 } else { 2 };
    if nodes.len() < needed {
        return (false, nodes.to_vec(), Spans::new());
    }
    let mut out = vec![int_node(coin, 99)];
    if coin < 50 {
        out.push(int_node(int_value(&nodes[1]).min(2), 2));
    }
    out.push(int_node(int_value(&nodes[needed - 1]).min(1000), 1000));
    (true, out, Spans::new())
}

fn gate_then_pick_shrinker() -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(gate_then_pick),
        vec![int_node(0, 99), int_node(0, 2), int_node(0, 1000)],
        Spans::new(),
    )
}

#[test]
fn try_shortening_via_increment_raises_a_gate_to_drop_the_draw_behind_it() {
    let mut shrinker = gate_then_pick_shrinker();
    drive_no_yield(shrinker.try_shortening_via_increment()).unwrap();
    assert_eq!(int_values(&shrinker), vec![99, 0]);
}

#[test]
fn shrink_lowers_a_raised_gate_to_the_smallest_value_that_keeps_the_shorter_shape() {
    let mut shrinker = gate_then_pick_shrinker();
    drive_no_yield(shrinker.shrink()).unwrap();
    assert_eq!(int_values(&shrinker), vec![50, 0]);
}

#[test]
fn try_shortening_via_increment_leaves_the_last_node_alone() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(_) => (true, Vec::new(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(0, 99)],
        Spans::new(),
    );
    drive_no_yield(shrinker.try_shortening_via_increment()).unwrap();
    assert_eq!(shrinker.calls, 0);
    assert_eq!(int_values(&shrinker), vec![0]);
}

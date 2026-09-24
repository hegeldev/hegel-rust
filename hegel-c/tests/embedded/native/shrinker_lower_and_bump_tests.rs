//! Unit tests for `lower_and_bump`'s descent after an accepted unit
//! decrement.
//!
//! The pass runs late in an iteration, so a node another pass has since
//! freed can be far above its boundary when the pass lowers it by one
//! index. Left at one step per pass step, the scheduler would re-step the
//! pass once per unit and spend the improvement cap before the value
//! passes get their turn again.

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

fn int_node(value: i128, min: i128, max: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(min),
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

#[test]
fn lower_and_bump_descends_after_an_accepted_unit_decrement() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (int_value(&nodes[0]) >= 1000, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(500_000, 0, 1_000_000), int_node(7, 0, 10)],
        Spans::new(),
    );
    drive_no_yield(shrinker.lower_and_bump()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1000);
    assert!(shrinker.calls < 200, "{} calls", shrinker.calls);
}

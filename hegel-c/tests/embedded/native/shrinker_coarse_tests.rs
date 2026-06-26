//! Unit tests for `Shrinker::initial_coarse_reduction`.

use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn small_int_node(value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(10),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn big_range_int_node(value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(1_000_000),
            shrink_towards: BigInt::from(0),
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

#[test]
fn initial_coarse_reduction_no_op_when_shape_stable() {
    let initial = vec![small_int_node(5)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 5);
}

#[test]
fn initial_coarse_reduction_lowers_when_shape_depends_on_value() {
    let initial = vec![
        small_int_node(3),
        small_int_node(0),
        small_int_node(0),
        small_int_node(0),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let n = match nodes.first().map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v.clone()).unwrap() as usize,
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let actual: Vec<_> = nodes[..1 + n.min(nodes.len() - 1)].to_vec();
                (true, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    assert!(int_value(&shrinker.current_nodes[0]) < 3);
}

#[test]
fn initial_coarse_reduction_skips_large_values() {
    let initial = vec![big_range_int_node(50)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 50);
}

#[test]
fn initial_coarse_reduction_skips_non_zero_min_value() {
    let mut node = small_int_node(3);
    if let ChoiceKind::Integer(ic) = std::sync::Arc::make_mut(&mut node.kind) {
        ic.min_value = BigInt::from(1);
    }
    let initial = vec![node];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 3);
}

#[test]
fn initial_coarse_reduction_skips_forced_node() {
    let mut node = small_int_node(5);
    node.was_forced = true;
    let initial = vec![node];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 5);
}

/// Initial counterexample `(1, 0)`: predicate accepts iff first integer
/// is 1. When the branch chosen by the integer doesn't change the
/// trailing shape, `initial_coarse_reduction` should leave the pair
/// untouched.
#[test]
fn initial_coarse_reduction_keeps_same_shape_one_of() {
    let initial = vec![small_int_node(1), small_int_node(0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = nodes[0].value == ChoiceValue::Integer(BigInt::from(1));
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    assert_eq!(shrinker.current_nodes.len(), 2);
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(int_value(&shrinker.current_nodes[1]), 0);
}

/// Coverage for the probe-accept branch in
/// `try_lower_node_as_alternative`: direct `replace` of the lowered
/// selector is rejected, but the random-continuation probe finds a
/// strictly smaller candidate.
#[test]
fn initial_coarse_reduction_accepts_probe_when_direct_replace_fails() {
    use std::cell::Cell;
    use std::rc::Rc;

    let initial = vec![
        small_int_node(3),
        small_int_node(0),
        small_int_node(0),
        small_int_node(0),
    ];
    let probe_calls = Rc::new(Cell::new(0_usize));
    let probe_calls_for_closure = probe_calls.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let head = match &nodes[0].value {
                    ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                if head == 3 && nodes.len() == 4 {
                    return (true, nodes.to_vec(), Spans::new());
                }
                if head == 0 && nodes.len() == 4 {
                    return (
                        false,
                        vec![small_int_node(0), small_int_node(0)],
                        Spans::new(),
                    );
                }
                (false, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => {
                probe_calls_for_closure.set(probe_calls_for_closure.get() + 1);
                (
                    true,
                    vec![small_int_node(0), small_int_node(0)],
                    Spans::new(),
                )
            }
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    assert!(probe_calls.get() > 0, "probe branch was never reached");
    assert_eq!(shrinker.current_nodes.len(), 2);
}

//! Unit tests for `Shrinker::initial_coarse_reduction`.
use crate::native::bignum::BigInt;

use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn small_int_node(value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(10),
            shrink_towards: BigInt::from(0),
        }),
        value: ChoiceValue::Integer(BigInt::from(value)),
        was_forced: false,
    }
}

fn big_range_int_node(value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(1_000_000),
            shrink_towards: BigInt::from(0),
        }),
        value: ChoiceValue::Integer(BigInt::from(value)),
        was_forced: false,
    }
}

fn int_value(node: &ChoiceNode) -> i128 {
    match &node.value {
        ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
        _ => unreachable!(),
    }
}

#[test]
fn initial_coarse_reduction_no_op_when_shape_stable() {
    // Predicate accepts everything, returns same shape → coarse phase
    // detects no shape change and defers to the main loop, leaving the
    // value untouched.
    let initial = vec![small_int_node(5)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 5);
}

#[test]
fn initial_coarse_reduction_lowers_when_shape_depends_on_value() {
    // The closure returns a shape that depends on the integer value:
    // when the integer is N, the realised sequence has length N+1.
    // Zeroing changes the shape, so the coarse pass fires.
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
                    Some(ChoiceValue::Integer(v)) => usize::try_from(v).unwrap(),
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
    shrinker.initial_coarse_reduction();
    // The shape-aware coarse pass should drop the integer.
    assert!(int_value(&shrinker.current_nodes[0]) < 3);
}

#[test]
fn initial_coarse_reduction_skips_large_values() {
    // value > 10 → the heuristic skips it.
    let initial = vec![big_range_int_node(50)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction();
    // Unchanged.
    assert_eq!(int_value(&shrinker.current_nodes[0]), 50);
}

#[test]
fn initial_coarse_reduction_skips_non_zero_min_value() {
    // Node has min_value=1; not a one_of selector pattern (those start
    // from zero).  Should be left alone.
    let mut node = small_int_node(3);
    if let ChoiceKind::Integer(ic) = &mut node.kind {
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
    shrinker.initial_coarse_reduction();
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
    shrinker.initial_coarse_reduction();
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
                // Always interesting iff first value == 1.
                let interesting = matches!(&nodes[0].value, ChoiceValue::Integer(v) if *v == 1);
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction();
    // Sequence should remain (1, 0); coarse phase doesn't lower the
    // selector when there's no shape change to exploit.
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
    // Track how many times the probe branch responded — used to assert
    // we actually reached it.
    let probe_calls = Rc::new(Cell::new(0_usize));
    let probe_calls_for_closure = probe_calls.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let head = match &nodes[0].value {
                    ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                if head == 3 && nodes.len() == 4 {
                    // The initial counterexample.  Interesting.
                    return (true, nodes.to_vec(), Spans::new());
                }
                if head == 0 && nodes.len() == 4 {
                    // The coarse pass's zeroing probe.  Return a
                    // different-shape realised sequence to flag
                    // shape_changed = true.
                    return (
                        false,
                        vec![small_int_node(0), small_int_node(0)],
                        Spans::new(),
                    );
                }
                // try_lower's direct `replace` lowers `head` to v in
                // 0..3.  Reject those so the probe branch runs.
                (false, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => {
                probe_calls_for_closure.set(probe_calls_for_closure.get() + 1);
                // Return a strictly shorter interesting sequence so
                // sort_key < initial_key and the probe arm at
                // `coarse.rs:96` fires.
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
    shrinker.initial_coarse_reduction();
    assert!(probe_calls.get() > 0, "probe branch was never reached");
    assert_eq!(shrinker.current_nodes.len(), 2);
}

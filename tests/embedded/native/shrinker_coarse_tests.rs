//! Unit tests for `Shrinker::initial_coarse_reduction`.
//!
//! Hypothesis reference: `shrinker.py:689-801`.

use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn small_int_node(value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: 0,
            max_value: 10,
            shrink_towards: 0,
        }),
        value: ChoiceValue::Integer(value),
        was_forced: false,
    }
}

fn big_range_int_node(value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: 0,
            max_value: 1_000_000,
            shrink_towards: 0,
        }),
        value: ChoiceValue::Integer(value),
        was_forced: false,
    }
}

fn int_value(node: &ChoiceNode) -> i128 {
    match node.value {
        ChoiceValue::Integer(v) => v,
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
                    Some(ChoiceValue::Integer(v)) => *v as usize,
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
        ic.min_value = 1;
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

/// Port of Hypothesis `test_shrinking_one_of_with_same_shape`
/// (`tests/conjecture/test_shrinker.py`).  Initial counterexample
/// `(1, 0)`: predicate accepts iff first integer is 1.  When the
/// branch chosen by the integer doesn't change the trailing shape,
/// `initial_coarse_reduction` should leave the pair untouched.
#[test]
fn initial_coarse_reduction_keeps_same_shape_one_of() {
    let initial = vec![small_int_node(1), small_int_node(0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                // Always interesting iff first value == 1.
                let interesting = matches!(nodes[0].value, ChoiceValue::Integer(1));
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

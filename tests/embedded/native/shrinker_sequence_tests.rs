//! Unit tests for the sequence-ordering shrink pass (`sort_values`).

use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: 0,
            max_value: 100,
            shrink_towards: 0,
        }),
        value: ChoiceValue::Integer(value),
        was_forced: false,
    }
}

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(value),
        was_forced: false,
    }
}

fn int_values(shrinker: &Shrinker) -> Vec<i128> {
    shrinker
        .current_nodes
        .iter()
        .map(|n| match n.value {
            ChoiceValue::Integer(v) => v,
            _ => unreachable!(),
        })
        .collect()
}

/// An always-accepting probe takes the full sort in one shot: `try_sort_group`
/// builds the sorted-order replacement, `replace` succeeds, and the pass
/// returns immediately (its bulk-sort fast path) rather than falling back to
/// the per-swap insertion sort.
#[test]
fn sort_values_takes_the_full_sort_when_accepted() {
    let initial = vec![int_node(5), int_node(1), int_node(3)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.sort_values_integers();
    assert_eq!(int_values(&shrinker), vec![1, 3, 5]);
}

/// Booleans sort `false` (0) before `true` (1) via the same bulk path.
#[test]
fn sort_values_sorts_booleans() {
    let initial = vec![bool_node(true), bool_node(false), bool_node(true)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.sort_values_booleans();
    let bools: Vec<bool> = shrinker
        .current_nodes
        .iter()
        .map(|n| matches!(n.value, ChoiceValue::Boolean(true)))
        .collect();
    assert_eq!(bools, vec![false, true, true]);
}

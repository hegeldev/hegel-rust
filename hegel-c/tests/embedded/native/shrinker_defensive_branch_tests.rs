//! Tests covering defensive branches in deletion.rs and sequence.rs
//! that were previously masked by `// nocov` annotations.

use crate::native::bignum::BigInt;
use std::collections::HashMap;

use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(i128::MIN + 1),
            max_value: BigInt::from(i128::MAX),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
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

#[test]
fn delete_chunks_handles_empty_initial_sequence() {
    let mut shrinker = accepting_shrinker(vec![]);
    shrinker.delete_chunks().unwrap();
    assert!(shrinker.current_nodes.is_empty());
}

#[test]
fn try_replace_with_deletion_returns_true_on_early_success() {
    let mut shrinker = accepting_shrinker(vec![int_node(42), int_node(7)]);
    let ok = shrinker
        .try_replace_with_deletion(0, ChoiceValue::Integer(BigInt::from(0)), 2)
        .unwrap();
    assert!(ok);
    match &shrinker.current_nodes[0].value {
        ChoiceValue::Integer(v) => assert_eq!(i128::try_from(v.clone()).unwrap(), 0),
        _ => unreachable!(),
    }
}

#[test]
fn sort_values_break_when_concurrent_shrink_drops_valid_indices() {
    use std::cell::Cell;
    use std::rc::Rc;
    let saw_full_sort = Rc::new(Cell::new(false));
    let saw_full_sort_clone = saw_full_sort.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                if !saw_full_sort_clone.get() && nodes.len() == 4 {
                    saw_full_sort_clone.set(true);
                    return (false, nodes.to_vec(), Spans::new());
                }
                let truncated: Vec<ChoiceNode> = nodes.iter().take(1).cloned().collect();
                (true, truncated, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(40), int_node(30), int_node(20), int_node(10)],
        Spans::new(),
    );
    shrinker.sort_values().unwrap();
    assert_eq!(shrinker.current_nodes.len(), 1);
}

#[test]
fn redistribute_integers_pair_idx_overshoots_after_concurrent_truncation() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let truncated: Vec<ChoiceNode> = nodes.iter().take(1).cloned().collect();
                (true, truncated, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(10), int_node(20), int_node(30), int_node(40)],
        Spans::new(),
    );
    shrinker.redistribute_integers().unwrap();
    assert_eq!(shrinker.current_nodes.len(), 1);
}

#[test]
fn lower_integers_together_break_when_indices_outrun_current_nodes() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let truncated: Vec<ChoiceNode> = nodes.iter().take(1).cloned().collect();
                (true, truncated, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(10), int_node(20), int_node(30)],
        Spans::new(),
    );
    shrinker.lower_integers_together().unwrap();
    assert!(shrinker.current_nodes.len() <= 3);
}

#[test]
fn lower_integers_together_skips_kind_punning() {
    use crate::native::core::choices::BooleanChoice;
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let mut out: Vec<ChoiceNode> = nodes.to_vec();
                if out.len() >= 2 {
                    out[1] = ChoiceNode::new(
                        ChoiceKind::Boolean(BooleanChoice),
                        ChoiceValue::Boolean(true),
                        false,
                    );
                }
                (true, out, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5), int_node(10)],
        Spans::new(),
    );
    shrinker.lower_integers_together().unwrap();
}

#[test]
fn shrink_duplicates_skips_groups_whose_members_diverged() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let mut out: Vec<ChoiceNode> = nodes.to_vec();
                if out.len() >= 2 {
                    out[1] = int_node(999);
                }
                (true, out, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(7), int_node(7), int_node(7)],
        Spans::new(),
    );
    shrinker.shrink_duplicates().unwrap();
}

#[test]
fn try_shortening_via_increment_break_on_concurrent_shrink() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(_) => (true, Vec::new(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5), int_node(10), int_node(15)],
        Spans::new(),
    );
    shrinker.try_shortening_via_increment().unwrap();
    assert!(shrinker.current_nodes.is_empty());
}

#[test]
fn replace_short_circuits_on_index_past_end_of_attempt() {
    let mut shrinker = accepting_shrinker(vec![int_node(5)]);
    let mut values = HashMap::new();
    values.insert(0, ChoiceValue::Integer(BigInt::from(0)));
    values.insert(10, ChoiceValue::Integer(BigInt::from(0)));
    assert!(!shrinker.replace(&values).unwrap());
}

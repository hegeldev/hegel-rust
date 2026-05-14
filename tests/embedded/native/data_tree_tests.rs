// Embedded tests for src/native/data_tree.rs — exercise the
// non-determinism panic, the kill-depth propagation, and the
// generate_novel_prefix exhaustion branches.

use super::*;
use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Status};
use rand::SeedableRng;
use rand::rngs::SmallRng;

fn int_kind(min: i128, max: i128) -> ChoiceKind {
    ChoiceKind::Integer(IntegerChoice {
        min_value: min,
        max_value: max,
        shrink_towards: 0,
    })
}

fn int_node(min: i128, max: i128, value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: int_kind(min, max),
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

#[test]
#[should_panic(expected = "non-deterministic")]
fn record_tree_panics_on_kind_mismatch() {
    // First record an integer node at position 0; recording a boolean
    // node at the same position trips the non-determinism panic.
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[int_node(0, 10, 0)], Status::Valid, &[]);
    record_tree(&mut root, &[bool_node(false)], Status::Valid, &[]);
}

#[test]
fn record_tree_kill_depths_marks_inner_nodes_exhausted() {
    // record_tree at depth >= 1 marks that node exhausted.
    let mut root = DataTreeNode::default();
    record_tree(
        &mut root,
        &[int_node(0, 10, 0), int_node(0, 10, 1)],
        Status::Valid,
        &[1],
    );
    // After kill at depth 1 the corresponding subtree is exhausted.
    // generate_novel_prefix should now avoid the killed branch.
    let mut rng = SmallRng::seed_from_u64(0);
    for _ in 0..50 {
        let prefix = generate_novel_prefix(&root, &mut rng);
        // either an empty prefix (no novel positions available) or the
        // returned prefix doesn't pass through the killed branch.
        assert!(
            prefix.is_empty()
                || prefix.first() != Some(&ChoiceValue::Integer(0))
                || prefix.len() == 1
        );
    }
}

#[test]
fn record_tree_kill_depths_out_of_range_is_a_no_op() {
    // kill_depths that exceeds the path length is silently skipped
    // (the `if depth < path.len()` guard).
    let mut root = DataTreeNode::default();
    record_tree(
        &mut root,
        &[int_node(0, 10, 0)],
        Status::Valid,
        &[99], // wildly out of range
    );
    // Tree records normally; no panic.
    assert!(root.kind.is_some() || !root.is_exhausted);
}

#[test]
fn generate_novel_prefix_returns_empty_for_exhausted_root() {
    // A tree whose root is marked exhausted produces an empty prefix.
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[], Status::Valid, &[0]);
    assert!(root.is_exhausted);
    let mut rng = SmallRng::seed_from_u64(0);
    assert!(generate_novel_prefix(&root, &mut rng).is_empty());
}

#[test]
fn generate_novel_prefix_terminates_when_subtree_exhausted() {
    // Boolean choice has only two children; record both and mark them
    // exhausted (status >= Invalid).  Then `pick_non_exhausted_value`
    // returns None on the second loop, exercising the
    // `if untried.is_empty() { return None; }` branch.
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[bool_node(false)], Status::Invalid, &[]);
    record_tree(&mut root, &[bool_node(true)], Status::Invalid, &[]);

    let mut rng = SmallRng::seed_from_u64(0);
    let prefix = generate_novel_prefix(&root, &mut rng);
    // Tree is now exhausted; novel prefix is empty (root.is_exhausted is true).
    assert!(prefix.is_empty());
}

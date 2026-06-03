// Embedded tests for src/native/data_tree.rs — exercise the
// non-determinism panic, the kill-depth propagation, and the
// generate_novel_prefix exhaustion branches.

use super::*;
use crate::native::bignum::BigInt;
use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Status};
use crate::native::rng::EngineRng;

fn int_kind(min: i128, max: i128) -> ChoiceKind {
    ChoiceKind::Integer(IntegerChoice {
        min_value: BigInt::from(min),
        max_value: BigInt::from(max),
        shrink_towards: BigInt::from(0),
    })
}

fn int_node(min: i128, max: i128, value: i128) -> ChoiceNode {
    ChoiceNode::new(
        int_kind(min, max),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(value),
        false,
    )
}

#[test]
fn record_tree_reports_kind_mismatch() {
    // First record an integer node at position 0; recording a boolean node at
    // the same position reports the non-determinism (rather than panicking, so
    // an FFI-driven engine doesn't abort).
    let mut root = DataTreeNode::default();
    assert!(record_tree(&mut root, &[int_node(0, 10, 0)], Status::Valid, &[]).is_none());
    let msg = record_tree(&mut root, &[bool_node(false)], Status::Valid, &[])
        .expect("kind mismatch should be reported");
    assert!(msg.contains("non-deterministic"), "{msg}");
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
    let mut rng = EngineRng::seeded(0);
    for _ in 0..50 {
        let prefix = generate_novel_prefix(&root, &mut rng);
        // either an empty prefix (no novel positions available) or the
        // returned prefix doesn't pass through the killed branch.
        assert!(
            prefix.is_empty()
                || prefix.first() != Some(&ChoiceValue::Integer(BigInt::from(0)))
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
    let mut rng = EngineRng::seeded(0);
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

    let mut rng = EngineRng::seeded(0);
    let prefix = generate_novel_prefix(&root, &mut rng);
    // Tree is now exhausted; novel prefix is empty (root.is_exhausted is true).
    assert!(prefix.is_empty());
}

#[test]
fn simulate_unseen_on_empty_tree() {
    // Nothing recorded: any choices are previously-unseen behaviour.
    let root = DataTreeNode::default();
    assert_eq!(simulate(&root, &[ChoiceValue::Boolean(false)]), None);
    assert_eq!(simulate(&root, &[]), None);
}

#[test]
fn simulate_returns_recorded_conclusion_for_exact_match() {
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[bool_node(false)], Status::Valid, &[]);
    assert_eq!(
        simulate(&root, &[ChoiceValue::Boolean(false)]),
        Some(Status::Valid)
    );
}

#[test]
fn simulate_ignores_trailing_choices_past_a_conclusion() {
    // The recorded run drew a single boolean and concluded; replaying a
    // longer sequence that shares that prefix never reads past the first
    // choice, so the outcome is the recorded one — this is the fixed-shape
    // case span mutation hits when it duplicates a span the test ignores.
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[bool_node(false)], Status::Invalid, &[]);
    assert_eq!(
        simulate(
            &root,
            &[
                ChoiceValue::Boolean(false),
                ChoiceValue::Boolean(true),
                ChoiceValue::Boolean(true),
            ],
        ),
        Some(Status::Invalid)
    );
}

#[test]
fn simulate_unseen_when_path_diverges() {
    // The recorded path went through `false`; `true` is an unrecorded
    // child, so the outcome is unknown.
    let mut root = DataTreeNode::default();
    record_tree(
        &mut root,
        &[bool_node(false), bool_node(false)],
        Status::Valid,
        &[],
    );
    assert_eq!(simulate(&root, &[ChoiceValue::Boolean(true)]), None);
}

#[test]
fn simulate_unseen_when_choices_run_out_mid_path() {
    // The recorded run drew two booleans; supplying only the first leaves
    // the tree still expecting a draw, so the real run would read past what
    // we hold — report unseen rather than guessing.
    let mut root = DataTreeNode::default();
    record_tree(
        &mut root,
        &[bool_node(false), bool_node(true)],
        Status::Valid,
        &[],
    );
    assert_eq!(simulate(&root, &[ChoiceValue::Boolean(false)]), None);
}

#[test]
fn simulate_returns_interesting_conclusion() {
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[int_node(0, 100, 7)], Status::Interesting, &[]);
    assert_eq!(
        simulate(&root, &[ChoiceValue::Integer(BigInt::from(7))]),
        Some(Status::Interesting)
    );
}

#[test]
fn simulate_follows_forced_value_ignoring_prefix() {
    // A forced draw ignores the replayed prefix value and always reproduces
    // the recorded (forced) value, so simulation must follow the single
    // forced child even when the supplied choice differs.
    let forced_true = ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(true),
        true,
    );
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[forced_true], Status::Valid, &[]);

    // Supplying `false` (≠ the forced `true`) still resolves to the forced
    // path and its conclusion.
    assert_eq!(
        simulate(&root, &[ChoiceValue::Boolean(false)]),
        Some(Status::Valid)
    );
}

#[test]
fn simulate_puns_out_of_range_prefix_to_unit() {
    // For Integer{0,10}, simplest() == 0 and unit() == 1. A replayed prefix
    // value outside the range fails validation and puns to unit() (a bare
    // `for_choices` replay has no original-kind info, so the `simplest()`
    // branch never applies), so a recorded path through the unit value is
    // matched.
    let mut root = DataTreeNode::default();
    record_tree(&mut root, &[int_node(0, 10, 1)], Status::Valid, &[]);

    // 999 is out of range → punned to unit() == 1 → matches the recorded path.
    assert_eq!(
        simulate(&root, &[ChoiceValue::Integer(BigInt::from(999))]),
        Some(Status::Valid)
    );
    // 7 is in range → used as-is → diverges from the recorded value (1).
    assert_eq!(
        simulate(&root, &[ChoiceValue::Integer(BigInt::from(7))]),
        None
    );
}

//! Unit tests for the `ChoiceTree` / `Chooser` infrastructure (Step 13).
//!
//! Hypothesis reference: `shrinking/choicetree.py`.

use super::{ChoiceTree, prefix_selection_order, random_selection_order};

#[test]
fn choicetree_step_records_decisions_and_marks_exhausted() {
    let mut tree = ChoiceTree::default();
    let values = vec!["a", "b", "c"];
    let choices = tree.step(prefix_selection_order(vec![]), |chooser| {
        let _ = chooser.choose(&values, |_| true)?;
        Ok(())
    });
    // prefix_selection_order with empty prefix walks n-1..=0 → picks
    // index 2 (the rightmost) first.
    assert_eq!(choices, vec![2]);
}

#[test]
fn choicetree_step_skips_exhausted_branches_on_replay() {
    let mut tree = ChoiceTree::default();
    let values = vec!["a", "b", "c"];
    // First step: picks 2.
    let first = tree.step(prefix_selection_order(vec![]), |chooser| {
        let _ = chooser.choose(&values, |_| true)?;
        Ok(())
    });
    // Second step: picks 1 (next-rightmost), since 2 is now exhausted.
    let second = tree.step(prefix_selection_order(vec![]), |chooser| {
        let _ = chooser.choose(&values, |_| true)?;
        Ok(())
    });
    // Third step: picks 0.
    let third = tree.step(prefix_selection_order(vec![]), |chooser| {
        let _ = chooser.choose(&values, |_| true)?;
        Ok(())
    });
    assert_eq!(first, vec![2]);
    assert_eq!(second, vec![1]);
    assert_eq!(third, vec![0]);
    assert!(tree.exhausted());
}

#[test]
fn choicetree_step_respects_condition_and_marks_failing_branches_dead() {
    let mut tree = ChoiceTree::default();
    let values = vec![1, 2, 3, 4];
    // Condition: only pick odd numbers.
    let first = tree.step(prefix_selection_order(vec![]), |chooser| {
        let _ = chooser.choose(&values, |&v| v % 2 == 1)?;
        Ok(())
    });
    // Expect index 2 → value 3 (the largest odd). Indices 3 (=4) and 1
    // (=2) are filtered out by the condition.
    assert_eq!(first, vec![2]);

    let second = tree.step(prefix_selection_order(vec![]), |chooser| {
        let _ = chooser.choose(&values, |&v| v % 2 == 1)?;
        Ok(())
    });
    assert_eq!(second, vec![0]); // value 1
    assert!(tree.exhausted());
}

#[test]
fn prefix_selection_order_starts_from_prefix_and_walks_left() {
    let mut order = prefix_selection_order(vec![2]);
    let seq = order(0, 5);
    // Starting from prefix[0]=2, walk left to 0 then jump to right end
    // (4, 3).  Yields 2, 1, 0, 4, 3.
    assert_eq!(seq, vec![2, 1, 0, 4, 3]);
}

#[test]
fn random_selection_order_yields_full_permutation() {
    let mut order = random_selection_order(7);
    let seq = order(0, 4);
    let mut sorted = seq.clone();
    sorted.sort();
    assert_eq!(sorted, vec![0, 1, 2, 3]);
}

#[test]
fn choicetree_handles_empty_values_as_dead_branch() {
    let mut tree = ChoiceTree::default();
    let values: Vec<&str> = Vec::new();
    let first = tree.step(prefix_selection_order(vec![]), |chooser| {
        let _ = chooser.choose(&values, |_| true)?;
        Ok(())
    });
    assert!(first.is_empty());
    assert!(tree.exhausted());
}

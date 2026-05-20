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
fn choicetree_rejecting_every_alternative_raises_dead_branch() {
    // Non-empty values + condition that rejects every entry → the
    // for loop exhausts every alternative via the "mark dead" branch
    // and falls through to the post-loop `Err(DeadBranch)` return.
    let mut tree = ChoiceTree::default();
    let values = vec![1, 2, 3, 4];
    let choices = tree.step(prefix_selection_order(vec![]), |chooser| {
        let _ = chooser.choose(&values, |_v: &i32| false)?;
        Ok(())
    });
    assert!(choices.is_empty());
    assert!(tree.exhausted());
}

#[test]
fn choicetree_break_when_live_count_hits_zero_mid_loop() {
    // Set up a state where the live children appear *first* in the
    // iteration order and the exhausted children appear *after* them.
    // After the live ones are marked dead the live_child_count is
    // zero, and the next iteration's start-of-loop check trips the
    // early break at line ~140.
    let mut tree = ChoiceTree::default();
    let values = vec![10, 20, 30, 40];

    // Step 1: deterministic picks at indices 2 and 3.  After this
    // step both indices 2 and 3 are exhausted in tree.root.
    let _ = tree.step(prefix_selection_order(vec![]), |chooser| {
        // Pick idx 2 (val 30) — prefix_selection_order(vec![]) yields
        // [3, 2, 1, 0]; condition rejects 40 (idx 3), accepts 30.
        let _ = chooser.choose(&values, |&v| v == 30)?;
        Ok(())
    });

    // Step 2: condition rejects every alternative.  Use a custom
    // selection order that visits the live children first (0, 1)
    // *then* the already-exhausted (2, 3).  Once we've marked both
    // live children dead, live_child_count is 0; iteration over
    // i=2 then trips the start-of-loop break.
    let custom_order: super::SelectionOrder = Box::new(|_, _| vec![0, 1, 2, 3]);
    let second = tree.step(custom_order, |chooser| {
        let _ = chooser.choose(&values, |_| false)?;
        Ok(())
    });
    assert!(second.is_empty());
    assert!(tree.exhausted());
}

#[test]
fn choicetree_finish_break_when_leaf_not_exhausted() {
    // A multi-depth choice where the leaf hasn't been fully explored
    // yet — finish() bubbles up only to the depth where the node is
    // exhausted, then breaks out of the bubble-up loop.
    let mut tree = ChoiceTree::default();
    // Two depths of choice.  Each step picks one path; after the
    // first step, the leaf [1][0] is exhausted but its parent at
    // depth 1 is not (still has unexplored sibling at idx 1).
    let _ = tree.step(prefix_selection_order(vec![]), |chooser| {
        let _outer = chooser.choose(&[10, 20], |_| true)?;
        let _inner = chooser.choose(&[100, 200], |_| true)?;
        Ok(())
    });
    // Second step starts at the same outer branch but a different
    // inner branch — its finish() bubble-up should stop at the outer
    // node which still has live children.
    let _ = tree.step(prefix_selection_order(vec![]), |chooser| {
        let _outer = chooser.choose(&[10, 20], |_| true)?;
        let _inner = chooser.choose(&[100, 200], |_| true)?;
        Ok(())
    });
    // Tree still has one live outer branch; not yet fully exhausted.
    assert!(!tree.exhausted());
}

#[test]
fn prefix_selection_order_empty_n_yields_empty() {
    let mut order = prefix_selection_order(vec![1]);
    assert!(order(0, 0).is_empty());
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

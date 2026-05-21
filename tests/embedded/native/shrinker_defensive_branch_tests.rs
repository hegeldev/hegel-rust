//! Tests covering defensive branches in deletion.rs and sequence.rs
//! that were previously masked by `// nocov` annotations (Step 5 of
//! the audit cleanup).

use std::collections::HashMap;

use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: i128::MIN + 1,
            max_value: i128::MAX,
            shrink_towards: 0,
        }),
        value: ChoiceValue::Integer(value),
        was_forced: false,
    }
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
    // Starting from an empty sequence, the outer loop's
    // `i = saturating_sub(0, k+1) = 0` and the inner-loop
    // `i >= current_nodes.len()` (0 >= 0) is true on entry, hitting
    // the previously-nocov break.
    let mut shrinker = accepting_shrinker(vec![]);
    shrinker.delete_chunks();
    assert!(shrinker.current_nodes.is_empty());
}

#[test]
fn try_replace_with_deletion_returns_true_on_early_success() {
    // Predicate accepts everything; replacing index 0 with the
    // simplest value succeeds straight through the early-success
    // path that the nocov masked.
    let mut shrinker = accepting_shrinker(vec![int_node(42), int_node(7)]);
    let ok = shrinker.try_replace_with_deletion(0, ChoiceValue::Integer(0), 2);
    assert!(ok);
    match shrinker.current_nodes[0].value {
        ChoiceValue::Integer(v) => assert_eq!(v, 0),
        _ => unreachable!(),
    }
}

#[test]
fn sort_values_break_when_concurrent_shrink_drops_valid_indices() {
    // Drive `sort_values` against a test_fn that truncates the
    // sequence on every Full run.  After the first replace succeeds,
    // current_nodes is shorter; the next iteration's re-filter
    // produces a `valid` whose len < j, hitting the previously-nocov
    // break.
    use std::cell::Cell;
    use std::rc::Rc;
    let calls = Rc::new(Cell::new(0_usize));
    let calls_clone = calls.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                calls_clone.set(calls_clone.get() + 1);
                // Always truncate to the first node so concurrent
                // shrinks shorten current_nodes mid-pass.
                let truncated: Vec<ChoiceNode> = nodes.iter().take(1).cloned().collect();
                (true, truncated, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5), int_node(4), int_node(3), int_node(2)],
        Spans::new(),
    );
    shrinker.sort_values();
    assert_eq!(shrinker.current_nodes.len(), 1);
}

#[test]
fn redistribute_integers_pair_idx_overshoots_after_concurrent_truncation() {
    // test_fn truncates current_nodes to a single integer on every Full
    // run, so pair_idx + gap (built from a stale int_indices snapshot)
    // overshoots current_ints.len() — the defensive branch decrements
    // pair_idx and continues.  Without coverage on that branch the
    // function would silently UB on the index when concurrent shrinks
    // run during a real shrink pass.
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
    shrinker.redistribute_integers();
    assert_eq!(shrinker.current_nodes.len(), 1);
}

#[test]
fn lower_integers_together_break_when_indices_outrun_current_nodes() {
    // Same shape: every Full run truncates current_nodes, so the
    // i/j indices captured in the pass's int_indices snapshot
    // overshoot the live shrink target on the next iteration.
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
    shrinker.lower_integers_together();
    assert!(shrinker.current_nodes.len() <= 3);
}

#[test]
fn lower_integers_together_skips_kind_punning() {
    // test_fn rewrites the second integer node to a Boolean kind on
    // every replay so `lower_integers_together`'s `let
    // ChoiceKind::Integer(ic_j) = ...` continue-arm fires.
    use crate::native::core::choices::BooleanChoice;
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let mut out: Vec<ChoiceNode> = nodes.to_vec();
                if out.len() >= 2 {
                    out[1] = ChoiceNode {
                        kind: ChoiceKind::Boolean(BooleanChoice),
                        value: ChoiceValue::Boolean(true),
                        was_forced: false,
                    };
                }
                (true, out, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5), int_node(10)],
        Spans::new(),
    );
    shrinker.lower_integers_together();
}

#[test]
fn shrink_duplicates_skips_groups_whose_members_diverged() {
    // The group key is (kind, value).  A prior pass that changed one
    // of the duplicates breaks the duplicate property; the
    // re-validation filter rejects the now-divergent group and the
    // pass continues with the next group.
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                // Always re-write the second element to a different value
                // so the (value-keyed) group of 3 collapses to 1 valid.
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
    shrinker.shrink_duplicates();
}

#[test]
fn try_shortening_via_increment_break_on_concurrent_shrink() {
    // try_shortening_via_increment iterates candidates per node; if a
    // prior consider in the same loop body shortens the sequence past
    // i, the inner `if i >= len { break }` fires.
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            // Truncate to empty so the loop's `i` overshoots.
            ShrinkRun::Full(_) => (true, Vec::new(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5), int_node(10), int_node(15)],
        Spans::new(),
    );
    shrinker.try_shortening_via_increment();
    assert!(shrinker.current_nodes.is_empty());
}

#[test]
fn replace_short_circuits_on_index_past_end_of_attempt() {
    // Doubly cover the replace L317 path: build a HashMap with two
    // entries, one in-range and one beyond, to ensure the early-return
    // doesn't depend on iteration order.
    let mut shrinker = accepting_shrinker(vec![int_node(5)]);
    let mut values = HashMap::new();
    values.insert(0, ChoiceValue::Integer(0));
    values.insert(10, ChoiceValue::Integer(0));
    assert!(!shrinker.replace(&values));
}

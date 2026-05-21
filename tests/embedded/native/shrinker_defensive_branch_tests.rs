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

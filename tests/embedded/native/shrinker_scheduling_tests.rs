//! Unit tests for `Shrinker::fixate_shrink_passes` (Step 12).
//!
//! Hypothesis reference: `shrinker.py:837-929`.

use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkPass, ShrinkRun, Shrinker};

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

#[test]
fn fixate_shrink_passes_runs_passes_to_fixed_point() {
    let initial = vec![int_node(10), int_node(20)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    let mut passes = vec![ShrinkPass::new(
        "zero_choices",
        Box::new(|sh| sh.zero_choices()),
    )];
    shrinker.fixate_shrink_passes(&mut passes);
    // Accepting predicate → integers driven to 0.
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => *v,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![0, 0]);
    // Stats: at least one shrink + one call recorded.
    let stats = shrinker.pass_stats(&passes);
    assert_eq!(stats.len(), 1);
    let (_, calls, shrinks, _) = stats[0];
    assert!(calls >= 1);
    assert!(shrinks >= 1);
}

#[test]
fn fixate_shrink_passes_records_deletion_stat_when_pass_shortens() {
    // Use `delete_chunks` against an accepting predicate; the pass
    // strips nodes one chunk at a time, so deletions get counted.
    let initial = vec![int_node(1); 5];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    let mut passes = vec![ShrinkPass::new(
        "delete_chunks",
        Box::new(|sh| sh.delete_chunks()),
    )];
    shrinker.fixate_shrink_passes(&mut passes);
    assert!(shrinker.current_nodes.is_empty());
    let stats = shrinker.pass_stats(&passes);
    let (_, _, _, deletions) = stats[0];
    assert!(deletions >= 1);
}

#[test]
fn fixate_shrink_passes_reorders_useful_passes_to_the_front() {
    // Pass A: does nothing (useless).  Pass B: actually shrinks the
    // integer.  After fixate, the next iteration should run B first.
    let initial = vec![int_node(5)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    let mut passes = vec![
        ShrinkPass::new("useless", Box::new(|_| ())),
        ShrinkPass::new(
            "useful",
            Box::new(|sh| sh.binary_search_integer_towards_zero()),
        ),
    ];
    shrinker.fixate_shrink_passes(&mut passes);
    // After fixate the useful pass should sit at index 0 (key 0 < 1).
    assert_eq!(passes[0].name, "useful");
    assert_eq!(passes[1].name, "useless");
}

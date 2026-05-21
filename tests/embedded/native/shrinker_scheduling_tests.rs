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
    let (_, calls, shrinks, _, _) = stats[0];
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
    let (_, _, _, deletions, _) = stats[0];
    assert!(deletions >= 1);
}

#[test]
fn consider_short_circuits_when_stalled() {
    // Set max_stall low; feed an uninteresting candidate over and over.
    // After max_stall closure calls without a shrink, consider() should
    // return false immediately without invoking the closure again.
    //
    // The stall guard only fires after at least one improvement has
    // been recorded (warmup: see the field doc for `max_stall`), so
    // seed an interesting smaller candidate first.
    use std::cell::Cell;
    use std::rc::Rc;
    let counter = Rc::new(Cell::new(0_usize));
    let counter_clone = counter.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                counter_clone.set(counter_clone.get() + 1);
                // Anything < 5 is interesting and strictly smaller.
                let interesting = matches!(nodes[0].value, ChoiceValue::Integer(v) if v < 5);
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5)],
        Spans::new(),
    );
    // Seed one improvement so the stall guard's warmup is satisfied.
    shrinker.consider(&[int_node(3)]);
    let baseline = counter.get();
    shrinker.max_stall = 10;
    // Reset calls_at_last_shrink so we measure the post-baseline budget.
    shrinker.calls_at_last_shrink = shrinker.calls;
    for v in 10..60 {
        shrinker.consider(&[int_node(v)]);
    }
    // Post-baseline closure calls capped at max_stall.
    assert!(
        counter.get() - baseline <= 10,
        "test_fn invoked {} times post-baseline, expected <= 10",
        counter.get() - baseline
    );
}

#[test]
fn max_stall_grows_after_shrink() {
    // A test_fn that's interesting for v < 10 but uninteresting
    // otherwise.  Each successful shrink should grow max_stall by
    // 2 * (calls - calls_at_last_shrink) so the shrinker doesn't
    // run out of budget on long descents.
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let v = match &nodes[0].value {
                    ChoiceValue::Integer(v) => *v,
                    _ => unreachable!(),
                };
                (v < 10, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(20)],
        Spans::new(),
    );
    // Lower max_stall so the grow step is observable without burning
    // hundreds of calls.
    shrinker.max_stall = 5;
    // Seed an improvement first to anchor calls_at_last_shrink.
    let accepted_first = shrinker.consider(&[int_node(9)]);
    assert!(accepted_first);
    let stall_after_first = shrinker.max_stall;
    // Burn 3 uninteresting calls (still within stall budget).
    for v in 11..14 {
        shrinker.consider(&[int_node(v)]);
    }
    // Another improvement.  span = calls - calls_at_last_shrink ≈ 3;
    // grown = 6 > 5, so max_stall should grow.
    shrinker.consider(&[int_node(5)]);
    assert!(
        shrinker.max_stall > stall_after_first,
        "max_stall failed to grow: {} -> {}",
        stall_after_first,
        shrinker.max_stall
    );
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

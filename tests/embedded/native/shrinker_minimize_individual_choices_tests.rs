//! Unit tests for `Shrinker::minimize_individual_choices` (Step 8).
//!
//! Hypothesis reference: `shrinker.py:1710-1808`.

use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Span, Spans};
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

fn forced_int_node(value: i128) -> ChoiceNode {
    let mut n = int_node(value);
    n.was_forced = true;
    n
}

fn int_value(node: &ChoiceNode) -> i128 {
    match node.value {
        ChoiceValue::Integer(v) => v,
        _ => unreachable!(),
    }
}

#[test]
fn minimize_individual_choices_drives_int_to_simplest_when_predicate_admits() {
    // Accepting predicate: the bin_search loop drives the integer all
    // the way to zero.
    let initial = vec![int_node(20)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.minimize_individual_choices();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 0);
}

#[test]
fn minimize_individual_choices_skips_forced_nodes() {
    let initial = vec![forced_int_node(7)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.minimize_individual_choices();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 7);
}

#[test]
fn minimize_individual_choices_invokes_span_delete_fallback() {
    // Set up a "size-controlling" integer: the first node decides how
    // many of the following nodes will be drawn.  When the integer is
    // lowered by 1, the realised actual_nodes is shorter — the fallback
    // tries deleting one of the trailing spans / nodes.  Predicate
    // accepts iff the integer is >= 1 *and* there's a trailing pair of
    // ones.
    //
    // Initial value: integer = 3, followed by three 1s.  Lowering to 2
    // produces a shorter actual_nodes (since the test "would" draw only
    // 2 elements).  The fallback should delete one of the trailing 1s
    // to make the candidate match.
    let initial = vec![int_node(3), int_node(1), int_node(1), int_node(1)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                // Read the integer at index 0.
                let count = match nodes.first().map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => *v as usize,
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let needed_len = 1 + count;
                let actual_len = needed_len.min(nodes.len());
                let actual_nodes = nodes[..actual_len].to_vec();
                // Build a single span around the trailing region for the
                // fallback to splice out.
                let mut spans = Spans::new();
                if actual_len > 1 {
                    spans.push(Span {
                        start: 1,
                        end: actual_len,
                        label: "list".to_string(),
                        depth: 0,
                        parent: None,
                        discarded: false,
                    });
                }
                let ok = actual_len >= 2 && count >= 1;
                (ok, actual_nodes, spans)
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.minimize_individual_choices();
    // After convergence the integer is at its minimum admissible value
    // (1) and the trailing region is the matching size (1 item).
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(shrinker.current_nodes.len(), 2);
}

#[test]
fn minimize_individual_choices_no_op_on_already_simplest_node() {
    let initial = vec![int_node(0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.minimize_individual_choices();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 0);
}

//! Unit tests for `Shrinker::node_program` (Step 11).
//!
//! Hypothesis references: `shrinker.py:1340-1376`, `shrinker.py:1857-1886`.

use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
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

#[test]
fn node_program_one_deletes_single_node_at_a_time() {
    // Accepting predicate: every prefix is interesting, so deletion at
    // every position should succeed.
    let initial = vec![int_node(1), int_node(2), int_node(3), int_node(4)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.node_program(1);
    // All nodes deletable → result is empty.
    assert!(shrinker.current_nodes.is_empty());
}

#[test]
fn node_program_two_deletes_pairs_adaptively() {
    // Accepting predicate; node_program(2) deletes 2 nodes at a time.
    // Starting with 6 nodes, the adaptive repeats land at 3 ⇒ all gone.
    let initial = vec![
        int_node(1),
        int_node(2),
        int_node(3),
        int_node(4),
        int_node(5),
        int_node(6),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.node_program(2);
    assert!(shrinker.current_nodes.is_empty());
}

#[test]
fn node_program_respects_predicate_rejecting_partial_deletes() {
    // Predicate accepts only sequences whose length is a multiple of 2.
    // node_program(2) keeps shape parity, so we can delete *any* number
    // of pairs.  node_program(1) can't apply because each odd-length
    // candidate is rejected.
    let initial = vec![
        int_node(1),
        int_node(2),
        int_node(3),
        int_node(4),
        int_node(5),
        int_node(6),
    ];

    // Test 1: node_program(1) — each candidate has odd length → rejected.
    {
        let mut shrinker = Shrinker::with_probe(
            Box::new(|run| match run {
                ShrinkRun::Full(nodes) => (nodes.len() % 2 == 0, nodes.to_vec(), Spans::new()),
                ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
            }),
            initial.clone(),
            Spans::new(),
        );
        shrinker.node_program(1);
        assert_eq!(shrinker.current_nodes.len(), 6);
    }

    // Test 2: node_program(2) — each candidate has even length → accepted.
    {
        let mut shrinker = Shrinker::with_probe(
            Box::new(|run| match run {
                ShrinkRun::Full(nodes) => (nodes.len() % 2 == 0, nodes.to_vec(), Spans::new()),
                ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
            }),
            initial,
            Spans::new(),
        );
        shrinker.node_program(2);
        assert_eq!(shrinker.current_nodes.len(), 0);
    }
}

#[test]
fn node_program_no_op_on_empty_or_too_long() {
    // Empty initial → nothing to do.
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        Vec::new(),
        Spans::new(),
    );
    shrinker.node_program(3);
    assert_eq!(shrinker.current_nodes.len(), 0);

    // n == 0 should also be a no-op.
    let initial = vec![int_node(1)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.node_program(0);
    assert_eq!(shrinker.current_nodes.len(), 1);
}

/// Port of Hypothesis `test_node_programs_are_adaptive`
/// (`tests/conjecture/test_shrinker.py`).  Start from 1000 false
/// booleans followed by a true: the predicate accepts iff the
/// sequence eventually reaches a true.  `node_program("X")`
/// (delete-one with adaptive `find_integer` repeats) should
/// collapse to a single `true` within a tight call budget.
#[test]
fn node_program_adaptively_deletes_long_false_run() {
    use crate::native::core::choices::BooleanChoice;
    let mut initial: Vec<ChoiceNode> = (0..1000)
        .map(|_| ChoiceNode {
            kind: ChoiceKind::Boolean(BooleanChoice),
            value: ChoiceValue::Boolean(false),
            was_forced: false,
        })
        .collect();
    initial.push(ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(true),
        was_forced: false,
    });
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                // Predicate: at least one true present.  The realised
                // sequence stops at the first true, mirroring
                // Hypothesis's draw-until-true loop.
                let mut realised: Vec<ChoiceNode> = Vec::new();
                let mut interesting = false;
                for n in nodes {
                    realised.push(n.clone());
                    if matches!(n.value, ChoiceValue::Boolean(true)) {
                        interesting = true;
                        break;
                    }
                }
                (interesting, realised, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.node_program(1);
    // The shrink target collapses to a single [true].
    assert_eq!(shrinker.current_nodes.len(), 1);
}

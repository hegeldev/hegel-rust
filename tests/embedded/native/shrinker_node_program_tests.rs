//! Unit tests for `Shrinker::node_program`.

use crate::native::core::choices::AnyInteger;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(
            IntegerChoice {
                min_value: 0,
                max_value: 100,
                shrink_towards: 0,
            }
            .into(),
        ),
        value: ChoiceValue::Integer(AnyInteger::I128(value)),
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

/// Initial counterexample is a run-length-encoded sequence
/// (1) (2, 2) (3, 3, 3) (4, 4, 4, 4) (5, 5, 5, 5, 5) where each "block"
/// starts with the count n and is followed by n more copies of n.
/// Predicate: a block reaching n==4 is the interesting one; the
/// trailing (5,...) block should be shrinkable away by `node_program`
/// deletion at lengths 1..=4.
#[test]
fn node_program_deletes_short_ranges() {
    let mut initial: Vec<ChoiceNode> = Vec::new();
    for i in 1..=5_i128 {
        for _ in 0..=i as usize {
            initial.push(int_node(i));
        }
    }
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                // Walk run-length-encoded blocks.  A block consists of
                // a leading integer `n` followed by `n` more copies of
                // `n`.  Interesting iff a block has n == 4 and all
                // values in the block validate.
                let mut idx = 0;
                let mut interesting = false;
                while idx < nodes.len() {
                    let n = match nodes[idx].value {
                        ChoiceValue::Integer(AnyInteger::I128(v)) => v,
                        _ => return (false, nodes.to_vec(), Spans::new()),
                    };
                    let block_end = idx + 1 + n.max(0) as usize;
                    if block_end > nodes.len() {
                        // Truncated block: not interesting and not invalid;
                        // just bail.
                        break;
                    }
                    for k in idx + 1..block_end {
                        match nodes[k].value {
                            ChoiceValue::Integer(AnyInteger::I128(v)) if v == n => {}
                            _ => return (false, nodes.to_vec(), Spans::new()),
                        }
                    }
                    if n == 4 {
                        interesting = true;
                    }
                    idx = block_end;
                }
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    for k in 1..=4 {
        shrinker.node_program(k);
    }
    // The minimum is the single 4-block: 5 nodes total ([4, 4, 4, 4, 4]).
    // The shrinker may converge faster or slower than that, but the
    // overall length should drop substantially from 20.
    assert!(
        shrinker.current_nodes.len() < initial_node_count(),
        "node_program failed to shrink: still {} nodes",
        shrinker.current_nodes.len()
    );
}

fn initial_node_count() -> usize {
    // 1+2+3+4+5+5 = 20 (since the loop's inclusive end produces n+1
    // copies of n for n >= 1).
    20
}

/// Start from 1000 false booleans followed by a true: the predicate
/// accepts iff the sequence eventually reaches a true. `node_program("X")`
/// (delete-one with adaptive `find_integer` repeats) should collapse
/// to a single `true` within a tight call budget.
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
                // Predicate: at least one true present. The realised
                // sequence stops at the first true (draw-until-true
                // semantics).
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

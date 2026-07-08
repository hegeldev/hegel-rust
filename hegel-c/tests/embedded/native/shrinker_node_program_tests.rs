//! Unit tests for `Shrinker::node_program`.

use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(100),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

#[test]
fn node_program_one_deletes_single_node_at_a_time() {
    let initial = vec![int_node(1), int_node(2), int_node(3), int_node(4)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.node_program(1).unwrap();
    assert!(shrinker.current_nodes.is_empty());
}

#[test]
fn node_program_two_deletes_pairs_adaptively() {
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
    shrinker.node_program(2).unwrap();
    assert!(shrinker.current_nodes.is_empty());
}

/// Deletable positions that only become deletable after the step to their
/// right lands: the leftward walk must probe the live target so accepted
/// steps compound, clearing the whole region in one invocation.
#[test]
fn node_program_left_extension_accumulates_across_accepted_steps() {
    fn values(nodes: &[ChoiceNode]) -> Vec<i128> {
        nodes
            .iter()
            .map(|n| match &n.value {
                ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
                _ => unreachable!(),
            })
            .collect()
    }
    let initial = vec![
        int_node(1),
        int_node(2),
        int_node(3),
        int_node(4),
        int_node(42),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let vals = values(nodes);
                let ok = vals.last() == Some(&42)
                    && vals[..vals.len() - 1] == [1, 2, 3, 4][..vals.len() - 1];
                (ok, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.node_program(1).unwrap();
    assert_eq!(
        values(&shrinker.current_nodes),
        vec![42],
        "one invocation should clear the whole right-to-left deletable region"
    );
}

#[test]
fn node_program_respects_predicate_rejecting_partial_deletes() {
    let initial = vec![
        int_node(1),
        int_node(2),
        int_node(3),
        int_node(4),
        int_node(5),
        int_node(6),
    ];

    {
        let mut shrinker = Shrinker::with_probe(
            Box::new(|run| match run {
                ShrinkRun::Full(nodes) => (nodes.len() % 2 == 0, nodes.to_vec(), Spans::new()),
                ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
            }),
            initial.clone(),
            Spans::new(),
        );
        shrinker.node_program(1).unwrap();
        assert_eq!(shrinker.current_nodes.len(), 6);
    }

    {
        let mut shrinker = Shrinker::with_probe(
            Box::new(|run| match run {
                ShrinkRun::Full(nodes) => (nodes.len() % 2 == 0, nodes.to_vec(), Spans::new()),
                ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
            }),
            initial,
            Spans::new(),
        );
        shrinker.node_program(2).unwrap();
        assert_eq!(shrinker.current_nodes.len(), 0);
    }
}

#[test]
fn node_program_no_op_on_empty_or_too_long() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        Vec::new(),
        Spans::new(),
    );
    shrinker.node_program(3).unwrap();
    assert_eq!(shrinker.current_nodes.len(), 0);

    let initial = vec![int_node(1)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.node_program(0).unwrap();
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
                let mut idx = 0;
                let mut interesting = false;
                while idx < nodes.len() {
                    let n = match &nodes[idx].value {
                        ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
                        _ => return (false, nodes.to_vec(), Spans::new()),
                    };
                    let block_end = idx + 1 + n.max(0) as usize;
                    if block_end > nodes.len() {
                        break;
                    }
                    for k in idx + 1..block_end {
                        match &nodes[k].value {
                            ChoiceValue::Integer(v) if i128::try_from(v).unwrap() == n => {}
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
        shrinker.node_program(k).unwrap();
    }
    assert!(
        shrinker.current_nodes.len() < initial_node_count(),
        "node_program failed to shrink: still {} nodes",
        shrinker.current_nodes.len()
    );
}

fn initial_node_count() -> usize {
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
        .map(|_| {
            ChoiceNode::new(
                ChoiceKind::Boolean(BooleanChoice),
                ChoiceValue::Boolean(false),
                false,
            )
        })
        .collect();
    initial.push(ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(true),
        false,
    ));
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
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
    shrinker.node_program(1).unwrap();
    assert_eq!(shrinker.current_nodes.len(), 1);
}

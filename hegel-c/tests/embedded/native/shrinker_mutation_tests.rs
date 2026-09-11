//! Unit tests for `mutate_and_shrink`'s divergence-gated random budget.

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn ranged_int_node(min: i128, max: i128, value: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(min),
            max_value: BigInt::from(max),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
        false,
    )
}

fn int_value(node: &ChoiceNode) -> i128 {
    match &node.value() {
        ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
        _ => unreachable!(),
    }
}

#[test]
fn mutate_invests_random_attempts_when_a_late_branch_switch_is_realised() {
    let switched_probes = Arc::new(AtomicUsize::new(0));
    let switched_c = switched_probes.clone();
    let initial = vec![
        ranged_int_node(0, 1, 1),
        ranged_int_node(0, 255, 0),
        ranged_int_node(0, 7, 3),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let switched = int_value(&nodes[0]) == 0;
                let last = if switched {
                    ranged_int_node(0, 9999, 9999)
                } else {
                    nodes[2].clone()
                };
                (
                    false,
                    vec![nodes[0].clone(), nodes[1].clone(), last],
                    Spans::new(),
                )
            }
            ShrinkRun::Probe { prefix, .. } => {
                if prefix.first() == Some(&ChoiceValue::Integer(BigInt::from(0))) {
                    switched_c.fetch_add(1, Ordering::Relaxed);
                }
                (false, Vec::new(), Spans::new())
            }
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.mutate_and_shrink()).unwrap();
    assert!(
        switched_probes.load(Ordering::Relaxed) >= 16,
        "a branch switch realised past i + 1 must receive the deep random budget, got {}",
        switched_probes.load(Ordering::Relaxed)
    );
}

#[test]
fn mutate_replays_each_shape_stable_candidate_exactly_once() {
    let probes = Arc::new(AtomicUsize::new(0));
    let probes_c = probes.clone();
    let initial = vec![ranged_int_node(0, 7, 3)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (false, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => {
                probes_c.fetch_add(1, Ordering::Relaxed);
                (false, Vec::new(), Spans::new())
            }
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.mutate_and_shrink()).unwrap();
    assert_eq!(
        probes.load(Ordering::Relaxed),
        0,
        "shape-stable candidates must get no random continuations"
    );
    assert_eq!(
        shrinker.calls, 7,
        "each of the seven candidate values gets one observing replay and no more"
    );
}

#[test]
fn mutate_observing_replay_accepts_an_interesting_smaller_run() {
    let initial = vec![ranged_int_node(0, 7, 3), ranged_int_node(0, 7, 7)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(_) => (
                true,
                vec![ranged_int_node(0, 7, 0), ranged_int_node(0, 7, 0)],
                Spans::new(),
            ),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.mutate_and_shrink()).unwrap();
    let values: Vec<i128> = shrinker.current_nodes.iter().map(int_value).collect();
    assert_eq!(values, vec![0, 0]);
}

#[test]
fn mutate_observing_replay_stops_at_the_improvement_cap() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![ranged_int_node(0, 7, 3)],
        Spans::new(),
    );
    shrinker.max_improvements = 0;
    assert!(drive_no_yield(shrinker.mutate_and_shrink()).is_err());
    assert_eq!(shrinker.calls, 0);
}

#[test]
fn mutate_observing_replay_is_a_no_op_when_stalled() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = int_value(&nodes[0]) <= 3;
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![ranged_int_node(0, 7, 5)],
        Spans::new(),
    );
    drive_no_yield(shrinker.consider(&[ranged_int_node(0, 7, 3)])).unwrap();
    shrinker.max_stall = 0;
    drive_no_yield(shrinker.mutate_and_shrink()).unwrap();
    assert_eq!(shrinker.calls, 1, "only the initial consider may have run");
}

#[test]
fn observing_replay_rejects_a_value_that_does_not_fit_the_node() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![ranged_int_node(0, 7, 3)],
        Spans::new(),
    );
    let snapshot = shrinker.current_nodes.clone();
    let diverged = drive_no_yield(shrinker.replay_observing_divergence(
        &snapshot,
        &ChoiceValue::Boolean(false),
        0,
    ))
    .unwrap();
    assert!(!diverged);
    assert_eq!(shrinker.calls, 0);
}

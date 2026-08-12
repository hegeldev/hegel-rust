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

#[test]
fn mutate_invests_random_attempts_when_a_branch_switch_is_realised() {
    let probes = Arc::new(AtomicUsize::new(0));
    let switched_probes = Arc::new(AtomicUsize::new(0));
    let probes_c = probes.clone();
    let switched_c = switched_probes.clone();
    let initial = vec![ranged_int_node(0, 1, 1), ranged_int_node(0, 7, 3)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { prefix, .. } => {
                probes_c.fetch_add(1, Ordering::Relaxed);
                let switched = prefix.first() == Some(&ChoiceValue::Integer(BigInt::from(0)));
                let second = if switched {
                    switched_c.fetch_add(1, Ordering::Relaxed);
                    ranged_int_node(0, 9999, 9999)
                } else {
                    ranged_int_node(0, 7, 7)
                };
                let branch = i128::try_from(match prefix.first() {
                    Some(ChoiceValue::Integer(v)) => v.clone(),
                    _ => BigInt::from(1),
                })
                .unwrap();
                (
                    false,
                    vec![ranged_int_node(0, 1, branch), second],
                    Spans::new(),
                )
            }
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.mutate_and_shrink()).unwrap();
    assert!(
        switched_probes.load(Ordering::Relaxed) >= 16,
        "a branch-switching candidate must receive the deep random budget, got {}",
        switched_probes.load(Ordering::Relaxed)
    );
}

#[test]
fn mutate_probes_each_shape_stable_candidate_exactly_once() {
    let probes = Arc::new(AtomicUsize::new(0));
    let probes_c = probes.clone();
    let initial = vec![ranged_int_node(0, 7, 3)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { prefix, .. } => {
                probes_c.fetch_add(1, Ordering::Relaxed);
                let value = match prefix.first() {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v.clone()).unwrap(),
                    _ => 0,
                };
                (false, vec![ranged_int_node(0, 7, value)], Spans::new())
            }
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.mutate_and_shrink()).unwrap();
    assert_eq!(
        probes.load(Ordering::Relaxed),
        7,
        "each of the seven candidate values gets one observing probe and no more"
    );
}

#[test]
fn mutate_observing_probe_accepts_an_interesting_smaller_run() {
    let initial = vec![ranged_int_node(0, 7, 3), ranged_int_node(0, 7, 7)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (
                true,
                vec![ranged_int_node(0, 7, 0), ranged_int_node(0, 7, 0)],
                Spans::new(),
            ),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.mutate_and_shrink()).unwrap();
    let values: Vec<i128> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value() {
            ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![0, 0]);
}

#[test]
fn mutate_observing_probe_stops_at_the_improvement_cap() {
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
fn mutate_observing_probe_is_a_no_op_when_stalled() {
    let probes = Arc::new(AtomicUsize::new(0));
    let probes_c = probes.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = matches!(&nodes[0].value(),
                    ChoiceValue::Integer(v) if i128::try_from(v.clone()).unwrap() <= 3);
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => {
                probes_c.fetch_add(1, Ordering::Relaxed);
                (false, Vec::new(), Spans::new())
            }
        }),
        vec![ranged_int_node(0, 7, 5)],
        Spans::new(),
    );
    drive_no_yield(shrinker.consider(&[ranged_int_node(0, 7, 3)])).unwrap();
    shrinker.max_stall = 0;
    drive_no_yield(shrinker.mutate_and_shrink()).unwrap();
    assert_eq!(probes.load(Ordering::Relaxed), 0);
}

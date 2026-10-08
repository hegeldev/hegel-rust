use super::*;
use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use alloc::boxed::Box;
use std::sync::atomic::{AtomicUsize, Ordering};

fn large_case() -> Vec<ChoiceNode> {
    (0..20)
        .map(|i| {
            ChoiceNode::integer(
                IntegerChoice {
                    min_value: BigInt::from(0),
                    max_value: BigInt::from(1000),
                    shrink_towards: BigInt::from(0),
                },
                BigInt::from(900 + i),
                false,
            )
        })
        .collect()
}

fn shrink_counting(max_calls: Option<usize>) -> (usize, Vec<ChoiceNode>, bool) {
    let executions = AtomicUsize::new(0);
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| {
            executions.fetch_add(1, Ordering::SeqCst);
            match run {
                ShrinkRun::Full(nodes) => (nodes.len() > 1, nodes.to_vec(), Spans::new()),
                ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
            }
        }),
        large_case(),
        Spans::new(),
    );
    shrinker.max_calls = max_calls;
    assert_eq!(drive_no_yield(shrinker.shrink()), Ok(()));
    let timed_out = shrinker.timed_out;
    (
        executions.load(Ordering::SeqCst),
        shrinker.current_nodes,
        timed_out,
    )
}

#[test]
fn a_call_budget_stops_the_shrink_where_it_stands() {
    let (unbounded, minimal, _) = shrink_counting(None);
    let (bounded, partial, timed_out) = shrink_counting(Some(5));
    assert!(bounded <= 5, "{bounded}");
    assert!(unbounded > bounded, "{unbounded} vs {bounded}");
    assert!(!timed_out);
    assert!(sort_key(&partial) < sort_key(&large_case()));
    assert!(sort_key(&minimal) <= sort_key(&partial));
}

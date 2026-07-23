//! Unit tests for `Shrinker::fixate_shrink_passes`.

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkPass, ShrinkRun, Shrinker};

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
fn fixate_shrink_passes_runs_passes_to_fixed_point() {
    let initial = vec![int_node(10), int_node(20)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    let mut passes = vec![ShrinkPass::new(
        "zero_choices",
        Box::new(|sh| Box::pin(sh.zero_choices())),
    )];
    drive_no_yield(shrinker.fixate_shrink_passes(&mut passes)).unwrap();
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![0, 0]);
    let stats = shrinker.pass_stats(&passes);
    assert_eq!(stats.len(), 1);
    let (_, calls, shrinks, _) = stats[0];
    assert!(calls >= 1);
    assert!(shrinks >= 1);
}

#[test]
fn fixate_shrink_passes_records_deletion_stat_when_pass_shortens() {
    let initial = vec![int_node(1); 5];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    let mut passes = vec![ShrinkPass::new(
        "delete_chunks",
        Box::new(|sh| Box::pin(sh.delete_chunks())),
    )];
    drive_no_yield(shrinker.fixate_shrink_passes(&mut passes)).unwrap();
    assert!(shrinker.current_nodes.is_empty());
    let stats = shrinker.pass_stats(&passes);
    let (_, _, _, deletions) = stats[0];
    assert!(deletions >= 1);
}

#[test]
fn consider_short_circuits_when_stalled() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                counter_clone.fetch_add(1, Ordering::Relaxed);
                let interesting = matches!(&nodes[0].value,
                    ChoiceValue::Integer(v) if i128::try_from(v).unwrap() < 5);
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5)],
        Spans::new(),
    );
    drive_no_yield(shrinker.consider(&[int_node(3)])).unwrap();
    let baseline = counter.load(Ordering::Relaxed);
    shrinker.max_stall = 10;
    shrinker.calls_at_last_shrink = shrinker.calls;
    for v in 10..60 {
        drive_no_yield(shrinker.consider(&[int_node(v)])).unwrap();
    }
    assert!(
        counter.load(Ordering::Relaxed) - baseline <= 10,
        "test_fn invoked {} times post-baseline, expected <= 10",
        counter.load(Ordering::Relaxed) - baseline
    );
}

#[test]
fn max_stall_grows_after_shrink() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let v = match &nodes[0].value {
                    ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
                    _ => unreachable!(),
                };
                (v == 1 || v == 9, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(20)],
        Spans::new(),
    );
    shrinker.max_stall = 5;
    let accepted_first = drive_no_yield(shrinker.consider(&[int_node(9)])).unwrap();
    assert!(accepted_first);
    let stall_after_first = shrinker.max_stall;
    for v in [8, 7, 6] {
        drive_no_yield(shrinker.consider(&[int_node(v)])).unwrap();
    }
    drive_no_yield(shrinker.consider(&[int_node(1)])).unwrap();
    assert!(
        shrinker.max_stall > stall_after_first,
        "max_stall failed to grow: {} -> {}",
        stall_after_first,
        shrinker.max_stall
    );
}

#[test]
fn shrink_terminates_when_stalled() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_clone = calls.clone();
    let initial = vec![int_node(5); 100];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                calls_clone.fetch_add(1, Ordering::Relaxed);
                (true, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.max_stall = 200;
    drive_no_yield(shrinker.shrink());
    assert!(
        calls.load(Ordering::Relaxed) <= 2 + 4 * shrinker.max_stall,
        "shrinker did not terminate fast enough: {} calls, max_stall {}",
        calls.load(Ordering::Relaxed),
        shrinker.max_stall
    );
}

#[test]
fn fixate_passes_does_full_run_even_when_stalled() {
    let initial: Vec<ChoiceNode> = (0..20).map(int_node).collect();
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = nodes
                    .iter()
                    .enumerate()
                    .all(|(i, n)| matches!(&n.value, ChoiceValue::Integer(v) if i128::try_from(v).unwrap() == i as i128));
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.max_stall = 5;
    let mut passes: Vec<ShrinkPass> = (1..=5)
        .map(|i| {
            ShrinkPass::new(
                "node_program",
                Box::new(move |sh| Box::pin(sh.node_program(i))),
            )
        })
        .collect();
    drive_no_yield(shrinker.fixate_shrink_passes(&mut passes)).unwrap();
    for sp in &passes {
        assert!(sp.calls > 0, "pass {} never ran", sp.name);
    }
}

#[test]
fn fixate_shrink_passes_reorders_useful_passes_to_the_front() {
    let initial = vec![int_node(5)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    let mut passes = vec![
        ShrinkPass::new("useless", Box::new(|_| Box::pin(async { Ok(()) }))),
        ShrinkPass::new(
            "useful",
            Box::new(|sh| Box::pin(sh.binary_search_integer_towards_zero())),
        ),
    ];
    drive_no_yield(shrinker.fixate_shrink_passes(&mut passes)).unwrap();
    assert_eq!(passes[0].name, "useful");
    assert_eq!(passes[1].name, "useless");
}

#[test]
fn fixate_emits_debug_per_pass_step_when_debug_set() {
    use std::sync::{Arc, Mutex};
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let log_clone = log.clone();
    let initial = vec![int_node(5)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.set_debug(move |msg| log_clone.lock().unwrap().push(msg.to_string()));
    let mut passes = vec![ShrinkPass::new(
        "binary_search_integer_towards_zero",
        Box::new(|sh| Box::pin(sh.binary_search_integer_towards_zero())),
    )];
    drive_no_yield(shrinker.fixate_shrink_passes(&mut passes)).unwrap();
    let messages = log.lock().unwrap();
    assert!(
        messages
            .iter()
            .any(|m| m == "Trying shrink pass: binary_search_integer_towards_zero"),
        "expected per-pass running message in log, got: {:?}",
        *messages
    );
}

#[test]
fn fixate_emits_no_debug_when_no_callback_set() {
    let initial = vec![int_node(5)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    let mut passes = vec![ShrinkPass::new(
        "zero_choices",
        Box::new(|sh| Box::pin(sh.zero_choices())),
    )];
    drive_no_yield(shrinker.fixate_shrink_passes(&mut passes)).unwrap();
    let v = match &shrinker.current_nodes[0].value {
        ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
        _ => unreachable!(),
    };
    assert_eq!(v, 0);
}

#[test]
fn shrink_emits_profile_report_when_debug_set() {
    use std::sync::{Arc, Mutex};
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let log_clone = log.clone();
    let initial = vec![int_node(5); 3];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.set_debug(move |msg| log_clone.lock().unwrap().push(msg.to_string()));
    drive_no_yield(shrinker.shrink());
    let messages = log.lock().unwrap();
    let combined = messages.join("\n");
    assert!(
        combined.contains("Shrink pass profiling"),
        "missing profile header. log: {}",
        combined
    );
    assert!(
        combined.contains("Useful passes:"),
        "missing useful-passes header. log: {}",
        combined
    );
    assert!(
        combined.contains("Useless passes:"),
        "missing useless-passes header. log: {}",
        combined
    );
    assert!(
        combined.contains("zero_choices"),
        "expected a zero_choices entry in the profile. log: {}",
        combined
    );
}

#[test]
fn shrink_profile_reports_singular_call_unit() {
    use std::sync::{Arc, Mutex};
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let log_clone = log.clone();
    let initial = vec![int_node(0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.set_debug(move |msg| log_clone.lock().unwrap().push(msg.to_string()));
    drive_no_yield(shrinker.shrink());
    let combined = log.lock().unwrap().join("\n");
    assert!(
        !combined.contains("1 calls"),
        "incorrect pluralization for 1 call. log: {}",
        combined
    );
    assert!(
        !combined.contains("1 choices"),
        "incorrect pluralization for 1 choice. log: {}",
        combined
    );
}

#[test]
fn shrink_stops_immediately_when_deadline_already_passed() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_clone = calls.clone();
    let initial = vec![int_node(5); 50];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                calls_clone.fetch_add(1, Ordering::Relaxed);
                (true, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.deadline = Some(Instant::now() - Duration::from_secs(1));
    drive_no_yield(shrinker.shrink());
    assert!(shrinker.timed_out, "expected the shrink to time out");
    assert_eq!(
        shrinker.calls, 0,
        "no candidate should have been considered"
    );
    assert_eq!(
        calls.load(Ordering::Relaxed),
        0,
        "the test fn must not have been invoked"
    );
    assert_eq!(
        shrinker.current_nodes.len(),
        50,
        "the example must be left unshrunk when the deadline has passed"
    );
}

#[test]
fn shrink_completes_normally_with_a_future_deadline() {
    use std::time::{Duration, Instant};
    let initial = vec![int_node(10), int_node(20)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (!nodes.is_empty(), nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.deadline = Some(Instant::now() + Duration::from_secs(300));
    drive_no_yield(shrinker.shrink());
    assert!(!shrinker.timed_out);
    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        values,
        vec![0],
        "should shrink to the minimal non-empty sequence"
    );
}

#[test]
fn consider_and_probe_stop_when_improvement_cap_reached() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5)],
        Spans::new(),
    );
    shrinker.max_improvements = 0;
    assert!(drive_no_yield(shrinker.consider(&[int_node(0)])).is_err());
    assert!(drive_no_yield(shrinker.probe(&[ChoiceValue::Integer(BigInt::from(0))], 8)).is_err());
    assert_eq!(shrinker.calls, 0, "the cap stops before any execution");
}

#[test]
fn past_deadline_latches_and_short_circuits_consider_and_probe() {
    use std::time::{Duration, Instant};
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5)],
        Spans::new(),
    );
    shrinker.deadline = Some(Instant::now() - Duration::from_secs(1));
    assert!(drive_no_yield(shrinker.consider(&[int_node(0)])).is_err());
    assert!(shrinker.timed_out);
    assert!(drive_no_yield(shrinker.consider(&[int_node(0)])).is_err());
    assert!(drive_no_yield(shrinker.probe(&[ChoiceValue::Integer(BigInt::from(0))], 8)).is_err());
    assert_eq!(shrinker.calls, 0, "nothing should have been executed");
}

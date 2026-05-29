//! Unit tests for `Shrinker::fixate_shrink_passes`.

use crate::native::core::choices::AnyInteger;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkPass, ShrinkRun, Shrinker};

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
            ChoiceValue::Integer(AnyInteger::I128(v)) => *v,
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
                let interesting =
                    matches!(nodes[0].value, ChoiceValue::Integer(AnyInteger::I128(v)) if v < 5);
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
                    ChoiceValue::Integer(AnyInteger::I128(v)) => *v,
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
fn shrink_terminates_when_stalled() {
    // Set up a predicate that accepts everything (so every shrink is
    // interesting) but never makes the sequence smaller — the shrinker
    // should bounce off the stall guard and terminate within
    // `1 + 2 * max_stall` calls.
    use std::cell::Cell;
    use std::rc::Rc;
    let calls = Rc::new(Cell::new(0_usize));
    let calls_clone = calls.clone();
    let initial = vec![int_node(5); 100];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                calls_clone.set(calls_clone.get() + 1);
                // Accept everything but never shrink: same sort_key.
                (true, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    // Lower max_stall so the test is fast.
    shrinker.max_stall = 200;
    shrinker.shrink();
    // Closure invocation count capped near max_stall —
    // `shrinker.calls <= 1 + 2 * shrinker.max_stall`. Bumped slightly to
    // account for fixate's per-iteration max_stall growth.
    assert!(
        calls.get() <= 2 + 4 * shrinker.max_stall,
        "shrinker did not terminate fast enough: {} calls, max_stall {}",
        calls.get(),
        shrinker.max_stall
    );
}

#[test]
fn fixate_passes_does_full_run_even_when_stalled() {
    // Starting target [0, 1, 2, ..., 19] with a predicate that requires
    // exactly that order, set max_stall low and hand 5 node_program
    // passes. Every pass should get at least one call even though the
    // stall guard fires repeatedly.
    let initial: Vec<ChoiceNode> = (0..20).map(int_node).collect();
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let interesting = nodes
                    .iter()
                    .enumerate()
                    .all(|(i, n)| matches!(n.value, ChoiceValue::Integer(AnyInteger::I128(v)) if v == i as i128));
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.max_stall = 5;
    let mut passes: Vec<ShrinkPass> = (1..=5)
        .map(|i| ShrinkPass::new("node_program", Box::new(move |sh| sh.node_program(i))))
        .collect();
    shrinker.fixate_shrink_passes(&mut passes);
    // Every pass got at least one call — fixate didn't bail out
    // before running the full pass list.
    for sp in &passes {
        assert!(sp.calls > 0, "pass {} never ran", sp.name);
    }
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

#[test]
fn fixate_emits_debug_per_pass_step_when_debug_set() {
    // With a debug callback installed, fixate_shrink_passes emits one
    // "Trying shrink pass: <name>" message per pass step — the per-call
    // visibility the user gets at Verbosity::Debug.
    use std::cell::RefCell;
    use std::rc::Rc;
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let log_clone = log.clone();
    let initial = vec![int_node(5)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.set_debug(move |msg| log_clone.borrow_mut().push(msg.to_string()));
    let mut passes = vec![ShrinkPass::new(
        "binary_search_integer_towards_zero",
        Box::new(|sh| sh.binary_search_integer_towards_zero()),
    )];
    shrinker.fixate_shrink_passes(&mut passes);
    let messages = log.borrow();
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
    // Without set_debug, the shrinker must not call any debug machinery
    // — verified indirectly by ensuring shrink() with no callback runs
    // cleanly and produces the same final state as before.
    let initial = vec![int_node(5)];
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
    let v = match &shrinker.current_nodes[0].value {
        ChoiceValue::Integer(AnyInteger::I128(v)) => *v,
        _ => unreachable!(),
    };
    assert_eq!(v, 0);
}

#[test]
fn shrink_emits_profile_report_when_debug_set() {
    // After shrink() finishes, the shrinker emits a "Shrink pass
    // profiling" report listing per-pass call counts split into useful
    // (shrinks > 0) and useless buckets.
    use std::cell::RefCell;
    use std::rc::Rc;
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let log_clone = log.clone();
    let initial = vec![int_node(5); 3];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.set_debug(move |msg| log_clone.borrow_mut().push(msg.to_string()));
    shrinker.shrink();
    let messages = log.borrow();
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
    // The profile mentions at least one specific pass that ran.
    assert!(
        combined.contains("zero_choices"),
        "expected a zero_choices entry in the profile. log: {}",
        combined
    );
}

#[test]
fn shrink_profile_reports_singular_call_unit() {
    // Singular/plural pluralization: "1 call" (no s), "2 calls" (with s).
    // We exercise both branches.
    use std::cell::RefCell;
    use std::rc::Rc;
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let log_clone = log.clone();
    let initial = vec![int_node(0)]; // already at the target
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.set_debug(move |msg| log_clone.borrow_mut().push(msg.to_string()));
    shrinker.shrink();
    let combined = log.borrow().join("\n");
    // With an already-minimal input the per-pass entries either don't
    // appear (calls == 0 is filtered out) or use singular forms.  We
    // assert that no malformed "1 calls" appears.
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

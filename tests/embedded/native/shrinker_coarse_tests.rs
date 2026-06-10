//! Unit tests for `Shrinker::initial_coarse_reduction`.

use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn small_int_node(value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(10),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn big_range_int_node(value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(1_000_000),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn int_value(node: &ChoiceNode) -> i128 {
    match &node.value {
        ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
        _ => unreachable!(),
    }
}

#[test]
fn initial_coarse_reduction_no_op_when_shape_stable() {
    // Predicate accepts everything, returns same shape → the zero probe
    // itself is incorporated (it is interesting and smaller, exactly as
    // in Hypothesis's reduce_each_alternative), and the expensive
    // alternative-walk is skipped: one call total.
    let initial = vec![small_int_node(5)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 0);
    assert_eq!(
        shrinker.calls, 1,
        "shape-stable node must cost exactly the zero probe"
    );
}

#[test]
fn initial_coarse_reduction_lowers_when_shape_depends_on_value() {
    // The closure returns a shape that depends on the integer value:
    // when the integer is N, the realised sequence has length N+1.
    // Zeroing changes the shape, so the coarse pass fires.
    let initial = vec![
        small_int_node(3),
        small_int_node(0),
        small_int_node(0),
        small_int_node(0),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let n = match nodes.first().map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v.clone()).unwrap() as usize,
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let actual: Vec<_> = nodes[..1 + n.min(nodes.len() - 1)].to_vec();
                (true, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    // The shape-aware coarse pass should drop the integer.
    assert!(int_value(&shrinker.current_nodes[0]) < 3);
}

#[test]
fn initial_coarse_reduction_skips_large_values() {
    // value > 10 → the heuristic skips it.
    let initial = vec![big_range_int_node(50)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    // Unchanged.
    assert_eq!(int_value(&shrinker.current_nodes[0]), 50);
}

#[test]
fn initial_coarse_reduction_skips_non_zero_min_value() {
    // Node has min_value=1; not a one_of selector pattern (those start
    // from zero).  Should be left alone.
    let mut node = small_int_node(3);
    if let ChoiceKind::Integer(ic) = std::sync::Arc::make_mut(&mut node.kind) {
        ic.min_value = BigInt::from(1);
    }
    let initial = vec![node];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 3);
}

#[test]
fn initial_coarse_reduction_skips_forced_node() {
    let mut node = small_int_node(5);
    node.was_forced = true;
    let initial = vec![node];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 5);
}

/// Initial counterexample `(1, 0)`: predicate accepts iff first integer
/// is 1. When the branch chosen by the integer doesn't change the
/// trailing shape, `initial_coarse_reduction` should leave the pair
/// untouched.
#[test]
fn initial_coarse_reduction_keeps_same_shape_one_of() {
    let initial = vec![small_int_node(1), small_int_node(0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                // Always interesting iff first value == 1.
                let interesting = nodes[0].value == ChoiceValue::Integer(BigInt::from(1));
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    // Sequence should remain (1, 0); coarse phase doesn't lower the
    // selector when there's no shape change to exploit.
    assert_eq!(shrinker.current_nodes.len(), 2);
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(int_value(&shrinker.current_nodes[1]), 0);
}

/// Coverage for the probe-accept branch in
/// `try_lower_node_as_alternative`: direct `replace` of the lowered
/// selector is rejected, but the random-continuation probe finds a
/// strictly smaller candidate.
#[test]
fn initial_coarse_reduction_accepts_probe_when_direct_replace_fails() {
    use std::cell::Cell;
    use std::rc::Rc;

    let initial = vec![
        small_int_node(3),
        small_int_node(0),
        small_int_node(0),
        small_int_node(0),
    ];
    // Track how many times the probe branch responded — used to assert
    // we actually reached it.
    let probe_calls = Rc::new(Cell::new(0_usize));
    let probe_calls_for_closure = probe_calls.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let head = match &nodes[0].value {
                    ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                if head == 3 && nodes.len() == 4 {
                    // The initial counterexample.  Interesting.
                    return (true, nodes.to_vec(), Spans::new());
                }
                if head == 0 && nodes.len() == 4 {
                    // The coarse pass's zeroing probe.  Return a
                    // different-shape realised sequence to flag
                    // shape_changed = true.
                    return (
                        false,
                        vec![small_int_node(0), small_int_node(0)],
                        Spans::new(),
                    );
                }
                // try_lower's direct `replace` lowers `head` to v in
                // 0..3.  Reject those so the probe branch runs.
                (false, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => {
                probe_calls_for_closure.set(probe_calls_for_closure.get() + 1);
                // Return a strictly shorter interesting sequence so
                // sort_key < initial_key and the probe arm at
                // `coarse.rs:96` fires.
                (
                    true,
                    vec![small_int_node(0), small_int_node(0)],
                    Spans::new(),
                )
            }
        }),
        initial,
        Spans::new(),
    );
    shrinker.initial_coarse_reduction().unwrap();
    assert!(probe_calls.get() > 0, "probe branch was never reached");
    assert_eq!(shrinker.current_nodes.len(), 2);
}

/// Drives the span-splice repair in `try_lower_node_as_alternative`
/// (Hypothesis's `try_lower_node_as_alternative` inner `for j in spans`
/// loop): the bare lowering and all three random probes lose the
/// score-gating sentinel, but splicing the probe's realised branch span
/// in front of the *preserved* original suffix repairs the test case.
#[test]
fn try_lower_node_as_alternative_splices_spans_to_repair_suffix() {
    use crate::native::core::Span;
    use crate::native::core::choices::BooleanChoice;

    fn bool_node(value: bool) -> ChoiceNode {
        ChoiceNode::new(
            ChoiceKind::Boolean(BooleanChoice),
            ChoiceValue::Boolean(value),
            false,
        )
    }
    fn sentinel_node(value: i128) -> ChoiceNode {
        ChoiceNode::new(
            ChoiceKind::Integer(IntegerChoice {
                min_value: BigInt::from(0),
                max_value: BigInt::from(1_000_000),
                shrink_towards: BigInt::from(0),
            }),
            ChoiceValue::Integer(BigInt::from(value)),
            false,
        )
    }
    fn branch_span(end: usize) -> Vec<Span> {
        vec![Span {
            start: 0,
            end,
            label: "one_of".to_string(),
            depth: 0,
            parent: None,
            discarded: false,
        }]
    }

    // Simulated test body: selector (0 => one bool, 1 => two bools), then
    // a sentinel that must be exactly 42 for the case to be interesting.
    // The realised spans always record the branch span over selector +
    // bools, mirroring a one_of generator's span.
    let realize = |selector: i128, bools: Vec<bool>, sentinel: i128| {
        let mut nodes = vec![small_int_node(selector)];
        nodes.extend(bools.iter().map(|&b| bool_node(b)));
        nodes.push(sentinel_node(sentinel));
        let spans = Spans::from(branch_span(nodes.len() - 1));
        let interesting = sentinel == 42;
        (interesting, nodes, spans)
    };
    let run_body = move |run: ShrinkRun<'_>| -> (bool, Vec<ChoiceNode>, Spans) {
        match run {
            ShrinkRun::Full(nodes) => {
                let selector = match &nodes[0].value {
                    ChoiceValue::Integer(v) => i128::try_from(v).unwrap().min(1),
                    _ => 0,
                };
                let n_bools = if selector == 1 { 2 } else { 1 };
                let mut bools = Vec::new();
                for k in 0..n_bools {
                    bools.push(matches!(
                        nodes.get(1 + k).map(|n| &n.value),
                        Some(ChoiceValue::Boolean(true))
                    ));
                }
                let sentinel = match nodes.get(1 + n_bools).map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v).unwrap(),
                    // Misaligned replay: the sentinel draw redraws as 0.
                    _ => 0,
                };
                realize(selector, bools, sentinel)
            }
            ShrinkRun::Probe { prefix, seed, .. } => {
                // Replay the prefix, then "draw randomly": the sentinel
                // never lands on 42, so no probe is interesting.
                let selector = match prefix.first() {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v).unwrap().min(1),
                    _ => 0,
                };
                let n_bools = if selector == 1 { 2 } else { 1 };
                realize(selector, vec![seed % 2 == 0; n_bools], 7 + seed as i128)
            }
        }
    };

    // Start: selector=1, two bools, sentinel=42.
    let initial = vec![
        small_int_node(1),
        bool_node(true),
        bool_node(true),
        sentinel_node(42),
    ];
    let initial_spans = Spans::from(branch_span(3));
    let mut shrinker = Shrinker::with_probe(Box::new(run_body), initial, initial_spans);
    shrinker.initial_coarse_reduction().unwrap();

    // The bare lowering [0, t, t, 42] realises a *shorter* shape whose
    // sentinel is redrawn (≠ 42), and every random probe also misses 42 —
    // only the span splice (probe's branch span + original [42] suffix)
    // can lower the selector.
    assert_eq!(
        shrinker.current_nodes[0].value,
        ChoiceValue::Integer(BigInt::from(0)),
        "selector should be lowered via the span-splice repair: {:?}",
        shrinker.current_nodes
    );
}

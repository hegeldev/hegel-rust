//! Unit tests for `Shrinker::try_shortening_via_increment`.

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{ChoiceNode, ChoiceValue, RealizedStream, Span, Spans};
use crate::native::shrinker::{ShrinkHalt, ShrinkRun, Shrinker};
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

fn selector(value: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(1),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
        false,
    )
}

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::boolean(BooleanChoice { p: 0.5 }, value, false)
}

fn clone_node(children: Vec<ChoiceNode>) -> ChoiceNode {
    ChoiceNode::clone_stream(Arc::new(RealizedStream::new(children, Vec::new())), false)
}

fn int_value(v: &ChoiceValue) -> i128 {
    match v {
        ChoiceValue::Integer(n) => i128::try_from(n.clone()).unwrap(),
        _ => unreachable!(),
    }
}

fn bool_value(v: &ChoiceValue) -> bool {
    match v {
        ChoiceValue::Boolean(b) => *b,
        _ => unreachable!(),
    }
}

fn values(nodes: &[ChoiceNode]) -> Vec<ChoiceValue> {
    nodes.iter().map(|n| n.value()).collect()
}

fn span(start: usize, end: usize) -> Span {
    Span {
        start,
        end,
        label: 1,
        depth: 0,
        parent: None,
        discarded: false,
    }
}

/// `one_of(tuple(bool, bool), bool)` where the interesting values are any
/// tuple with a `true` and the plain `true`: the selector followed by the
/// alternative's draws, as replayed from a list of values. Interesting
/// results realise the nodes the test consumed; a run that needs more
/// values than it was given realises what it consumed and is boring.
fn one_of_tuple_or_bool(choices: &[ChoiceValue]) -> (bool, Vec<ChoiceNode>) {
    let Some(first) = choices.first() else {
        return (false, Vec::new());
    };
    let branch = int_value(first);
    let mut nodes = vec![selector(branch)];
    let mut bools = choices[1..].iter().map(bool_value);
    if branch == 0 {
        let (Some(a), Some(b)) = (bools.next(), bools.next()) else {
            nodes.extend(choices[1..].iter().map(|v| bool_node(bool_value(v))));
            return (false, nodes);
        };
        nodes.push(bool_node(a));
        nodes.push(bool_node(b));
        (a || b, nodes)
    } else {
        let Some(b) = bools.next() else {
            return (false, nodes);
        };
        nodes.push(bool_node(b));
        (b, nodes)
    }
}

fn one_of_shrinker(initial: Vec<ChoiceNode>, spans: Spans) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| {
            let choices = match run {
                ShrinkRun::Full(nodes) => values(nodes),
                ShrinkRun::Probe { prefix, .. } => prefix.to_vec(),
            };
            let (interesting, nodes) = one_of_tuple_or_bool(&choices);
            (interesting, nodes, Spans::new())
        }),
        initial,
        spans,
    )
}

#[test]
fn switches_to_a_shorter_branch_by_raising_the_selector_and_dropping_a_stale_node() {
    let mut shrinker = one_of_shrinker(
        vec![selector(0), bool_node(false), bool_node(true)],
        Spans::new(),
    );
    drive_no_yield(shrinker.try_shortening_via_increment()).unwrap();
    assert_eq!(
        values(&shrinker.current_nodes),
        vec![
            ChoiceValue::Integer(BigInt::from(1)),
            ChoiceValue::Boolean(true)
        ]
    );
}

#[test]
fn deletes_a_span_after_the_raised_node_before_single_nodes() {
    let mut spans = Spans::new();
    spans.push(span(1, 2));
    let mut shrinker = one_of_shrinker(vec![selector(0), bool_node(false), bool_node(true)], spans);
    drive_no_yield(shrinker.try_shortening_via_increment()).unwrap();
    assert_eq!(shrinker.calls, 2);
    assert_eq!(shrinker.current_nodes.len(), 2);
}

#[test]
fn accepts_a_probe_that_realises_a_shorter_interesting_run() {
    let mut shrinker = one_of_shrinker(
        vec![selector(0), bool_node(true), bool_node(true)],
        Spans::new(),
    );
    drive_no_yield(shrinker.try_shortening_via_increment()).unwrap();
    assert_eq!(shrinker.calls, 1);
    assert_eq!(
        values(&shrinker.current_nodes),
        vec![
            ChoiceValue::Integer(BigInt::from(1)),
            ChoiceValue::Boolean(true)
        ]
    );
}

#[test]
fn leaves_the_target_alone_when_raising_does_not_change_the_path() {
    let initial = vec![selector(0), bool_node(false), bool_node(false)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(_) => (true, Vec::new(), Spans::new()),
            ShrinkRun::Probe { prefix, .. } => {
                let nodes: Vec<ChoiceNode> = prefix
                    .iter()
                    .map(|v| match v {
                        ChoiceValue::Integer(n) => selector(i128::try_from(n.clone()).unwrap()),
                        ChoiceValue::Boolean(b) => bool_node(*b),
                        _ => unreachable!(),
                    })
                    .collect();
                (false, nodes, Spans::new())
            }
        }),
        initial.clone(),
        Spans::new(),
    );
    drive_no_yield(shrinker.try_shortening_via_increment()).unwrap();
    assert_eq!(values(&shrinker.current_nodes), values(&initial));
    assert_eq!(shrinker.calls, 3);
}

#[test]
fn skips_nodes_without_a_dense_index_or_already_at_their_last_index() {
    let initial = vec![clone_node(vec![bool_node(false)]), bool_node(true)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|_: ShrinkRun<'_>| (true, Vec::new(), Spans::new())),
        initial.clone(),
        Spans::new(),
    );
    drive_no_yield(shrinker.try_shortening_via_increment()).unwrap();
    assert_eq!(shrinker.calls, 0);
    assert_eq!(values(&shrinker.current_nodes), values(&initial));
}

/// Interesting only once the selector is raised and the *last* boolean,
/// beyond the deletion window, is the one dropped: the raise changes the
/// path (the probe realises nothing), but no deletion within the window
/// completes it, and the raised booleans are already at their last index.
/// The span over the first boolean coincides with its single-node
/// deletion, which is therefore attempted once.
#[test]
fn only_deletes_within_the_window_after_the_raised_node() {
    let mut initial = vec![selector(0)];
    initial.extend((0..11).map(|_| bool_node(true)));
    initial.push(bool_node(false));
    let mut spans = Spans::new();
    spans.push(span(1, 2));
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let switched = nodes.len() == 12
                    && int_value(&nodes[0].value()) == 1
                    && nodes[1..].iter().all(|n| bool_value(&n.value()));
                (switched, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial.clone(),
        spans,
    );
    drive_no_yield(shrinker.try_shortening_via_increment()).unwrap();
    assert_eq!(values(&shrinker.current_nodes), values(&initial));
    assert_eq!(shrinker.calls, (1 + 8) + 1);
}

fn shrinker_after_one_improvement() -> Shrinker<'static> {
    let mut shrinker = one_of_shrinker(
        vec![selector(0), bool_node(true), bool_node(true)],
        Spans::new(),
    );
    drive_no_yield(shrinker.consider(&[selector(0), bool_node(false), bool_node(true)])).unwrap();
    assert_eq!(shrinker.improvements, 1);
    shrinker
}

#[test]
fn stops_when_the_stall_guard_gates_the_probe() {
    let mut shrinker = shrinker_after_one_improvement();
    shrinker.max_stall = 0;
    drive_no_yield(shrinker.try_shortening_via_increment()).unwrap();
    assert_eq!(shrinker.current_nodes.len(), 3);
    assert_eq!(shrinker.calls, 1);
}

#[test]
fn stops_when_the_stall_guard_gates_a_deletion_attempt() {
    let mut shrinker = shrinker_after_one_improvement();
    shrinker.max_stall = 1;
    drive_no_yield(shrinker.try_shortening_via_increment()).unwrap();
    assert_eq!(shrinker.current_nodes.len(), 3);
    assert_eq!(shrinker.calls, 2);
}

#[test]
fn propagates_the_improvement_cap() {
    let mut shrinker = one_of_shrinker(
        vec![selector(0), bool_node(false), bool_node(true)],
        Spans::new(),
    );
    shrinker.max_improvements = 0;
    let halt = drive_no_yield(shrinker.try_shortening_via_increment()).unwrap_err();
    assert_eq!(halt, ShrinkHalt::Stop);
}

fn int_node(value: i128, max: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(max),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
        false,
    )
}

/// A coin in `[0, 99]` gates a pick: while the coin is below 50 the test
/// draws the pick, then a value; from 50 up it draws the value alone. Every
/// complete input is interesting, so the two-draw shape is reachable only
/// by raising the coin past the threshold.
fn gate_then_pick(choices: &[ChoiceValue]) -> (bool, Vec<ChoiceNode>) {
    let Some(first) = choices.first() else {
        return (false, Vec::new());
    };
    let coin = int_value(first);
    let needed = if coin < 50 { 3 } else { 2 };
    let mut nodes = vec![int_node(coin, 99)];
    if choices.len() < needed {
        return (false, nodes);
    }
    if coin < 50 {
        nodes.push(int_node(int_value(&choices[1]).min(2), 2));
    }
    nodes.push(int_node(int_value(&choices[needed - 1]).min(1000), 1000));
    (true, nodes)
}

fn gate_then_pick_shrinker() -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| {
            let choices = match run {
                ShrinkRun::Full(nodes) => values(nodes),
                ShrinkRun::Probe { prefix, .. } => prefix.to_vec(),
            };
            let (interesting, nodes) = gate_then_pick(&choices);
            (interesting, nodes, Spans::new())
        }),
        vec![int_node(0, 99), int_node(0, 2), int_node(0, 1000)],
        Spans::new(),
    )
}

fn int_values(shrinker: &Shrinker<'_>) -> Vec<i128> {
    values(&shrinker.current_nodes)
        .iter()
        .map(int_value)
        .collect()
}

#[test]
fn raises_a_gate_to_its_largest_value_to_drop_the_draw_behind_it() {
    let mut shrinker = gate_then_pick_shrinker();
    drive_no_yield(shrinker.try_shortening_via_increment()).unwrap();
    assert_eq!(int_values(&shrinker), vec![99, 0]);
}

#[test]
fn shrink_lowers_a_raised_gate_to_the_threshold_that_keeps_the_shorter_shape() {
    let mut shrinker = gate_then_pick_shrinker();
    drive_no_yield(shrinker.shrink()).unwrap();
    assert_eq!(int_values(&shrinker), vec![50, 0]);
}

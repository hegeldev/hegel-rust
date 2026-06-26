//! Tests for the shrinker-level span infrastructure.
//!
//! Covers:
//! * `Shrinker::current_spans` is replaced when an improvement is accepted.
//! * `Shrinker::changed_nodes` accumulates indices whose value differs from
//!   the last `clear_change_tracking` checkpoint.
//! * `Shrinker::changed_nodes` resets when the shape (length / kind list)
//!   changes — the diff between two structures of different shapes is not
//!   well-defined.
//! * `Shrinker::clear_change_tracking` empties the set and rebaselines.

use crate::native::bignum::BigInt;
use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Span, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(i128::MIN),
            max_value: BigInt::from(i128::MAX),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(value),
        false,
    )
}

fn span(start: usize, end: usize, label: &str) -> Span {
    Span {
        start,
        end,
        label: label.to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    }
}

#[test]
fn consider_replaces_current_spans_on_improvement() {
    let initial = vec![int_node(5), int_node(5)];
    let mut initial_spans = Spans::new();
    initial_spans.push(span(0, 2, "initial"));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let mut spans = Spans::new();
                spans.push(span(0, nodes.len(), "updated"));
                (true, nodes.to_vec(), spans)
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        initial_spans,
    );
    assert_eq!(shrinker.current_spans.get(0).unwrap().label, "initial");

    let smaller = vec![int_node(0), int_node(0)];
    assert!(shrinker.consider(&smaller).unwrap());
    assert_eq!(shrinker.current_spans.len(), 1);
    assert_eq!(shrinker.current_spans.get(0).unwrap().label, "updated");
}

#[test]
fn consider_leaves_current_spans_alone_when_candidate_not_smaller() {
    let initial = vec![int_node(0)];
    let mut initial_spans = Spans::new();
    initial_spans.push(span(0, 1, "kept"));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let mut spans = Spans::new();
                spans.push(span(0, nodes.len(), "would_be_replaced"));
                (true, nodes.to_vec(), spans)
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial.clone(),
        initial_spans,
    );

    assert!(shrinker.consider(&initial).unwrap());
    assert_eq!(shrinker.current_spans.get(0).unwrap().label, "kept");

    let larger = vec![int_node(7)];
    shrinker.consider(&larger).unwrap();
    assert_eq!(shrinker.current_spans.get(0).unwrap().label, "kept");
}

#[test]
fn changed_nodes_accumulates_diff_against_checkpoint() {
    let initial = vec![int_node(10), int_node(10), int_node(10)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    assert!(shrinker.changed_nodes().is_empty());

    shrinker
        .consider(&[int_node(0), int_node(10), int_node(10)])
        .unwrap();
    assert_eq!(shrinker.changed_nodes().len(), 1);
    assert!(shrinker.changed_nodes().contains(&0));

    shrinker
        .consider(&[int_node(0), int_node(10), int_node(0)])
        .unwrap();
    let changed = shrinker.changed_nodes();
    assert!(changed.contains(&0));
    assert!(changed.contains(&2));
    assert_eq!(changed.len(), 2);
}

#[test]
fn changed_nodes_clears_on_shape_change() {
    let initial = vec![int_node(5), int_node(5), int_node(5)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );

    shrinker
        .consider(&[int_node(0), int_node(5), int_node(5)])
        .unwrap();
    assert!(!shrinker.changed_nodes().is_empty());

    shrinker.consider(&[int_node(0), int_node(0)]).unwrap();
    assert!(shrinker.changed_nodes().is_empty());
}

#[test]
fn changed_nodes_clears_on_kind_change_in_place() {
    let initial = vec![int_node(5), int_node(5)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(_) => {
                let actual = vec![int_node(0), bool_node(false)];
                (true, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.consider(&[int_node(0), int_node(0)]).unwrap();
    assert!(shrinker.changed_nodes().is_empty());
}

#[test]
fn forced_nodes_survive_every_shrinker_pass() {
    use crate::native::shrinker::{ShrinkPass, Shrinker};

    let mut forced = int_node(7);
    forced.was_forced = true;
    let initial = vec![int_node(9), forced, int_node(11)];
    let snapshot_forced_idx = 1;
    let initial_forced_value = match &initial[snapshot_forced_idx].value {
        ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
        _ => unreachable!(),
    };

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    let mut passes = vec![
        ShrinkPass::new("zero_choices", Box::new(|sh| sh.zero_choices())),
        ShrinkPass::new(
            "binary_search_integer_towards_zero",
            Box::new(|sh| sh.binary_search_integer_towards_zero()),
        ),
        ShrinkPass::new(
            "minimize_individual_choices",
            Box::new(|sh| sh.minimize_individual_choices()),
        ),
        ShrinkPass::new("shrink_duplicates", Box::new(|sh| sh.shrink_duplicates())),
    ];
    shrinker.fixate_shrink_passes(&mut passes).unwrap();
    let value = match &shrinker.current_nodes[snapshot_forced_idx].value {
        ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
        _ => unreachable!(),
    };
    assert_eq!(value, initial_forced_value);
    assert!(shrinker.current_nodes[snapshot_forced_idx].was_forced);
}

#[test]
fn clear_change_tracking_rebaselines_and_empties_set() {
    let initial = vec![int_node(10), int_node(10)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.consider(&[int_node(0), int_node(10)]).unwrap();
    assert!(shrinker.changed_nodes().contains(&0));

    shrinker.clear_change_tracking();
    assert!(shrinker.changed_nodes().is_empty());

    shrinker.consider(&[int_node(0), int_node(0)]).unwrap();
    let changed = shrinker.changed_nodes();
    assert!(changed.contains(&1));
    assert!(!changed.contains(&0));
}

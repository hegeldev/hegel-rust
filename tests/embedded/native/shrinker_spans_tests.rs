//! Tests for the shrinker-level span infrastructure introduced in Step 1.
//!
//! Covers:
//! * `Shrinker::current_spans` is replaced when an improvement is accepted.
//! * `Shrinker::changed_nodes` accumulates indices whose value differs from
//!   the last `clear_change_tracking` checkpoint, mirroring
//!   `shrinker.py:1097-1131`.
//! * `Shrinker::changed_nodes` resets when the shape (length / kind list)
//!   changes — the diff between two structures of different shapes is not
//!   well-defined.
//! * `Shrinker::clear_change_tracking` empties the set and rebaselines.

use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Span, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: i128::MIN,
            max_value: i128::MAX,
            shrink_towards: 0,
        }),
        value: ChoiceValue::Integer(value),
        was_forced: false,
    }
}

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(value),
        was_forced: false,
    }
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
    // Closure returns one fixed span for the very first accepted candidate
    // and a different one for any subsequent one, so we can assert that
    // `current_spans` tracks the most recent accepted run, not the initial
    // construction.
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

    // A smaller candidate triggers `accept_improvement`, which swaps in the
    // closure-provided spans.
    let smaller = vec![int_node(0), int_node(0)];
    assert!(shrinker.consider(&smaller));
    assert_eq!(shrinker.current_spans.len(), 1);
    assert_eq!(shrinker.current_spans.get(0).unwrap().label, "updated");
}

#[test]
fn consider_leaves_current_spans_alone_when_candidate_not_smaller() {
    // A candidate whose sort_key equals the current one returns true (lateral)
    // but doesn't go through `accept_improvement`.  `current_spans` must
    // stay at the initial state.
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

    // Same as current_nodes → fast-path returns true without calling test_fn.
    assert!(shrinker.consider(&initial));
    assert_eq!(shrinker.current_spans.get(0).unwrap().label, "kept");

    // Non-improving (same sort_key, different values would have to be
    // returned by closure — but since same sort_key, no change tracked).
    // We instead pass a strictly larger candidate to verify the not-smaller
    // path leaves spans untouched.
    let larger = vec![int_node(7)];
    shrinker.consider(&larger);
    assert_eq!(shrinker.current_spans.get(0).unwrap().label, "kept");
}

#[test]
fn changed_nodes_accumulates_diff_against_checkpoint() {
    // Each improvement diffs against `last_checkpoint_nodes` (the initial
    // value), so the set accumulates every index that has ever differed.
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

    // Shrink node 0 → set should contain {0}.
    shrinker.consider(&[int_node(0), int_node(10), int_node(10)]);
    assert_eq!(shrinker.changed_nodes().len(), 1);
    assert!(shrinker.changed_nodes().contains(&0));

    // Shrink node 2 → set should contain {0, 2}.
    shrinker.consider(&[int_node(0), int_node(10), int_node(0)]);
    let changed = shrinker.changed_nodes();
    assert!(changed.contains(&0));
    assert!(changed.contains(&2));
    assert_eq!(changed.len(), 2);
}

#[test]
fn changed_nodes_clears_on_shape_change() {
    // When a shrink changes the sequence's length, there's no stable index
    // identity between old and new, so `update_change_tracking` clears the
    // set (`shrinker.py:1120`).
    let initial = vec![int_node(5), int_node(5), int_node(5)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );

    shrinker.consider(&[int_node(0), int_node(5), int_node(5)]);
    assert!(!shrinker.changed_nodes().is_empty());

    // A two-element candidate is strictly smaller and changes the shape.
    shrinker.consider(&[int_node(0), int_node(0)]);
    assert!(shrinker.changed_nodes().is_empty());
}

#[test]
fn changed_nodes_clears_on_kind_change_in_place() {
    // Same-length but different kinds at some position is also a shape
    // change.  We mock this by returning actual nodes whose kind discriminant
    // differs from the candidate at index 1.
    let initial = vec![int_node(5), int_node(5)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(_) => {
                // Always rewrite index 1 to a Boolean kind so the shape
                // changes.
                let actual = vec![int_node(0), bool_node(false)];
                (true, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.consider(&[int_node(0), int_node(0)]);
    // Kind change → set cleared.
    assert!(shrinker.changed_nodes().is_empty());
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
    shrinker.consider(&[int_node(0), int_node(10)]);
    assert!(shrinker.changed_nodes().contains(&0));

    shrinker.clear_change_tracking();
    assert!(shrinker.changed_nodes().is_empty());

    // After clearing, the new baseline is the post-shrink state, so the
    // next diff is against `[0, 10]` rather than the original `[10, 10]`.
    shrinker.consider(&[int_node(0), int_node(0)]);
    let changed = shrinker.changed_nodes();
    assert!(changed.contains(&1));
    assert!(!changed.contains(&0));
}

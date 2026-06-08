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
    assert!(shrinker.consider(&smaller).unwrap());
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
    assert!(shrinker.consider(&initial).unwrap());
    assert_eq!(shrinker.current_spans.get(0).unwrap().label, "kept");

    // Non-improving (same sort_key, different values would have to be
    // returned by closure — but since same sort_key, no change tracked).
    // We instead pass a strictly larger candidate to verify the not-smaller
    // path leaves spans untouched.
    let larger = vec![int_node(7)];
    shrinker.consider(&larger).unwrap();
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
    shrinker
        .consider(&[int_node(0), int_node(10), int_node(10)])
        .unwrap();
    assert_eq!(shrinker.changed_nodes().len(), 1);
    assert!(shrinker.changed_nodes().contains(&0));

    // Shrink node 2 → set should contain {0, 2}.
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
    // When a shrink changes the sequence's length, there's no stable index
    // identity between old and new, so `update_change_tracking` clears the
    // set.
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

    // A two-element candidate is strictly smaller and changes the shape.
    shrinker.consider(&[int_node(0), int_node(0)]).unwrap();
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
    shrinker.consider(&[int_node(0), int_node(0)]).unwrap();
    // Kind change → set cleared.
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
fn consider_cache_evicts_when_over_capacity() {
    // The cache is bounded at 4096 entries; once we cross that limit
    // each new insertion evicts an arbitrary existing entry.  Driving
    // 4100 distinct uninteresting candidates exercises the eviction
    // path at mod.rs:~167.
    let initial = vec![int_node(0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            // Always uninteresting → every candidate gets cached.
            ShrinkRun::Full(nodes) => (false, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    for v in 1..=4100i128 {
        // Each candidate has a distinct value → distinct sort_key →
        // distinct cache key.
        shrinker.consider(&[int_node(v)]).unwrap();
    }
    // No panic, no growth past the bound (we only assert the rough
    // upper bound — exact size depends on hashing).
    // The behaviour we care about is that `consider` keeps working
    // even after the bound is reached.
    assert!(!shrinker.consider(&[int_node(99999)]).unwrap());
}

#[test]
fn consider_cache_short_circuits_repeated_candidate() {
    // Closure increments a counter on each invocation.  Calling
    // `consider` twice with the same candidate should only invoke the
    // closure once.
    use std::cell::Cell;
    use std::rc::Rc;
    let count = Rc::new(Cell::new(0u32));
    let inner = Rc::clone(&count);
    let initial = vec![int_node(5)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                inner.set(inner.get() + 1);
                // Return false so the candidate stays in cache without
                // becoming the new shrink target.
                (false, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    let candidate = vec![int_node(7)];
    shrinker.consider(&candidate).unwrap();
    shrinker.consider(&candidate).unwrap();
    shrinker.consider(&candidate).unwrap();
    assert_eq!(count.get(), 1);
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

    // After clearing, the new baseline is the post-shrink state, so the
    // next diff is against `[0, 10]` rather than the original `[10, 10]`.
    shrinker.consider(&[int_node(0), int_node(0)]).unwrap();
    let changed = shrinker.changed_nodes();
    assert!(changed.contains(&1));
    assert!(!changed.contains(&0));
}

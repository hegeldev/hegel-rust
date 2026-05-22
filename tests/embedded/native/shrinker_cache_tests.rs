//! Tests for the `consider_cache`.  Previously the cache was bounded
//! at 4096 with `HashSet::iter().next()` eviction (implementation-
//! defined).  After the audit it's unbounded — matching Hypothesis's
//! Python-dict `cached_test_function` and avoiding the seed-dependent
//! shrink-trajectory flake that any deterministic bounded-eviction
//! strategy introduced in the native runner.
//!
//! Also covers the previously-nocov defensive branches of `replace`
//! and `find_integer` so coverage is no longer escaped via annotation.

use std::collections::HashMap;

use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker, find_integer};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: i128::MIN + 1,
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

#[test]
fn replace_rejects_index_past_end_of_current_nodes() {
    // Mirrors the bind_deletion scenario: a `replace` is invoked with
    // an index that's beyond `current_nodes.len()` because an earlier
    // call inside the same callback shortened the sequence.  We hit
    // it directly here.
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5), int_node(10)],
        Spans::new(),
    );
    let mut values = HashMap::new();
    values.insert(99, ChoiceValue::Integer(0));
    assert!(!shrinker.replace(&values));
}

#[test]
fn replace_rejects_value_that_fails_kind_validate() {
    // Mirrors the one_of branch-switch scenario: the kind at position
    // i is now Boolean (after value-punning a previous shrink), but
    // the caller still tries to assign an Integer.  validate() refuses;
    // replace should return false rather than panic downstream.
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![bool_node(true)],
        Spans::new(),
    );
    let mut values = HashMap::new();
    values.insert(0, ChoiceValue::Integer(42));
    assert!(!shrinker.replace(&values));
}

#[test]
fn find_integer_bails_when_exponential_probe_overflows() {
    // Predicate that's always true makes the exponential probe walk
    // hi all the way to usize::MAX/2 and then trip the checked_mul
    // overflow guard.  This exercises the previously-nocov fallback
    // path.  The guard returns the last `lo` rather than infinitely
    // looping.
    let result = find_integer(|_| true);
    // The probe doubles `hi` from 5 upward; the final `lo` it returns
    // when checked_mul fails is the largest power-of-two-times-5 fitting
    // in usize that successfully evaluated — at least 2^60 on 64-bit
    // platforms.
    assert!(
        result >= 1 << 60,
        "result {result} should be very large; expected >= 2^60"
    );
}

#[test]
fn consider_cache_short_circuits_repeat_lookups() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let seen = Rc::new(RefCell::new(Vec::<i128>::new()));
    let seen_clone = seen.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let v = match nodes[0].value {
                    ChoiceValue::Integer(v) => v,
                    _ => unreachable!(),
                };
                seen_clone.borrow_mut().push(v);
                (false, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(0)],
        Spans::new(),
    );
    shrinker.max_stall = usize::MAX;

    // First wave: each unique value runs the closure.
    for v in 1..=200_i128 {
        shrinker.consider(&[int_node(v)]);
    }
    assert_eq!(seen.borrow().len(), 200);

    // Second wave: every lookup hits the cache.
    for v in 1..=200_i128 {
        shrinker.consider(&[int_node(v)]);
    }
    assert_eq!(seen.borrow().len(), 200, "cache short-circuit failed");
}

#[test]
fn consider_cache_distinguishes_kind_punned_candidates() {
    // Boolean(false) and Integer(0) share the same `NodeSortKey::Scalar(0,
    // false)`.  Without a kind tag in the cache key, a cache hit on the
    // boolean false would mask a kind-punned Integer(0) candidate that
    // the test_fn should still get to evaluate.
    use std::cell::RefCell;
    use std::rc::Rc;
    let seen = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let seen_clone = seen.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let tag = match nodes[0].value {
                    ChoiceValue::Boolean(_) => "bool",
                    ChoiceValue::Integer(_) => "int",
                    _ => "other",
                };
                seen_clone.borrow_mut().push(tag);
                (false, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![bool_node(true)],
        Spans::new(),
    );
    shrinker.max_stall = usize::MAX;
    shrinker.consider(&[bool_node(false)]);
    shrinker.consider(&[int_node(0)]);
    // Both should reach the closure: distinct kinds = distinct cache keys.
    assert_eq!(seen.borrow().as_slice(), &["bool", "int"]);
}

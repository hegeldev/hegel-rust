//! Tests for the `consider_cache` insertion-order eviction (Step 6 of
//! the audit cleanup).  Previously the cache used `HashSet::iter().next()`
//! to pick an eviction victim, which is implementation-defined; with
//! `VecDeque + HashSet` the oldest entry is the one dropped.
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
    // in usize that successfully evaluated.
    let expected_lo = (usize::MAX >> 1) | 1; // any usize >= 2^62 on 64-bit
    assert!(
        result >= 1 << 60,
        "result {result} should be very large; expected >= 2^60"
    );
    let _ = expected_lo;
}

#[test]
fn consider_cache_evicts_oldest_entry_first() {
    use std::cell::RefCell;
    use std::rc::Rc;

    // Track which values the closure actually saw.
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
    // Disable max_stall so cache lookups, not stall, gate calls.
    shrinker.max_stall = usize::MAX;

    // Fill the cache: 4097 distinct uninteresting candidates.  The
    // 4097th insert triggers eviction of the first.
    for v in 1..=4097_i128 {
        shrinker.consider(&[int_node(v)]);
    }
    let first_round = seen.borrow().len();
    assert_eq!(first_round, 4097);

    // Re-asking for v=2..=4097 should hit the cache and skip the
    // closure entirely.
    for v in 2..=4097_i128 {
        shrinker.consider(&[int_node(v)]);
    }
    // No new closure invocations from cached hits.
    assert_eq!(seen.borrow().len(), 4097);

    // v=1 was the first inserted; it should have been the one evicted.
    // Re-asking for v=1 should now hit the closure again.
    shrinker.consider(&[int_node(1)]);
    assert_eq!(
        seen.borrow().len(),
        4098,
        "v=1 should have been evicted; expected closure to fire again"
    );
}

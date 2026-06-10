//! Tests for the shrinker's result cache and the
//! `cached_test_function` pre-checks (Hypothesis's
//! `Shrinker.cached_test_function`). The cache is unbounded — avoiding
//! the seed-dependent shrink-trajectory flake that any deterministic
//! bounded-eviction strategy introduced in the native runner.
//!
//! Also covers the previously-nocov defensive branches of `replace`
//! and `find_integer` so coverage is no longer escaped via annotation.

use crate::native::bignum::BigInt;
use std::collections::HashMap;

use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker, find_integer};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(i128::MIN + 1),
            max_value: BigInt::from(i128::MAX),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn int_node_with(min: i128, towards: i128, value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(min),
            max_value: BigInt::from(i128::MAX),
            shrink_towards: BigInt::from(towards),
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

#[test]
fn replace_rejects_index_past_end_of_current_nodes() {
    // Reproduces the bind_deletion scenario: a `replace` is invoked with
    // an index that's beyond `current_nodes.len()` because an earlier
    // call inside the same callback shortened the sequence. We hit
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
    values.insert(99, ChoiceValue::Integer(BigInt::from(0)));
    assert!(!shrinker.replace(&values).unwrap());
}

#[test]
fn replace_rejects_value_that_fails_kind_validate() {
    // Reproduces the one_of branch-switch scenario: the kind at position
    // i is now Boolean (after value-punning a previous shrink), but
    // the caller still tries to assign an Integer. validate() refuses;
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
    values.insert(0, ChoiceValue::Integer(BigInt::from(42)));
    assert!(!shrinker.replace(&values).unwrap());
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
                let v = match &nodes[0].value {
                    ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
                    _ => unreachable!(),
                };
                seen_clone.borrow_mut().push(v);
                (false, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(1000)],
        Spans::new(),
    );
    shrinker.max_stall = usize::MAX;

    // First wave: each unique value runs the closure.
    for v in 1..=200_i128 {
        shrinker.consider(&[int_node(v)]).unwrap();
    }
    assert_eq!(seen.borrow().len(), 200);

    // Second wave: every lookup hits the cache.
    for v in 1..=200_i128 {
        shrinker.consider(&[int_node(v)]).unwrap();
    }
    assert_eq!(seen.borrow().len(), 200, "cache short-circuit failed");
}

#[test]
fn consider_reports_improvement_not_mere_interest() {
    // A candidate that is interesting but sortkey-larger than the current
    // target must report `false` — Hypothesis's `consider_new_nodes`
    // returns "did the shrink target improve", not "was it interesting".
    // Passes act on this bool; "interesting" semantics produced phantom
    // successes (e.g. shrink_ordering's local permutation).
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5)],
        Spans::new(),
    );
    assert!(!shrinker.consider(&[int_node(7)]).unwrap());
    assert_eq!(
        shrinker.current_nodes[0].value,
        ChoiceValue::Integer(BigInt::from(5)),
        "rejected candidate must not displace the target"
    );
    // A genuinely smaller candidate still reports true.
    assert!(shrinker.consider(&[int_node(3)]).unwrap());
}

#[test]
fn consider_short_circuits_candidates_equal_to_current() {
    // Re-proposing the current target is vacuously successful and must
    // not invoke the test function (Hypothesis's `startswith` pre-check).
    use std::cell::Cell;
    use std::rc::Rc;
    let count = Rc::new(Cell::new(0_usize));
    let count_clone = count.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                count_clone.set(count_clone.get() + 1);
                (false, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5), int_node(6)],
        Spans::new(),
    );
    assert!(shrinker.consider(&[int_node(5), int_node(6)]).unwrap());
    assert_eq!(count.get(), 0, "current target must not be re-executed");
}

#[test]
fn consider_rejects_larger_candidates_without_running() {
    // A candidate whose sort key exceeds the current target's cannot be
    // an improvement; Hypothesis rejects it before running the test
    // function.
    use std::cell::Cell;
    use std::rc::Rc;
    let count = Rc::new(Cell::new(0_usize));
    let count_clone = count.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                count_clone.set(count_clone.get() + 1);
                (true, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5)],
        Spans::new(),
    );
    assert!(!shrinker.consider(&[int_node(9)]).unwrap());
    assert_eq!(count.get(), 0, "larger candidate must not be executed");
}

#[test]
fn cache_distinguishes_same_sortkey_different_values() {
    // Two integer nodes under different constraints can share a sort key
    // while holding different values: value 3 at shrink_towards 0 and
    // value 13 at shrink_towards 10 both have key (3, false). The cache
    // must be keyed on the candidate's *values* (like Hypothesis's
    // engine-level cache, which is keyed on the choice sequence), not on
    // sort-key shape, or the second candidate is falsely short-circuited.
    use std::cell::RefCell;
    use std::rc::Rc;
    let seen = Rc::new(RefCell::new(Vec::<i128>::new()));
    let seen_clone = seen.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let v = match &nodes[0].value {
                    ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
                    _ => unreachable!(),
                };
                seen_clone.borrow_mut().push(v);
                (false, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(100)],
        Spans::new(),
    );
    shrinker.consider(&[int_node_with(0, 0, 3)]).unwrap();
    shrinker.consider(&[int_node_with(10, 10, 13)]).unwrap();
    assert_eq!(
        seen.borrow().as_slice(),
        &[3, 13],
        "distinct values must both reach the test function"
    );
}

#[test]
fn size_dependency_fixup_reuses_cached_result() {
    // try_replace_with_deletion re-inspects a candidate that replace()
    // already executed, to read how many nodes the test realised. That
    // second look must be served from the result cache — one execution
    // of the [3, 7] candidate total — and must bump shrinker bookkeeping
    // only for real executions.
    use std::cell::RefCell;
    use std::rc::Rc;
    let seen = Rc::new(RefCell::new(Vec::<Vec<i128>>::new()));
    let seen_clone = seen.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let vals: Vec<i128> = nodes
                    .iter()
                    .map(|n| match &n.value {
                        ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
                        _ => unreachable!(),
                    })
                    .collect();
                seen_clone.borrow_mut().push(vals);
                // Only the exact original sequence is interesting.
                let interesting = matches!(run, ShrinkRun::Full(n) if {
                    matches!(&n[0].value, ChoiceValue::Integer(v) if i128::try_from(v).unwrap() == 5)
                });
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5), int_node(7)],
        Spans::new(),
    );
    let improved = shrinker
        .try_replace_with_deletion(0, ChoiceValue::Integer(BigInt::from(3)), 2)
        .unwrap();
    assert!(!improved);
    let executions = seen
        .borrow()
        .iter()
        .filter(|vals| vals.as_slice() == [3, 7])
        .count();
    assert_eq!(
        executions, 1,
        "the [3, 7] candidate must be executed once and then served from cache"
    );
}

#[test]
fn consider_cache_distinguishes_kind_punned_candidates() {
    // Boolean(false) and Integer(0) share the same `Scalar(0, false)` sort
    // key.  Without a kind tag in the cache key, a cache hit on the
    // boolean false would mask a kind-punned Integer(0) candidate that
    // the test_fn should still get to evaluate.
    use std::cell::RefCell;
    use std::rc::Rc;
    let seen = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let seen_clone = seen.clone();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let tag = match &nodes[0].value {
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
    shrinker.consider(&[bool_node(false)]).unwrap();
    shrinker.consider(&[int_node(0)]).unwrap();
    // Both should reach the closure: distinct kinds = distinct cache keys.
    assert_eq!(seen.borrow().as_slice(), &["bool", "int"]);
}

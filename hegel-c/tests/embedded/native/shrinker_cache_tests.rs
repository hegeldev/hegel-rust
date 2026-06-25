//! Covers the previously-nocov defensive branches of `replace` and
//! `find_integer` so coverage is no longer escaped via annotation.
//!
//! (The shrinker no longer has its own negative-result cache: repeated
//! candidates are deduped by the engine's data cache and choice tree behind
//! the test closure, the single source of truth, matching Hypothesis's
//! `Shrinker.cached_test_function`.)

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

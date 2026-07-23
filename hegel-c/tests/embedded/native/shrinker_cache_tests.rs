//! Covers the previously-nocov defensive branches of `replace` and
//! `FindInteger` so coverage is no longer escaped via annotation.
//!
//! (The shrinker no longer has its own negative-result cache: repeated
//! candidates are deduped by the engine's data cache and choice tree behind
//! the test closure, the single source of truth, matching Hypothesis's
//! `Shrinker.cached_test_function`.)

use crate::native::bignum::BigInt;
use std::collections::HashMap;

use crate::exchange::drive_no_yield;
use crate::native::core::choices::{BooleanChoice, IntegerChoice};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::search::FindInteger;
use crate::native::shrinker::{ShrinkRun, Shrinker};

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
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![int_node(5), int_node(10)],
        Spans::new(),
    );
    let mut values = HashMap::new();
    values.insert(99, ChoiceValue::Integer(BigInt::from(0)));
    assert!(!drive_no_yield(shrinker.replace(&values)).unwrap());
}

#[test]
fn replace_rejects_value_that_fails_kind_validate() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![bool_node(true)],
        Spans::new(),
    );
    let mut values = HashMap::new();
    values.insert(0, ChoiceValue::Integer(BigInt::from(42)));
    assert!(!drive_no_yield(shrinker.replace(&values)).unwrap());
}

#[test]
fn find_integer_bails_when_exponential_probe_overflows() {
    let mut search = FindInteger::new();
    while search.probe().is_some() {
        search.record(true);
    }
    let result = search.result();
    assert!(
        result >= 1 << 60,
        "result {result} should be very large; expected >= 2^60"
    );
}

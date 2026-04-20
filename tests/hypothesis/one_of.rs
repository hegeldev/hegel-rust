//! Ported from hypothesis-python/tests/cover/test_one_of.py
//!
//! Omitted (Python-specific, no Rust counterpart):
//! - test_one_of_single_strategy_is_noop: Python `is` identity check
//! - test_one_of_without_strategies_suggests_sampled_from: Python dynamic typing error
//! - test_one_of_unwrapping: Python `repr()` output

use crate::common::utils::{assert_all_examples, expect_panic};
use hegel::generators::{self as gs, Generator};

#[test]
fn test_one_of_empty() {
    expect_panic(
        || {
            gs::one_of::<i64, _>(vec![]);
        },
        "one_of requires at least one generator",
    );
}

#[test]
fn test_one_of_filtered() {
    assert_all_examples(
        gs::one_of(vec![gs::integers::<i64>().filter(|i| *i != 0).boxed()]),
        |i: &i64| *i != 0,
    );
}

#[test]
fn test_one_of_flatmapped() {
    assert_all_examples(
        gs::one_of(vec![
            gs::just(100i64)
                .flat_map(|n| gs::integers::<i64>().min_value(n))
                .boxed(),
        ]),
        |i: &i64| *i >= 100,
    );
}

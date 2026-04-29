//! Ported from hypothesis-python/tests/nocover/test_sets.py

use std::collections::HashSet;

use crate::common::utils::{assert_all_examples, find_any};
use hegel::generators::{self as gs, Generator};

#[test]
fn test_can_draw_sets_of_hard_to_find_elements() {
    let rarebool = gs::floats::<f64>()
        .min_value(0.0)
        .max_value(1.0)
        .map(|x: f64| x <= 0.05);
    find_any(
        gs::hashsets(rarebool).min_size(2),
        |s: &HashSet<bool>| s.len() >= 2,
    );
}

#[test]
fn test_empty_sets() {
    assert_all_examples(
        gs::hashsets(gs::integers::<i64>()).max_size(0),
        |s: &HashSet<i64>| s.is_empty(),
    );
}

#[test]
fn test_bounded_size_sets() {
    assert_all_examples(
        gs::hashsets(gs::integers::<i64>()).max_size(2),
        |s: &HashSet<i64>| s.len() <= 2,
    );
}

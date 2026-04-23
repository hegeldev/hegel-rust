//! Ported from hypothesis-python/tests/nocover/test_collective_minimization.py.
//!
//! The upstream file parametrizes a single test body over
//! `standard_types`, a heterogeneous Python list of strategies. Rust is
//! strongly typed, so we cannot iterate over a mixed-type list of
//! generators; instead each concrete strategy from `standard_types`
//! becomes its own `#[test]`. The assertion is the same in each case:
//! force a list of 10 values with at least 2 distinct debug reprs,
//! shrink, and verify shrinking collapses to 2-3 distinct values.
//!
//! Strategies from `standard_types` that can only ever produce a single
//! value (`lists(none(), max_size=0)`, `tuples()`, `just("a")`,
//! `fixed_dictionaries({})`) are omitted: the predicate is vacuously
//! unsatisfiable, so the Python port reaches `except Unsatisfiable: pass`
//! and the case carries no signal.

use std::collections::HashSet;
use std::fmt::Debug;

use hegel::generators::{self as gs, Generator};

use crate::common::utils::Minimal;

fn check_collective_minimization<T, G>(spec: G)
where
    G: Generator<T> + 'static,
    T: Send + Debug + 'static,
{
    let n = 10;
    let xs = Minimal::new(
        gs::vecs(spec).min_size(n).max_size(n),
        |x: &Vec<T>| {
            x.iter()
                .map(|v| format!("{v:?}"))
                .collect::<HashSet<_>>()
                .len()
                >= 2
        },
    )
    .test_cases(2000)
    .run();
    assert_eq!(xs.len(), n);
    let distinct: HashSet<String> = xs.iter().map(|v| format!("{v:?}")).collect();
    assert!(
        (2..=3).contains(&distinct.len()),
        "expected 2..=3 distinct values after shrinking, got {} ({xs:?})",
        distinct.len(),
    );
}

#[test]
fn test_can_collectively_minimize_booleans() {
    check_collective_minimization(gs::booleans());
}

#[test]
fn test_can_collectively_minimize_abc_booleans() {
    check_collective_minimization(gs::tuples!(
        gs::booleans(),
        gs::booleans(),
        gs::booleans()
    ));
}

#[test]
fn test_can_collectively_minimize_abc_bool_bool_int() {
    check_collective_minimization(gs::tuples!(
        gs::booleans(),
        gs::booleans(),
        gs::integers::<i64>()
    ));
}

#[test]
fn test_can_collectively_minimize_integers() {
    check_collective_minimization(gs::integers::<i64>());
}

#[test]
fn test_can_collectively_minimize_integers_min_3() {
    check_collective_minimization(gs::integers::<i64>().min_value(3));
}

#[test]
fn test_can_collectively_minimize_floats() {
    check_collective_minimization(gs::floats::<f64>());
}

#[test]
fn test_can_collectively_minimize_floats_bounded() {
    check_collective_minimization(gs::floats::<f64>().min_value(-2.0).max_value(3.0));
}

#[test]
fn test_can_collectively_minimize_floats_min_neg_2() {
    check_collective_minimization(gs::floats::<f64>().min_value(-2.0));
}

#[test]
fn test_can_collectively_minimize_floats_max_neg_zero() {
    check_collective_minimization(gs::floats::<f64>().max_value(-0.0));
}

#[test]
fn test_can_collectively_minimize_floats_min_zero() {
    check_collective_minimization(gs::floats::<f64>().min_value(0.0));
}

#[test]
fn test_can_collectively_minimize_text() {
    check_collective_minimization(gs::text());
}

#[test]
fn test_can_collectively_minimize_binary() {
    check_collective_minimization(gs::binary());
}

#[test]
fn test_can_collectively_minimize_tuples_bool_bool() {
    check_collective_minimization(gs::tuples!(gs::booleans(), gs::booleans()));
}

#[test]
fn test_can_collectively_minimize_sampled_from_range_10() {
    check_collective_minimization(gs::sampled_from((0..10).collect::<Vec<i64>>()));
}

#[test]
fn test_can_collectively_minimize_sampled_from_abc() {
    check_collective_minimization(gs::sampled_from(vec!["a", "b", "c"]));
}

#[test]
fn test_can_collectively_minimize_lists_of_booleans() {
    check_collective_minimization(gs::vecs(gs::booleans()));
}

#[test]
fn test_can_collectively_minimize_lists_of_lists_of_booleans() {
    check_collective_minimization(gs::vecs(gs::vecs(gs::booleans())));
}

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
//! Omissions from `standard_types`, by reason:
//!
//! * Predicate is vacuously unsatisfiable (all examples share one repr).
//!   The Python test catches `Unsatisfiable`; Rust's `Minimal` panics if
//!   nothing is ever found. Dropped entries:
//!   `lists(none(), max_size=0)`, `tuples()`, `sets(none(), max_size=0)`,
//!   `frozensets(none(), max_size=0)`, `fixed_dictionaries({})`,
//!   `floats(min_value=3.14, max_value=3.14)`,
//!   `lists(floats(0.0, 0.0))`, `none()`,
//!   `integers().flatmap(lambda v: lists(just(v)))`.
//!
//! * Debug format is not deterministic across equal values. `HashMap`
//!   and `HashSet` iterate in a seed-dependent order, so two equal maps
//!   can print differently and the "≤ 3 distinct reprs" assertion would
//!   fail spuriously. Dropped:
//!   `dictionaries(booleans(), integers())`,
//!   `dictionaries(text(), booleans())`,
//!   `frozensets(integers())`, `sets(frozensets(booleans()))`.
//!
//! * No hegel-rust counterpart: `complex_numbers()`, `fractions()`,
//!   `decimals()`, `recursive(...)`, and
//!   `booleans().flatmap(lambda x: booleans() if x else complex_numbers())`
//!   (depends on `complex_numbers()`). `randoms(use_true_random=True)`
//!   is also omitted — `HegelRandom`'s `Debug` would not produce stable
//!   repr counts.

use std::collections::HashSet;
use std::fmt::Debug;

use hegel::generators::{self as gs, Generator};

use crate::common::utils::Minimal;

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum IntOrBoolTuple {
    Int(i64),
    BoolTuple((bool,)),
}

fn check_collective_minimization<T, G>(spec: G)
where
    G: Generator<T> + 'static,
    T: Send + Debug + 'static,
{
    let n = 10;
    let xs = Minimal::new(gs::vecs(spec).min_size(n).max_size(n), |x: &Vec<T>| {
        x.iter()
            .map(|v| format!("{v:?}"))
            .collect::<HashSet<_>>()
            .len()
            >= 2
    })
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
    check_collective_minimization(gs::tuples!(gs::booleans(), gs::booleans(), gs::booleans()));
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
fn test_can_collectively_minimize_fixed_dict_int_bool() {
    check_collective_minimization(
        gs::fixed_dicts()
            .field("a", gs::integers::<i64>())
            .field("b", gs::booleans())
            .build(),
    );
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
fn test_can_collectively_minimize_integers_wide_range() {
    check_collective_minimization(
        gs::integers::<i128>()
            .min_value(-(1i128 << 32))
            .max_value(1i128 << 64),
    );
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
fn test_can_collectively_minimize_floats_full_range() {
    check_collective_minimization(gs::floats::<f64>().min_value(-f64::MAX).max_value(f64::MAX));
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

#[test]
fn test_can_collectively_minimize_one_of_int_or_bool_tuple() {
    check_collective_minimization(gs::one_of(vec![
        gs::integers::<i64>().map(IntOrBoolTuple::Int).boxed(),
        gs::tuples!(gs::booleans())
            .map(IntOrBoolTuple::BoolTuple)
            .boxed(),
    ]));
}

#[test]
fn test_can_collectively_minimize_one_of_strings() {
    check_collective_minimization(gs::one_of(vec![
        gs::just("a".to_string()).boxed(),
        gs::just("b".to_string()).boxed(),
        gs::just("c".to_string()).boxed(),
    ]));
}

#[test]
fn test_can_collectively_minimize_flatmap_ordered_pair() {
    check_collective_minimization(gs::integers::<i64>().flat_map(|right| {
        gs::integers::<i64>()
            .min_value(0)
            .map(move |length| (right.wrapping_sub(length), right))
    }));
}

#[test]
fn test_can_collectively_minimize_filter_large_abs() {
    check_collective_minimization(gs::integers::<i64>().filter(|x: &i64| *x > 100 || *x < -100));
}

use crate::common::utils::{Minimal, minimal};
use hegel::generators::{self as gs, Generator};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Tagged {
    Int(i64),
    Text(String),
    Bool(bool),
}

#[test]
fn test_minimize_one_of_integers() {
    for _ in 0..10 {
        let result = minimal(
            gs::one_of(vec![
                gs::integers::<i64>().boxed(),
                gs::integers::<i64>().min_value(100).max_value(200).boxed(),
            ]),
            |_: &i64| true,
        );
        assert_eq!(result, 0);
    }
}

#[test]
fn test_minimize_one_of_mixed() {
    for _ in 0..10 {
        let result = minimal(
            gs::one_of(vec![
                gs::integers::<i64>().map(Tagged::Int).boxed(),
                gs::text().map(Tagged::Text).boxed(),
                gs::booleans().map(Tagged::Bool).boxed(),
            ]),
            |_: &Tagged| true,
        );
        assert!(
            result == Tagged::Int(0)
                || result == Tagged::Text(String::new())
                || result == Tagged::Bool(false)
        );
    }
}

#[test]
fn test_minimize_mixed_list() {
    let result = minimal(
        gs::vecs(gs::one_of(vec![
            gs::integers::<i64>().map(Tagged::Int).boxed(),
            gs::text().map(Tagged::Text).boxed(),
        ])),
        |x: &Vec<Tagged>| x.len() >= 10,
    );
    assert_eq!(result.len(), 10);
    for item in &result {
        assert!(*item == Tagged::Int(0) || *item == Tagged::Text(String::new()),);
    }
}

#[test]
fn test_mixed_list_flatmap() {
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum BoolOrText {
        Bool(bool),
        Text(String),
    }

    let bool_or_text = hegel::compose!(|tc| {
        let b: bool = tc.draw(gs::booleans());
        if b {
            BoolOrText::Bool(tc.draw(gs::booleans()))
        } else {
            BoolOrText::Text(tc.draw(gs::text()))
        }
    });

    let result = Minimal::new(gs::vecs(bool_or_text), |ls: &Vec<BoolOrText>| {
        let bools = ls
            .iter()
            .filter(|x| matches!(x, BoolOrText::Bool(_)))
            .count();
        let texts = ls
            .iter()
            .filter(|x| matches!(x, BoolOrText::Text(_)))
            .count();
        bools >= 3 && texts >= 3
    })
    .test_cases(10000)
    .run();
    assert_eq!(result.len(), 6);
    let as_set: std::collections::HashSet<_> = result.into_iter().collect();
    assert_eq!(
        as_set,
        std::collections::HashSet::from(
            [BoolOrText::Bool(false), BoolOrText::Text(String::new()),]
        )
    );
}

#[test]
fn test_one_of_slip() {
    let result = minimal(
        gs::one_of(vec![
            gs::integers::<i64>().min_value(101).max_value(200).boxed(),
            gs::integers::<i64>().min_value(0).max_value(100).boxed(),
        ]),
        |_: &i64| true,
    );
    assert_eq!(result, 101);
}

/// Asserts a tight joint minimum for the sum-style predicate
/// `i + f >= 100 && i != 1 && f != 1.0`. With those exclusions both sides
/// are forced above their individual shrink targets, so the only way to
/// reach the joint minimum is via `redistribute_numeric_pairs` walking both
/// sides together.
fn assert_tight_joint_minimum(i: i64, f: f64) {
    assert!(
        i != 1,
        "integer at its shrink target — joint walk didn't fire"
    );
    assert!(
        f != 1.0,
        "float at its shrink target — joint walk didn't fire"
    );
    assert!(
        (i as f64) + f >= 100.0,
        "predicate fails for (i={i}, f={f})"
    );
    assert!(
        (i as f64) + f < 101.0,
        "joint sum {:.3} is more than one unit slack — pair didn't tighten",
        (i as f64) + f
    );
}

fn pair_predicate(i: i64, f: f64) -> bool {
    (i as f64) + f >= 100.0 && i != 1 && f != 1.0
}

#[test]
fn test_redistribute_int_float_pair() {
    let (i, f) = minimal(
        hegel::tuples!(
            gs::integers::<i64>().min_value(1).max_value(10_000),
            gs::floats::<f64>()
                .min_value(0.5)
                .max_value(1000.0)
                .allow_nan(false)
                .allow_infinity(false),
        ),
        |&(i, f): &(i64, f64)| pair_predicate(i, f),
    );
    assert_tight_joint_minimum(i, f);
}

#[test]
fn test_redistribute_float_int_pair() {
    let (f, i) = minimal(
        hegel::tuples!(
            gs::floats::<f64>()
                .min_value(0.5)
                .max_value(1000.0)
                .allow_nan(false)
                .allow_infinity(false),
            gs::integers::<i64>().min_value(1).max_value(10_000),
        ),
        |&(f, i): &(f64, i64)| pair_predicate(i, f),
    );
    assert_tight_joint_minimum(i, f);
}

#[test]
fn test_redistribute_pair_with_boolean_in_sequence() {
    let (b, i, f) = minimal(
        hegel::tuples!(
            gs::booleans(),
            gs::integers::<i64>().min_value(1).max_value(10_000),
            gs::floats::<f64>()
                .min_value(0.5)
                .max_value(1000.0)
                .allow_nan(false)
                .allow_infinity(false),
        ),
        |&(_, i, f): &(bool, i64, f64)| pair_predicate(i, f),
    );
    assert!(!b);
    assert_tight_joint_minimum(i, f);
}

#[test]
fn test_sum_of_pair_float() {
    let (a, b) = minimal(
        hegel::tuples!(
            gs::floats::<f64>()
                .min_value(0.0)
                .max_value(1000.0)
                .allow_nan(false)
                .allow_infinity(false),
            gs::floats::<f64>()
                .min_value(0.0)
                .max_value(1000.0)
                .allow_nan(false)
                .allow_infinity(false),
        ),
        |&(a, b): &(f64, f64)| a + b > 1000.0,
    );
    assert_eq!((a, b), (1.0, 1000.0));
}

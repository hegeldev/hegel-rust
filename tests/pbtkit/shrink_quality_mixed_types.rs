//! Ported from resources/pbtkit/tests/shrink_quality/test_mixed_types.py

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
        assert!(
            *item == Tagged::Int(0) || *item == Tagged::Text(String::new()),
        );
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
        let bools = ls.iter().filter(|x| matches!(x, BoolOrText::Bool(_))).count();
        let texts = ls.iter().filter(|x| matches!(x, BoolOrText::Text(_))).count();
        bools >= 3 && texts >= 3
    })
    .test_cases(10000)
    .run();
    assert_eq!(result.len(), 6);
    let as_set: std::collections::HashSet<_> = result.into_iter().collect();
    assert_eq!(
        as_set,
        std::collections::HashSet::from([
            BoolOrText::Bool(false),
            BoolOrText::Text(String::new()),
        ])
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

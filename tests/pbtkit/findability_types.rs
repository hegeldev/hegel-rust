//! Ported from resources/pbtkit/tests/findability/test_types.py
//!
//! These verify the engine can find structural counterexamples — type
//! mismatches, unsorted lists, non-ASCII strings, duplicate characters, etc.

use std::collections::HashSet;
use std::mem::discriminant;

use crate::common::utils::expect_panic;
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum FloatOrBool {
    Float(f64),
    Bool(bool),
}

#[test]
fn test_one_of_produces_different_types() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x = tc.draw(gs::one_of(vec![
                    gs::floats::<f64>().map(FloatOrBool::Float).boxed(),
                    gs::booleans().map(FloatOrBool::Bool).boxed(),
                ]));
                let y = tc.draw(gs::one_of(vec![
                    gs::floats::<f64>().map(FloatOrBool::Float).boxed(),
                    gs::booleans().map(FloatOrBool::Bool).boxed(),
                ]));
                assert!(discriminant(&x) == discriminant(&y));
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_list_is_not_always_sorted() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let xs: Vec<i64> =
                    tc.draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(100)));
                let mut sorted = xs.clone();
                sorted.sort();
                assert!(sorted == xs);
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_float_is_not_always_an_endpoint() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x: f64 = tc.draw(
                    gs::floats::<f64>()
                        .min_value(1.0)
                        .max_value(2.0)
                        .allow_nan(false),
                );
                assert!(x == 1.0 || x == 2.0);
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_can_find_string_with_duplicate_characters() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let s: String = tc.draw(gs::text().min_size(2));
                let unique: HashSet<char> = s.chars().collect();
                assert!(unique.len() == s.chars().count());
            })
            .settings(Settings::new().test_cases(200).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_can_find_non_ascii_text() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x: String = tc.draw(gs::text());
                assert!(x.is_ascii());
            })
            .settings(Settings::new().test_cases(200).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_removing_element_from_non_unique_list() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let mut xs: Vec<i64> = tc.draw(
                    gs::vecs(gs::integers::<i64>().min_value(0).max_value(10)).min_size(2),
                );
                let y: i64 = tc.draw(gs::sampled_from(xs.clone()));
                let pos = xs.iter().position(|&v| v == y).unwrap();
                xs.remove(pos);
                assert!(!xs.contains(&y));
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "Property test failed",
    );
}

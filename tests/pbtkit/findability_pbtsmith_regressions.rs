//! Ported from resources/pbtkit/tests/findability/test_pbtsmith_regressions.py

use std::collections::HashSet;

use crate::common::utils::expect_panic;
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};

#[test]
fn test_zero_from_wide_integer_range() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let v0: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(8191));
                assert!(v0 > 0);
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_duplicate_tuples_in_list() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let v0: Vec<(i64, i64)> = tc.draw(
                    gs::vecs(gs::tuples!(
                        gs::integers::<i64>().min_value(0).max_value(184),
                        gs::integers::<i64>().min_value(0).max_value(184)
                    ))
                    .max_size(10),
                );
                let unique: HashSet<_> = v0.iter().collect();
                assert!(v0.len() == unique.len());
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_non_negative_float_is_not_always_positive() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let v0: f64 = tc.draw(
                    gs::floats::<f64>()
                        .allow_nan(false)
                        .allow_infinity(false)
                        .filter(|x: &f64| *x >= 0.0),
                );
                assert!(v0 > 0.0);
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
#[ignore = "conjunction of empty bytes + zero is hard to find reliably (xfail upstream)"]
fn test_empty_bytes_with_wide_dependent_range() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                tc.draw(gs::just(false));
                let v1: Vec<u8> = tc.draw(gs::binary().max_size(20));
                tc.draw(gs::binary().max_size(20));
                let v3: i64 = tc.draw(gs::booleans().map(|x| x as i64));
                let v4: i64 = tc.draw(gs::integers::<i64>().min_value(v3).max_value(v3 + 39));
                if !v1.is_empty() {
                    tc.draw(gs::booleans());
                } else {
                    assert!(v3 + v4 > 0);
                }
            })
            .settings(Settings::new().test_cases(5000).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
#[ignore = "conjunction of empty bytes + zero depends on random seed (xfail upstream)"]
fn test_empty_bytes_with_dependent_condition() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                tc.draw(gs::just(false));
                let v1: Vec<u8> = tc.draw(gs::binary().max_size(20));
                tc.draw(gs::binary().max_size(20));
                let v3: i64 = tc.draw(gs::booleans().map(|x| x as i64));
                let v4: i64 = tc.draw(gs::integers::<i64>().min_value(v3).max_value(v3 + 2));
                if !v1.is_empty() {
                    tc.draw(gs::booleans());
                } else {
                    assert!(v3 + v4 > 0);
                }
            })
            .settings(Settings::new().test_cases(5000).database(None))
            .run();
        },
        "Property test failed",
    );
}

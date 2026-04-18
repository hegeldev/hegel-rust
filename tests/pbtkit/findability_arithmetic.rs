//! Ported from resources/pbtkit/tests/findability/test_arithmetic.py.
//!
//! These tests verify the engine can find counterexamples to false
//! arithmetic properties, and does not spuriously fail on true ones.
//!
//! Note: the Python originals use arbitrary-precision integers so they can
//! freely draw from the full `i64` range and add values without worrying
//! about overflow. In Rust, `i64 + i64` panics on overflow in debug mode, so
//! the true-property tests use `wrapping_add` — wrapping arithmetic over
//! `i64` is still a commutative group, so associativity and commutativity
//! still hold.

use crate::common::utils::expect_panic;
use hegel::generators as gs;
use hegel::{Hegel, Settings};

#[test]
fn test_float_addition_is_not_associative() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x: f64 = tc.draw(gs::floats::<f64>());
                let y: f64 = tc.draw(gs::floats::<f64>());
                let z: f64 = tc.draw(gs::floats::<f64>());
                assert!(x + (y + z) == (x + y) + z);
            })
            .settings(Settings::new().test_cases(2000).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_float_addition_does_not_cancel() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x: f64 = tc.draw(gs::floats::<f64>().min_value(-1e100).max_value(1e100));
                let y: f64 = tc.draw(gs::floats::<f64>().min_value(-1e100).max_value(1e100));
                assert!(x + (y - x) == y);
            })
            .settings(Settings::new().test_cases(2000).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_string_addition_is_not_commutative() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x: String = tc.draw(gs::text().min_size(1));
                let y: String = tc.draw(gs::text().min_size(1));
                assert!(format!("{x}{y}") == format!("{y}{x}"));
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_bytes_addition_is_not_commutative() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x: Vec<u8> = tc.draw(gs::binary().min_size(1));
                let y: Vec<u8> = tc.draw(gs::binary().min_size(1));
                let xy: Vec<u8> = x.iter().chain(y.iter()).copied().collect();
                let yx: Vec<u8> = y.iter().chain(x.iter()).copied().collect();
                assert!(xy == yx);
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_integer_bound_can_be_exceeded() {
    for t in [1i64, 10, 100, 1000] {
        expect_panic(
            || {
                Hegel::new(move |tc| {
                    let x: i64 = tc.draw(gs::integers::<i64>());
                    assert!(x < t);
                })
                .settings(Settings::new().test_cases(10000).database(None))
                .run();
            },
            "Property test failed",
        );
    }
}

#[test]
fn test_int_is_not_always_negative() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x: i64 = tc.draw(gs::integers::<i64>());
                assert!(x < 0);
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_int_addition_is_commutative() {
    Hegel::new(|tc| {
        let x: i64 = tc.draw(gs::integers::<i64>());
        let y: i64 = tc.draw(gs::integers::<i64>());
        assert_eq!(x.wrapping_add(y), y.wrapping_add(x));
    })
    .settings(Settings::new().database(None))
    .run();
}

#[test]
fn test_int_addition_is_associative() {
    Hegel::new(|tc| {
        let x: i64 = tc.draw(gs::integers::<i64>());
        let y: i64 = tc.draw(gs::integers::<i64>());
        let z: i64 = tc.draw(gs::integers::<i64>());
        assert_eq!(
            x.wrapping_add(y.wrapping_add(z)),
            x.wrapping_add(y).wrapping_add(z),
        );
    })
    .settings(Settings::new().database(None))
    .run();
}

#[test]
fn test_reversing_preserves_integer_addition() {
    Hegel::new(|tc| {
        let xs: Vec<i64> = tc.draw(gs::vecs(gs::integers::<i64>()));
        let forward = xs.iter().copied().fold(0i64, i64::wrapping_add);
        let backward = xs.iter().rev().copied().fold(0i64, i64::wrapping_add);
        assert_eq!(forward, backward);
    })
    .settings(Settings::new().database(None))
    .run();
}

#[test]
fn test_integer_division_preserves_order() {
    Hegel::new(|tc| {
        let n: i64 = tc.draw(gs::integers::<i64>().min_value(1));
        assert!(n / 2 < n);
    })
    .settings(Settings::new().database(None))
    .run();
}

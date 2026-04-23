//! Ported from resources/pbtkit/tests/findability/test_floats.py.
//!
//! These tests verify the engine can find various categories of interesting
//! floating-point values (counterexamples to false properties), and does not
//! spuriously falsify true float properties.

use crate::common::utils::{expect_panic, find_any};
use hegel::generators as gs;
use hegel::{Hegel, Settings};

#[test]
fn test_inversion_is_imperfect() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x: f64 = tc.draw(gs::floats::<f64>());
                if x == 0.0 {
                    return;
                }
                let y = 1.0 / x;
                assert!(x * y == 1.0);
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_can_find_nan_in_list() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let xs: Vec<f64> = tc.draw(gs::vecs(gs::floats::<f64>()));
                assert!(!xs.iter().any(|x| x.is_nan()));
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_can_find_positive_infinity() {
    find_any(gs::floats::<f64>(), |x: &f64| {
        *x > 0.0 && x.is_infinite()
    });
}

#[test]
fn test_can_find_negative_infinity() {
    find_any(gs::floats::<f64>(), |x: &f64| {
        *x < 0.0 && x.is_infinite()
    });
}

#[test]
fn test_can_find_non_integer_float() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x: f64 = tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false));
                assert!(x == x.trunc());
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_can_find_integer_float() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x: f64 = tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false));
                assert!(x != x.trunc());
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_can_find_float_outside_exact_int_range() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x: f64 = tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false));
                assert!(x + 1.0 != x);
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_can_find_float_that_does_not_round_trip_through_str() {
    // Counterexample: NaN, since NaN != NaN regardless of how it round-trips.
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x: f64 = tc.draw(gs::floats::<f64>());
                let parsed: f64 = format!("{x}").parse().unwrap();
                assert!(parsed == x);
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_can_find_float_that_does_not_round_trip_through_repr() {
    // Rust has no separate `repr` formatting; `{:?}` matches `{}` for f64
    // round-tripping. Kept distinct to mirror the upstream test surface.
    expect_panic(
        || {
            Hegel::new(|tc| {
                let x: f64 = tc.draw(gs::floats::<f64>());
                let parsed: f64 = format!("{x:?}").parse().unwrap();
                assert!(parsed == x);
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

#[test]
fn test_half_bounded_generates_zero() {
    find_any(
        gs::floats::<f64>().min_value(-1.0).allow_nan(false),
        |x: &f64| *x == 0.0,
    );
    find_any(
        gs::floats::<f64>().max_value(1.0).allow_nan(false),
        |x: &f64| *x == 0.0,
    );
}

// ── True properties that should NOT be falsified ───────────────────────────

#[test]
fn test_is_float() {
    // Trivially true under Rust's static typing — drawing `f64` always
    // returns `f64`. Kept as a smoke test mirroring the upstream surface.
    Hegel::new(|tc| {
        tc.draw(gs::floats::<f64>());
    })
    .settings(Settings::new().test_cases(1000).database(None))
    .run();
}

#[test]
fn test_negation_is_self_inverse() {
    Hegel::new(|tc| {
        let x: f64 = tc.draw(gs::floats::<f64>().allow_nan(false));
        let y = -x;
        assert_eq!(-y, x);
    })
    .settings(Settings::new().test_cases(1000).database(None))
    .run();
}

#[test]
fn test_largest_range_has_no_infinities() {
    Hegel::new(|tc| {
        let x: f64 = tc.draw(
            gs::floats::<f64>()
                .min_value(-f64::MAX)
                .max_value(f64::MAX)
                .allow_nan(false),
        );
        assert!(!x.is_infinite());
    })
    .settings(Settings::new().test_cases(200).database(None))
    .run();
}

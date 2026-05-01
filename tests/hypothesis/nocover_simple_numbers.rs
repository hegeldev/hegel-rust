//! Ported from hypothesis-python/tests/nocover/test_simple_numbers.py.
//!
//! Python's upstream parametrizes the `test_minimizes_int_*_boundary`
//! family over ~32 numeric boundaries and
//! `test_floats_from_zero_have_reasonable_range` over k in 0..10; we
//! iterate in a `for` loop inside a single `#[test]` rather than expand
//! to ~100 separate tests. `test_floats_in_constrained_range` is
//! parametrized over only four (left, right) pairs so each gets its own
//! `#[test]`, matching the existing repo convention.
//!
//! `TestFloatsAreFloats` asserts `isinstance(arg, float)`; `gs::floats`
//! is statically typed `f64` in Rust, so per the api-mapping note those
//! become smoke tests that the generator runs.

use crate::common::utils::{Minimal, minimal};
use hegel::generators as gs;
use hegel::{Hegel, Settings};

#[test]
fn test_minimize_negative_int() {
    assert_eq!(minimal(gs::integers::<i64>(), |x: &i64| *x < 0), -1);
    assert_eq!(minimal(gs::integers::<i64>(), |x: &i64| *x < -1), -2);
}

#[test]
fn test_positive_negative_int() {
    assert_eq!(minimal(gs::integers::<i64>(), |x: &i64| *x > 0), 1);
    assert_eq!(minimal(gs::integers::<i64>(), |x: &i64| *x > 1), 2);
}

fn boundaries() -> Vec<i64> {
    let mut bs: Vec<i64> = Vec::new();
    for i in 0..10 {
        bs.push(1i64 << i);
        bs.push((1i64 << i) - 1);
        bs.push((1i64 << i) + 1);
    }
    for i in 0..6 {
        bs.push(10i64.pow(i));
    }
    bs.sort();
    bs.dedup();
    bs
}

#[test]
fn test_minimizes_int_down_to_boundary() {
    for boundary in boundaries() {
        assert_eq!(
            minimal(gs::integers::<i64>(), move |x: &i64| *x >= boundary),
            boundary,
            "boundary = {boundary}"
        );
    }
}

#[test]
fn test_minimizes_int_up_to_boundary() {
    for boundary in boundaries() {
        assert_eq!(
            minimal(gs::integers::<i64>(), move |x: &i64| *x <= -boundary),
            -boundary,
            "boundary = {boundary}"
        );
    }
}

#[test]
fn test_minimizes_ints_from_down_to_boundary() {
    for boundary in boundaries() {
        assert_eq!(
            minimal(
                gs::integers::<i64>().min_value(boundary - 10),
                move |x: &i64| {
                    assert!(*x >= boundary - 10);
                    *x >= boundary
                }
            ),
            boundary,
            "boundary = {boundary}"
        );
        assert_eq!(
            minimal(gs::integers::<i64>().min_value(boundary), |_: &i64| true),
            boundary,
            "boundary = {boundary}"
        );
    }
}

#[test]
fn test_minimizes_negative_integer_range_upwards() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(-10).max_value(-1),
            |_: &i64| true
        ),
        -1
    );
}

#[test]
fn test_minimizes_integer_range_to_boundary() {
    for boundary in boundaries() {
        assert_eq!(
            minimal(
                gs::integers::<i64>()
                    .min_value(boundary)
                    .max_value(boundary + 100),
                |_: &i64| true
            ),
            boundary,
            "boundary = {boundary}"
        );
    }
}

#[test]
fn test_single_integer_range_is_range() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(1).max_value(1),
            |_: &i64| true
        ),
        1
    );
}

#[test]
fn test_minimal_small_number_in_large_range() {
    assert_eq!(
        minimal(
            gs::integers::<i64>()
                .min_value(-(1i64 << 32))
                .max_value(1i64 << 32),
            |x: &i64| *x >= 101
        ),
        101
    );
}

#[test]
fn test_minimal_small_sum_float_list() {
    let xs = Minimal::new(gs::vecs(gs::floats::<f64>()).min_size(5), |x: &Vec<f64>| {
        x.iter().sum::<f64>() >= 1.0
    })
    .run();
    assert_eq!(xs, vec![0.0, 0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn test_minimals_boundary_floats() {
    assert_eq!(
        minimal(
            gs::floats::<f64>().min_value(-1.0).max_value(1.0),
            |_: &f64| true
        ),
        0.0
    );
}

#[test]
fn test_minimal_non_boundary_float() {
    assert_eq!(
        minimal(
            gs::floats::<f64>().min_value(1.0).max_value(9.0),
            |x: &f64| *x > 2.0
        ),
        3.0
    );
}

#[test]
fn test_minimal_float_is_zero() {
    assert_eq!(minimal(gs::floats::<f64>(), |_: &f64| true), 0.0);
}

#[test]
fn test_minimal_asymetric_bounded_float() {
    assert_eq!(
        minimal(
            gs::floats::<f64>().min_value(1.1).max_value(1.6),
            |_: &f64| true
        ),
        1.5
    );
}

#[test]
fn test_negative_floats_simplify_to_zero() {
    assert_eq!(minimal(gs::floats::<f64>(), |x: &f64| *x <= -1.0), -1.0);
}

#[test]
fn test_minimal_infinite_float_is_positive() {
    assert_eq!(
        minimal(gs::floats::<f64>(), |x: &f64| x.is_infinite()),
        f64::INFINITY
    );
}

#[test]
fn test_can_minimal_infinite_negative_float() {
    let x = minimal(gs::floats::<f64>(), |x: &f64| *x < -f64::MAX);
    assert!(x < -f64::MAX);
}

#[test]
fn test_can_minimal_float_on_boundary_of_representable() {
    minimal(gs::floats::<f64>(), |x: &f64| {
        *x + 1.0 == *x && !x.is_infinite()
    });
}

#[test]
fn test_minimize_nan() {
    assert!(minimal(gs::floats::<f64>(), |x: &f64| x.is_nan()).is_nan());
}

#[test]
fn test_minimize_very_large_float() {
    let t = f64::MAX / 2.0;
    assert_eq!(minimal(gs::floats::<f64>(), move |x: &f64| *x >= t), t);
}

fn is_integral(value: f64) -> bool {
    value.is_finite() && value == value.trunc()
}

#[test]
fn test_can_minimal_float_far_from_integral() {
    minimal(gs::floats::<f64>(), |x: &f64| {
        x.is_finite() && !is_integral(*x * (1u64 << 32) as f64)
    });
}

#[test]
fn test_list_of_fractional_float() {
    let xs = Minimal::new(gs::vecs(gs::floats::<f64>()).min_size(5), |x: &Vec<f64>| {
        x.iter().filter(|t| **t >= 1.5).count() >= 5
    })
    .run();
    let distinct: std::collections::HashSet<u64> = xs.iter().map(|v| v.to_bits()).collect();
    assert_eq!(distinct.len(), 1);
    assert_eq!(xs[0], 2.0);
}

#[test]
fn test_minimal_fractional_float() {
    assert_eq!(minimal(gs::floats::<f64>(), |x: &f64| *x >= 1.5), 2.0);
}

#[test]
fn test_minimizes_lists_of_negative_ints_up_to_boundary() {
    let result = minimal(
        gs::vecs(gs::integers::<i64>()).min_size(10),
        |x: &Vec<i64>| x.iter().filter(|t| **t <= -1).count() >= 10,
    );
    assert_eq!(result, vec![-1; 10]);
}

fn check_floats_in_constrained_range(left: f64, right: f64) {
    Hegel::new(move |tc| {
        let r: f64 = tc.draw(gs::floats::<f64>().min_value(left).max_value(right));
        assert!(left <= r && r <= right);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_floats_in_constrained_range_zero_up() {
    check_floats_in_constrained_range(0.0, f64::from_bits(1));
}

#[test]
fn test_floats_in_constrained_range_zero_down() {
    check_floats_in_constrained_range(-f64::from_bits(1), 0.0);
}

#[test]
fn test_floats_in_constrained_range_straddle_zero() {
    check_floats_in_constrained_range(-f64::from_bits(1), f64::from_bits(1));
}

#[test]
fn test_floats_in_constrained_range_subnormal_pair() {
    check_floats_in_constrained_range(f64::from_bits(1), f64::from_bits(2));
}

#[test]
fn test_bounds_are_respected() {
    assert_eq!(
        minimal(gs::floats::<f64>().min_value(1.0), |_: &f64| true),
        1.0
    );
    assert_eq!(
        minimal(gs::floats::<f64>().max_value(-1.0), |_: &f64| true),
        -1.0
    );
}

#[test]
fn test_floats_from_zero_have_reasonable_range() {
    for k in 0..10i32 {
        let n = 10f64.powi(k);
        assert_eq!(
            minimal(gs::floats::<f64>().min_value(0.0), move |x: &f64| *x >= n),
            n,
            "k = {k}"
        );
        assert_eq!(
            minimal(gs::floats::<f64>().max_value(0.0), move |x: &f64| *x <= -n),
            -n,
            "k = {k}"
        );
    }
}

#[test]
fn test_explicit_allow_nan() {
    minimal(gs::floats::<f64>().allow_nan(true), |x: &f64| x.is_nan());
}

#[test]
fn test_one_sided_contains_infinity() {
    minimal(gs::floats::<f64>().min_value(1.0), |x: &f64| {
        x.is_infinite()
    });
    minimal(gs::floats::<f64>().max_value(1.0), |x: &f64| {
        x.is_infinite()
    });
}

#[test]
fn test_no_allow_infinity_upper() {
    Hegel::new(|tc| {
        let x: f64 = tc.draw(gs::floats::<f64>().min_value(0.0).allow_infinity(false));
        assert!(!x.is_infinite());
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_no_allow_infinity_lower() {
    Hegel::new(|tc| {
        let x: f64 = tc.draw(gs::floats::<f64>().max_value(0.0).allow_infinity(false));
        assert!(!x.is_infinite());
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

// TestFloatsAreFloats: upstream asserts isinstance(arg, float). f64 is
// statically typed in Rust, so these reduce to smoke tests.

#[test]
fn test_floats_are_floats_unbounded() {
    Hegel::new(|tc| {
        tc.draw(gs::floats::<f64>());
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_floats_are_floats_int_float_bounds() {
    Hegel::new(|tc| {
        tc.draw(
            gs::floats::<f64>()
                .min_value(0.0)
                .max_value(u64::MAX as f64),
        );
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_floats_are_floats_float_float_bounds() {
    Hegel::new(|tc| {
        tc.draw(
            gs::floats::<f64>()
                .min_value(0.0_f64)
                .max_value(u64::MAX as f64),
        );
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

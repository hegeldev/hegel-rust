//! Ported from hypothesis-python/tests/cover/test_float_utils.py

#![cfg(feature = "native")]

use hegel::__native_test_internals::{
    FloatConstraints, choice_equal_float, choice_permitted_float, count_between_floats,
    make_float_clamper, next_down, next_up, sign_aware_lte,
};
use hegel::generators as gs;
use hegel::{Hegel, Settings};

// SMALLEST_SUBNORMAL = next_up(0.0) = bit pattern 1.
const SMALLEST_SUBNORMAL: f64 = 5e-324;

fn float_constr(min_value: f64, max_value: f64) -> FloatConstraints {
    FloatConstraints {
        min_value,
        max_value,
        allow_nan: true,
        smallest_nonzero_magnitude: SMALLEST_SUBNORMAL,
    }
}

fn float_constr_nan(min_value: f64, max_value: f64, allow_nan: bool) -> FloatConstraints {
    FloatConstraints {
        min_value,
        max_value,
        allow_nan,
        smallest_nonzero_magnitude: SMALLEST_SUBNORMAL,
    }
}

fn float_constr_magnitude(
    min_value: f64,
    max_value: f64,
    smallest_nonzero_magnitude: f64,
) -> FloatConstraints {
    FloatConstraints {
        min_value,
        max_value,
        allow_nan: true,
        smallest_nonzero_magnitude,
    }
}

#[test]
fn test_can_handle_straddling_zero() {
    assert_eq!(count_between_floats(-0.0, 0.0), 2);
}

#[test]
fn test_next_up_nan() {
    assert!(next_up(f64::NAN).is_nan());
}

#[test]
fn test_next_up_inf() {
    assert_eq!(next_up(f64::INFINITY), f64::INFINITY);
}

#[test]
fn test_next_up_neg_zero() {
    assert_eq!(next_up(-0.0), -0.0);
}

#[test]
fn test_next_down_nan() {
    assert!(next_down(f64::NAN).is_nan());
}

#[test]
fn test_next_down_neg_inf() {
    assert_eq!(next_down(f64::NEG_INFINITY), f64::NEG_INFINITY);
}

#[test]
fn test_next_down_zero() {
    assert_eq!(next_down(0.0), 0.0);
}

// Pin the non-edge branches of next_up / next_down: for a finite non-zero
// value the result must be the adjacent float bit pattern. Without these,
// the `n >= 0` / `n < 0` branch of `next_up` is never exercised by the
// Python-mirrored tests above (each of those cases short-circuits in an
// early return).
#[test]
fn test_next_up_next_down_finite() {
    assert_eq!(next_up(1.0), f64::from_bits(1.0_f64.to_bits() + 1));
    assert_eq!(next_up(-1.0), f64::from_bits((-1.0_f64).to_bits() - 1));
    assert_eq!(next_down(1.0), f64::from_bits(1.0_f64.to_bits() - 1));
    assert_eq!(next_down(-1.0), f64::from_bits((-1.0_f64).to_bits() + 1));
}

fn check_float_clamper(constraints: &FloatConstraints, input_value: f64) {
    let clamper = make_float_clamper(constraints);
    let clamped = clamper(input_value);
    if clamped.is_nan() {
        assert!(constraints.allow_nan);
    } else {
        assert!(sign_aware_lte(constraints.min_value, clamped));
        assert!(sign_aware_lte(clamped, constraints.max_value));
    }
    if choice_permitted_float(input_value, constraints) {
        assert!(choice_equal_float(input_value, clamped));
    }
}

#[test]
fn test_float_clamper_examples() {
    // exponent comparisons:
    check_float_clamper(&float_constr(1.0, f64::MAX), 0.0);
    check_float_clamper(&float_constr(1.0, f64::MAX), 1.0);
    check_float_clamper(&float_constr(1.0, f64::MAX), 10.0);
    check_float_clamper(&float_constr(1.0, f64::MAX), f64::MAX);
    check_float_clamper(&float_constr(1.0, f64::MAX), f64::INFINITY);

    // mantissa comparisons:
    check_float_clamper(&float_constr(100.0001, 100.0003), 100.0001);
    check_float_clamper(&float_constr(100.0001, 100.0003), 100.0002);
    check_float_clamper(&float_constr(100.0001, 100.0003), 100.0003);
    check_float_clamper(&float_constr_nan(100.0001, 100.0003, false), f64::NAN);
    check_float_clamper(&float_constr_nan(0.0, 10.0, false), f64::NAN);
    check_float_clamper(&float_constr_nan(0.0, 10.0, true), f64::NAN);

    // branch coverage of resampling in the "out of range of smallest magnitude" case
    check_float_clamper(&float_constr_magnitude(-4.0, -1.0, 4.0), 4.0);
    check_float_clamper(&float_constr_magnitude(-4.0, -1.0, 4.0), 5.0);
    check_float_clamper(&float_constr_magnitude(-4.0, -1.0, 4.0), 6.0);
    check_float_clamper(&float_constr_magnitude(1.0, 4.0, 4.0), -4.0);
    check_float_clamper(&float_constr_magnitude(1.0, 4.0, 4.0), -5.0);
    check_float_clamper(&float_constr_magnitude(1.0, 4.0, 4.0), -6.0);

    check_float_clamper(&float_constr(-5e-324, -0.0), 3.0);
    check_float_clamper(&float_constr(0.0, 0.0), -0.0);
    check_float_clamper(&float_constr(-0.0, -0.0), 0.0);
}

// Exercise the defensive `return lower` branch of the clamp at the tail of
// `make_float_clamper`: when the constraint is pathological (no value can
// satisfy both `sm > max` and `-sm >= min`) the clamper falls back to
// `min_value` rather than returning a value below it. Python's port relies
// on the defensive clamp for exactly this robustness; Python coverage
// doesn't test it, but Rust's ratchet demands a witness.
#[test]
fn test_float_clamper_defensive_lower() {
    let c = FloatConstraints {
        min_value: -3.0,
        max_value: -1.0,
        allow_nan: false,
        smallest_nonzero_magnitude: 4.0,
    };
    let clamper = make_float_clamper(&c);
    assert_eq!(clamper(5.0), -3.0);
}

#[test]
fn test_float_clamper_property() {
    Hegel::new(|tc| {
        let use_min_value: bool = tc.draw(gs::booleans());
        let use_max_value: bool = tc.draw(gs::booleans());
        let allow_nan: bool = tc.draw(gs::booleans());

        let min_value = if use_min_value {
            tc.draw(gs::floats::<f64>().allow_nan(false))
        } else {
            f64::NEG_INFINITY
        };
        let max_value = if use_max_value {
            tc.draw(gs::floats::<f64>().min_value(min_value).allow_nan(false))
        } else {
            f64::INFINITY
        };

        let largest_magnitude = min_value.abs().max(max_value.abs());
        let smallest_nonzero_magnitude = if largest_magnitude > 0.0 {
            tc.draw(
                gs::floats::<f64>()
                    .min_value(0.0)
                    .max_value(largest_magnitude.min(1.0))
                    .exclude_min(true)
                    .allow_infinity(false),
            )
        } else {
            SMALLEST_SUBNORMAL
        };

        assert!(sign_aware_lte(min_value, max_value));
        let constraints = FloatConstraints {
            min_value,
            max_value,
            allow_nan,
            smallest_nonzero_magnitude,
        };
        let input_value: f64 = tc.draw(gs::floats::<f64>());
        check_float_clamper(&constraints, input_value);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

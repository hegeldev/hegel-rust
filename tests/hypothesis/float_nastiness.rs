//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_float_nastiness.py.
//!
//! Tests that rely on Python API surface absent in hegel-rust are omitted:
//!
//! - `test_float16_can_exclude_infinity` — f16 is not supported (the `Float`
//!   trait is implemented only for `f32` and `f64`).
//! - `test_disallowed_width` — hegel-rust's float generator has no `width`
//!   parameter (width is implicit in the generator's type).
//! - `test_out_of_range` — relies on Python's arbitrary-precision integer
//!   overflow semantics (`10**400` as a bound) and the `width` kwarg.
//! - `test_no_single_floats_in_range` — depends on the `width` kwarg
//!   decoupling bound precision from generated precision.
//! - `test_fuzzing_floats_bounds` — parametrizes over `st.nothing()` (no
//!   hegel-rust counterpart) and `width=16`.
//!
//! Tests checking that invalid argument combinations raise InvalidArgument
//! are native-gated off: validation currently happens server-side, and the
//! native backend does not yet reject these combinations.

#[cfg(not(feature = "native"))]
use crate::common::utils::expect_panic;
use crate::common::utils::{assert_all_examples, check_can_generate_examples, find_any, minimal};
use hegel::generators::{self as gs, Generator};
#[cfg(not(feature = "native"))]
use hegel::{Hegel, Settings};

#[test]
fn test_floats_are_in_range_large() {
    let lower = 9.9792015476736e291_f64;
    let upper = 1.7976931348623157e308_f64;
    assert_all_examples(
        gs::floats::<f64>().min_value(lower).max_value(upper),
        move |t: &f64| lower <= *t && *t <= upper,
    );
}

#[test]
fn test_floats_are_in_range_full() {
    let lower = -f64::MAX;
    let upper = f64::MAX;
    assert_all_examples(
        gs::floats::<f64>().min_value(lower).max_value(upper),
        move |t: &f64| lower <= *t && *t <= upper,
    );
}

#[test]
fn test_can_generate_positive_zero() {
    let result = minimal(gs::floats::<f64>(), |x: &f64| !x.is_sign_negative());
    assert_eq!(result, 0.0);
    assert!(!result.is_sign_negative());
}

#[test]
fn test_can_generate_negative_zero() {
    let result = minimal(gs::floats::<f64>(), |x: &f64| x.is_sign_negative());
    assert_eq!(result, 0.0);
    assert!(result.is_sign_negative());
}

const ZERO_INTERVAL_CASES: [(f64, f64); 4] = [
    (-1.0, 1.0),
    (-0.0, 1.0),
    (-1.0, 0.0),
    (-f64::MIN_POSITIVE, f64::MIN_POSITIVE),
];

#[test]
fn test_can_generate_positive_zero_in_interval() {
    for (l, r) in ZERO_INTERVAL_CASES {
        let result = minimal(gs::floats::<f64>().min_value(l).max_value(r), |x: &f64| {
            !x.is_sign_negative()
        });
        assert_eq!(result, 0.0);
        assert!(!result.is_sign_negative());
    }
}

#[test]
fn test_can_generate_negative_zero_in_interval() {
    for (l, r) in ZERO_INTERVAL_CASES {
        let result = minimal(gs::floats::<f64>().min_value(l).max_value(r), |x: &f64| {
            x.is_sign_negative()
        });
        assert_eq!(result, 0.0);
        assert!(result.is_sign_negative());
    }
}

#[test]
fn test_does_not_generate_negative_if_right_boundary_is_positive() {
    assert_all_examples(
        gs::floats::<f64>().min_value(0.0).max_value(1.0),
        |x: &f64| !x.is_sign_negative(),
    );
}

#[test]
fn test_does_not_generate_positive_if_right_boundary_is_negative() {
    assert_all_examples(
        gs::floats::<f64>().min_value(-1.0).max_value(-0.0),
        |x: &f64| x.is_sign_negative(),
    );
}

#[test]
fn test_half_bounded_generates_zero_from_min() {
    find_any(gs::floats::<f64>().min_value(-1.0), |x: &f64| *x == 0.0);
}

#[test]
fn test_half_bounded_generates_zero_from_max() {
    find_any(gs::floats::<f64>().max_value(1.0), |x: &f64| *x == 0.0);
}

#[test]
fn test_half_bounded_respects_sign_of_upper_bound() {
    assert_all_examples(gs::floats::<f64>().max_value(-0.0), |x: &f64| {
        x.is_sign_negative()
    });
}

#[test]
fn test_half_bounded_respects_sign_of_lower_bound() {
    assert_all_examples(gs::floats::<f64>().min_value(0.0), |x: &f64| {
        !x.is_sign_negative()
    });
}

#[test]
fn test_filter_nan() {
    assert_all_examples(gs::floats::<f64>().allow_nan(false), |x: &f64| !x.is_nan());
}

#[test]
fn test_filter_infinity() {
    assert_all_examples(gs::floats::<f64>().allow_infinity(false), |x: &f64| {
        !x.is_infinite()
    });
}

#[test]
fn test_can_guard_against_draws_of_nan() {
    let tagged_floats = gs::one_of(vec![
        gs::tuples!(gs::just(0_i32), gs::floats::<f64>().allow_nan(false)).boxed(),
        gs::tuples!(gs::just(1_i32), gs::floats::<f64>().allow_nan(true)).boxed(),
    ]);
    let (tag, _f) = minimal(tagged_floats, |x: &(i32, f64)| x.1.is_nan());
    assert_eq!(tag, 1);
}

#[test]
fn test_very_narrow_interval() {
    let upper_bound = -1.0_f64;
    let lower_bound = f64::from_bits(upper_bound.to_bits() + 10);
    assert!(lower_bound < upper_bound);

    assert_all_examples(
        gs::floats::<f64>()
            .min_value(lower_bound)
            .max_value(upper_bound),
        move |f: &f64| lower_bound <= *f && *f <= upper_bound,
    );
}

#[test]
fn test_up_means_greater() {
    assert_all_examples(gs::floats::<f64>(), |x: &f64| {
        let hi = x.next_up();
        if *x < hi {
            return true;
        }
        (x.is_nan() && hi.is_nan())
            || (*x > 0.0 && x.is_infinite())
            || (*x == hi && *x == 0.0 && x.is_sign_negative() && !hi.is_sign_negative())
    });
}

#[test]
fn test_down_means_lesser() {
    assert_all_examples(gs::floats::<f64>(), |x: &f64| {
        let lo = x.next_down();
        if *x > lo {
            return true;
        }
        (x.is_nan() && lo.is_nan())
            || (*x < 0.0 && x.is_infinite())
            || (*x == lo && *x == 0.0 && lo.is_sign_negative() && !x.is_sign_negative())
    });
}

#[test]
fn test_updown_roundtrip() {
    assert_all_examples(
        gs::floats::<f64>().allow_nan(false).allow_infinity(false),
        |val: &f64| *val == val.next_down().next_up() && *val == val.next_up().next_down(),
    );
}

#[test]
fn test_float32_can_exclude_infinity() {
    assert_all_examples(gs::floats::<f32>().allow_infinity(false), |x: &f32| {
        !x.is_infinite()
    });
}

#[test]
fn test_finite_min_bound_does_not_overflow() {
    assert_all_examples(
        gs::floats::<f64>().min_value(1e304).allow_infinity(false),
        |x: &f64| !x.is_infinite(),
    );
}

#[test]
fn test_finite_max_bound_does_not_overflow() {
    assert_all_examples(
        gs::floats::<f64>().max_value(-1e304).allow_infinity(false),
        |x: &f64| !x.is_infinite(),
    );
}

#[test]
fn test_can_exclude_endpoints() {
    assert_all_examples(
        gs::floats::<f64>()
            .min_value(0.0)
            .max_value(1.0)
            .exclude_min(true)
            .exclude_max(true),
        |x: &f64| 0.0 < *x && *x < 1.0,
    );
}

#[test]
fn test_can_exclude_neg_infinite_endpoint() {
    assert_all_examples(
        gs::floats::<f64>()
            .min_value(f64::NEG_INFINITY)
            .max_value(-1e307)
            .exclude_min(true),
        |x: &f64| !x.is_infinite(),
    );
}

#[test]
fn test_can_exclude_pos_infinite_endpoint() {
    assert_all_examples(
        gs::floats::<f64>()
            .min_value(1e307)
            .max_value(f64::INFINITY)
            .exclude_max(true),
        |x: &f64| !x.is_infinite(),
    );
}

#[test]
fn test_zero_intervals_are_ok() {
    check_can_generate_examples(gs::floats::<f64>().min_value(0.0).max_value(0.0));
    check_can_generate_examples(gs::floats::<f64>().min_value(-0.0).max_value(0.0));
    check_can_generate_examples(gs::floats::<f64>().min_value(-0.0).max_value(-0.0));
}

// Validation-only tests: the server's Hypothesis rejects these invalid argument
// combinations with an InvalidArgument error. The native backend does not yet
// enforce these checks, so we gate the tests to server mode.

#[cfg(not(feature = "native"))]
#[test]
fn test_exclude_infinite_endpoint_is_invalid_min() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let _: f64 = tc.draw(
                    gs::floats::<f64>()
                        .min_value(f64::INFINITY)
                        .exclude_min(true),
                );
            })
            .settings(Settings::new().test_cases(1).database(None))
            .run();
        },
        "InvalidArgument",
    );
}

#[cfg(not(feature = "native"))]
#[test]
fn test_exclude_infinite_endpoint_is_invalid_max() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let _: f64 = tc.draw(
                    gs::floats::<f64>()
                        .max_value(f64::NEG_INFINITY)
                        .exclude_max(true),
                );
            })
            .settings(Settings::new().test_cases(1).database(None))
            .run();
        },
        "InvalidArgument",
    );
}

#[cfg(not(feature = "native"))]
#[test]
fn test_exclude_entire_interval() {
    for bound in [1.0_f64, -1.0_f64, 1e10_f64, -1e-10_f64] {
        for (lo, hi) in [(true, false), (false, true), (true, true)] {
            expect_panic(
                || {
                    Hegel::new(move |tc| {
                        let _: f64 = tc.draw(
                            gs::floats::<f64>()
                                .min_value(bound)
                                .max_value(bound)
                                .exclude_min(lo)
                                .exclude_max(hi),
                        );
                    })
                    .settings(Settings::new().test_cases(1).database(None))
                    .run();
                },
                "InvalidArgument",
            );
        }
    }
}

#[cfg(not(feature = "native"))]
#[test]
fn test_cannot_exclude_endpoint_with_zero_interval() {
    for lo in [0.0_f64, -0.0_f64] {
        for hi in [0.0_f64, -0.0_f64] {
            for (exmin, exmax) in [(true, false), (false, true), (true, true)] {
                expect_panic(
                    || {
                        Hegel::new(move |tc| {
                            let _: f64 = tc.draw(
                                gs::floats::<f64>()
                                    .min_value(lo)
                                    .max_value(hi)
                                    .exclude_min(exmin)
                                    .exclude_max(exmax),
                            );
                        })
                        .settings(Settings::new().test_cases(1).database(None))
                        .run();
                    },
                    "InvalidArgument",
                );
            }
        }
    }
}

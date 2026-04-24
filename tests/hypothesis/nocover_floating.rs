//! Ported from hypothesis-python/tests/nocover/test_floating.py.
//!
//! The upstream uses `@fails` to assert that a particular property does
//! not hold universally — i.e. Hypothesis can find a counterexample. In
//! hegel-rust the equivalent is a direct `find_any` call whose condition
//! is the negation of the Python assertion (plus its `assume(...)` guard).
//! The upstream's `TRY_HARDER` settings bump `max_examples` to 1000 and
//! suppress `filter_too_much`; we mirror both on the finders that filter
//! to rare values (NaN, infinity).
//!
//! `test_can_find_negative_and_signaling_nans` is driven through
//! `gs::nan_floats()` (hegel-rust's port of Hypothesis's `NanStrategy`)
//! rather than the `floats().filter(math.isnan)` + filter-rewriting path
//! that Python uses. hegel-rust's `.filter()` is a generic 3-try rejection
//! sampler with no `is_nan` special case, so the direct NaN generator is
//! needed to hit all four (sign × mantissa-pattern) variants inside the
//! 1000-attempt budget.

use crate::common::utils::{FindAny, assert_all_examples};
use hegel::generators as gs;
use hegel::{HealthCheck, Hegel, Settings};

#[test]
fn test_is_float() {
    // Rust's f64 generator is statically typed, so every drawn value is a
    // float by construction; we still assert the generator runs.
    assert_all_examples(gs::floats::<f64>(), |_: &f64| true);
}

#[test]
fn test_inversion_is_imperfect() {
    // @fails: find x != 0 such that x * (1/x) != 1.0. A NaN draw satisfies
    // the condition (1/NaN = NaN, NaN * NaN = NaN, NaN != 1.0).
    FindAny::new(gs::floats::<f64>(), |x: &f64| {
        *x != 0.0 && *x * (1.0 / *x) != 1.0
    })
    .max_attempts(1000)
    .suppress_health_check(HealthCheck::FilterTooMuch)
    .run();
}

#[test]
fn test_largest_range() {
    assert_all_examples(
        gs::floats::<f64>().min_value(-f64::MAX).max_value(f64::MAX),
        |x: &f64| !x.is_infinite(),
    );
}

#[test]
fn test_negation_is_self_inverse() {
    // Not a @fails test, so Hegel::new directly. TRY_HARDER → test_cases(1000)
    // + FilterTooMuch suppression, mirroring the upstream.
    Hegel::new(|tc| {
        let x: f64 = tc.draw(gs::floats::<f64>());
        tc.assume(!x.is_nan());
        let y = -x;
        assert!(-y == x);
    })
    .settings(
        Settings::new()
            .test_cases(1000)
            .database(None)
            .suppress_health_check([HealthCheck::FilterTooMuch]),
    )
    .run();
}

#[test]
fn test_is_not_nan() {
    // @fails: find a list containing a NaN.
    FindAny::new(gs::vecs(gs::floats::<f64>()), |xs: &Vec<f64>| {
        xs.iter().any(|x| x.is_nan())
    })
    .max_attempts(1000)
    .suppress_health_check(HealthCheck::FilterTooMuch)
    .run();
}

#[test]
fn test_is_not_positive_infinite() {
    FindAny::new(gs::floats::<f64>(), |x: &f64| *x > 0.0 && x.is_infinite())
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
}

#[test]
fn test_is_not_negative_infinite() {
    FindAny::new(gs::floats::<f64>(), |x: &f64| *x < 0.0 && x.is_infinite())
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
}

#[test]
fn test_is_int() {
    // @fails: find a finite float that is not integer-valued (e.g. 0.5).
    FindAny::new(gs::floats::<f64>(), |x: &f64| {
        x.is_finite() && *x != x.trunc()
    })
    .max_attempts(1000)
    .suppress_health_check(HealthCheck::FilterTooMuch)
    .run();
}

#[test]
fn test_is_not_int() {
    // @fails: find a finite integer-valued float (e.g. 0.0).
    FindAny::new(gs::floats::<f64>(), |x: &f64| {
        x.is_finite() && *x == x.trunc()
    })
    .max_attempts(1000)
    .suppress_health_check(HealthCheck::FilterTooMuch)
    .run();
}

#[test]
fn test_is_in_exact_int_range() {
    // @fails: find a finite float so large that x + 1 == x (magnitude ≥ 2^53).
    FindAny::new(gs::floats::<f64>(), |x: &f64| {
        x.is_finite() && *x + 1.0 == *x
    })
    .max_attempts(1000)
    .suppress_health_check(HealthCheck::FilterTooMuch)
    .run();
}

#[test]
fn test_can_find_floats_that_do_not_round_trip_through_strings() {
    // @fails: find x where its string form doesn't parse back to an equal
    // value. NaN satisfies this because NaN != NaN.
    FindAny::new(gs::floats::<f64>(), |x: &f64| {
        x.to_string().parse::<f64>().unwrap() != *x
    })
    .max_attempts(1000)
    .suppress_health_check(HealthCheck::FilterTooMuch)
    .run();
}

#[test]
fn test_can_find_floats_that_do_not_round_trip_through_reprs() {
    // Rust has no `repr()`; the Debug format plays the same role and produces
    // a round-trippable representation for all finite/infinite floats. NaN
    // still breaks it because NaN != NaN.
    FindAny::new(gs::floats::<f64>(), |x: &f64| {
        format!("{x:?}").parse::<f64>().unwrap() != *x
    })
    .max_attempts(1000)
    .suppress_health_check(HealthCheck::FilterTooMuch)
    .run();
}

// Upstream parametrises this over (snan, neg) ∈ {False, True}²; port the
// four cases as separate tests so each runs under its own TRY_HARDER budget.
// `snan` is true when `abs(x)`'s bit pattern differs from `f64::NAN`'s (i.e.
// the mantissa has any non-high bit set); `neg` is true when the sign bit
// is set. Matches `float_to_int(abs(x)) != float_to_int(float("nan"))` and
// `math.copysign(1, x) == -1` from the upstream.

fn variant_matches(x: f64, snan: bool, neg: bool) -> bool {
    let abs_bits = x.abs().to_bits();
    let nan_bits = f64::NAN.to_bits();
    let is_snan = abs_bits != nan_bits;
    let is_neg = x.is_sign_negative();
    snan == is_snan && neg == is_neg
}

// The two `quiet` variants require mantissa_bits == 0 exactly, which
// `gs::nan_floats()` produces with only ~0.5% combined probability per
// draw (≈1% for mantissa=0 via the nasty-boundary path × 50% for the
// sign bit). At the upstream's TRY_HARDER budget of 1000 the residual
// failure rate is empirically ~7% — high enough to break CI. The
// signaling variants need mantissa != 0 (essentially always true), so
// they're fine at 1000. Bumping the two quiet tests to 10_000 drops the
// failure odds to well below 1e-6 while still completing in <1s. The
// upstream passes at 1000 because Hypothesis's example database caches
// the counterexample across runs; our FindAny disables the database to
// keep tests hermetic.
const QUIET_NAN_ATTEMPTS: u64 = 10_000;

#[test]
fn test_can_find_negative_and_signaling_nans_quiet_positive() {
    FindAny::new(gs::nan_floats(), |x: &f64| {
        variant_matches(*x, false, false)
    })
    .max_attempts(QUIET_NAN_ATTEMPTS)
    .suppress_health_check(HealthCheck::FilterTooMuch)
    .run();
}

#[test]
fn test_can_find_negative_and_signaling_nans_quiet_negative() {
    FindAny::new(gs::nan_floats(), |x: &f64| variant_matches(*x, false, true))
        .max_attempts(QUIET_NAN_ATTEMPTS)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
}

#[test]
fn test_can_find_negative_and_signaling_nans_signaling_positive() {
    FindAny::new(gs::nan_floats(), |x: &f64| variant_matches(*x, true, false))
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
}

#[test]
fn test_can_find_negative_and_signaling_nans_signaling_negative() {
    FindAny::new(gs::nan_floats(), |x: &f64| variant_matches(*x, true, true))
        .max_attempts(1000)
        .suppress_health_check(HealthCheck::FilterTooMuch)
        .run();
}

#[test]
fn test_floats_are_in_range() {
    Hegel::new(|tc| {
        let x: f64 = tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false));
        let y: f64 = tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false));
        let (x, y) = if x <= y { (x, y) } else { (y, x) };
        tc.assume(x < y);

        let t: f64 = tc.draw(gs::floats::<f64>().min_value(x).max_value(y));
        assert!(x <= t && t <= y);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

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
//! Individually-skipped tests:
//!
//! - `test_can_find_negative_and_signaling_nans` — the upstream relies on
//!   Hypothesis's `filter_rewriting` optimisation to turn
//!   `floats().filter(math.isnan)` into a NaN-only strategy, then hunts
//!   for each of the four (sign × signaling) bit-pattern variants in
//!   1000 filtered draws. hegel-rust's `.filter()` is a generic 3-try
//!   rejection sampler, so within 1000 test cases we see only a handful
//!   of NaNs total — not enough to hit every signaling/quiet × +/− slot.
//!   Unskip once `src/native/` grows filter rewriting for `is_nan`-style
//!   predicates (or a dedicated NaN-only float generator).

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

//! Ported from hypothesis-python/tests/conjecture/test_forced.py
//!
//! Tests Hypothesis's forced-value mechanism: calling a draw with a `forced`
//! argument returns that specific value, and the resulting choice sequence
//! replays (via `ConjectureData.for_choices`) to the same value without
//! needing `forced`.
//!
//! Individually-skipped tests:
//!
//! - `test_forced_many` — `cu.many(data, ..., forced=N)` takes a forced
//!   total-count argument that our native `ManyState` does not expose;
//!   `ManyState::new(min_size, max_size)` has no forced-count parameter
//!   and `schema::mod::many_more` only forces the *per-step* boolean
//!   based on min/max bounds.
//! - `test_forced_with_large_magnitude_integers` — the test uses
//!   `2**127 + 1` as an integer bound, which overflows `i128`. Native
//!   `draw_integer` takes `i128` bounds and cannot represent the
//!   Python-bignum range this test exercises.
//! - The `@given(choice_types_constraints(use_forced=True))` branch of
//!   `test_forced_values` — requires porting
//!   `hypothesis.internal.conjecture.provider_conformance.choice_types_constraints`,
//!   a separate strategy with no native counterpart yet.
//! - The four `@example("integer", {"shrink_towards":…, "weights":{…}, "forced":…})`
//!   cases of `test_forced_values` — native `draw_integer(min, max)` has
//!   no `shrink_towards` or `weights` constraint.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{ChoiceValue, NativeTestCase, choice_equal_float};
use rand::SeedableRng;
use rand::rngs::SmallRng;

/// `hypothesis.internal.floats.SMALLEST_SUBNORMAL` — `f64::from_bits(1)`.
const SMALLEST_SUBNORMAL: f64 = f64::from_bits(1);
/// `hypothesis.internal.floats.SIGNALING_NAN` — bit pattern `0x7FF4_…`.
const SIGNALING_NAN: f64 = f64::from_bits(0x7FF4_0000_0000_0000);

fn choices_of(ntc: &NativeTestCase) -> Vec<ChoiceValue> {
    ntc.nodes.iter().map(|n| n.value.clone()).collect()
}

fn fresh() -> NativeTestCase {
    NativeTestCase::new_random(SmallRng::seed_from_u64(0))
}

// -- test_forced_values @example cases ------------------------------------
//
// Each test drives one concrete `@example` from the Python original. The
// shape is identical across choice types:
//   1. draw with `forced=X` → result equals X
//   2. replay the resulting choice sequence via `for_choices` → same X

#[test]
fn test_forced_values_boolean_64bit_p() {
    // @example(("boolean", {"p": 1e-19, "forced": True}))
    let mut ntc = fresh();
    let v = ntc.weighted(1e-19, Some(true)).ok().unwrap();
    assert!(v);

    let choices = choices_of(&ntc);
    let mut replay = NativeTestCase::for_choices(&choices, None);
    assert!(replay.weighted(1e-19, None).ok().unwrap());
}

#[test]
fn test_forced_values_boolean_62bit_p() {
    // @example(("boolean", {"p": 3e-19, "forced": True}))
    let mut ntc = fresh();
    let v = ntc.weighted(3e-19, Some(true)).ok().unwrap();
    assert!(v);

    let choices = choices_of(&ntc);
    let mut replay = NativeTestCase::for_choices(&choices, None);
    assert!(replay.weighted(3e-19, None).ok().unwrap());
}

fn forced_float_roundtrip(
    min_value: f64,
    max_value: f64,
    allow_nan: bool,
    allow_infinity: bool,
    forced: f64,
) {
    let mut ntc = fresh();
    let drawn = ntc
        .draw_float_forced(min_value, max_value, allow_nan, allow_infinity, forced)
        .ok()
        .unwrap();
    assert!(choice_equal_float(drawn, forced));

    let choices = choices_of(&ntc);
    let mut replay = NativeTestCase::for_choices(&choices, None);
    let replayed = replay
        .draw_float(min_value, max_value, allow_nan, allow_infinity)
        .ok()
        .unwrap();
    assert!(choice_equal_float(replayed, forced));
}

// The Python test has a single `test_forced_values` parameterized by `@example`;
// each Rust test below mirrors one `@example` row for `choice_type == "float"`.
// Defaults: `min_value=-inf, max_value=inf, allow_nan=true, allow_infinity=true`.
#[test]
fn test_forced_values_float_zero() {
    forced_float_roundtrip(f64::NEG_INFINITY, f64::INFINITY, true, true, 0.0);
}

#[test]
fn test_forced_values_float_neg_zero() {
    forced_float_roundtrip(f64::NEG_INFINITY, f64::INFINITY, true, true, -0.0);
}

#[test]
fn test_forced_values_float_one() {
    forced_float_roundtrip(f64::NEG_INFINITY, f64::INFINITY, true, true, 1.0);
}

#[test]
fn test_forced_values_float_small() {
    forced_float_roundtrip(f64::NEG_INFINITY, f64::INFINITY, true, true, 1.2345);
}

#[test]
fn test_forced_values_float_smallest_subnormal() {
    forced_float_roundtrip(
        f64::NEG_INFINITY,
        f64::INFINITY,
        true,
        true,
        SMALLEST_SUBNORMAL,
    );
}

#[test]
fn test_forced_values_float_neg_smallest_subnormal() {
    forced_float_roundtrip(
        f64::NEG_INFINITY,
        f64::INFINITY,
        true,
        true,
        -SMALLEST_SUBNORMAL,
    );
}

#[test]
fn test_forced_values_float_100_smallest_subnormal() {
    forced_float_roundtrip(
        f64::NEG_INFINITY,
        f64::INFINITY,
        true,
        true,
        100.0 * SMALLEST_SUBNORMAL,
    );
}

#[test]
fn test_forced_values_float_nan() {
    forced_float_roundtrip(f64::NEG_INFINITY, f64::INFINITY, true, true, f64::NAN);
}

#[test]
fn test_forced_values_float_neg_nan() {
    // Python's `-math.nan` → NaN with sign bit set. Use `copysign` to get the
    // same bit pattern deterministically (Rust's `-f64::NAN` has
    // implementation-defined sign).
    let neg_nan = f64::NAN.copysign(-1.0);
    forced_float_roundtrip(f64::NEG_INFINITY, f64::INFINITY, true, true, neg_nan);
}

#[test]
fn test_forced_values_float_signaling_nan() {
    forced_float_roundtrip(f64::NEG_INFINITY, f64::INFINITY, true, true, SIGNALING_NAN);
}

#[test]
fn test_forced_values_float_neg_signaling_nan() {
    let neg_snan = SIGNALING_NAN.copysign(-1.0);
    forced_float_roundtrip(f64::NEG_INFINITY, f64::INFINITY, true, true, neg_snan);
}

#[test]
fn test_forced_values_float_pos_infinity() {
    // Python's `1e999` evaluates to `+inf`.
    forced_float_roundtrip(f64::NEG_INFINITY, f64::INFINITY, true, true, f64::INFINITY);
}

#[test]
fn test_forced_values_float_neg_infinity() {
    // Python's `-1e999` evaluates to `-inf`.
    forced_float_roundtrip(
        f64::NEG_INFINITY,
        f64::INFINITY,
        true,
        true,
        f64::NEG_INFINITY,
    );
}

#[test]
fn test_forced_values_float_nan_with_degenerate_bounds() {
    // Regression: previously errored on our `{pos,neg}_clamper` logic not
    // considering NaN.  Forced NaN with `min_value == max_value == -inf`.
    forced_float_roundtrip(f64::NEG_INFINITY, f64::NEG_INFINITY, true, true, f64::NAN);
}

// -- test_forced_floats_with_nan -----------------------------------------
//
// Python: @pytest.mark.parametrize over `(sign, min_value, max_value)`.
// The test asserts only that `draw_float(..., forced=sign * nan)` does not
// error — it's a regression for float-clamper construction with a
// sign-opposite NaN.

fn forced_float_nan(sign: f64, min_value: f64, max_value: f64) {
    let forced = f64::NAN.copysign(sign);
    let mut ntc = fresh();
    let drawn = ntc
        .draw_float_forced(min_value, max_value, true, true, forced)
        .ok()
        .unwrap();
    assert!(drawn.is_nan());
}

#[test]
fn test_forced_floats_with_nan_pos_pos_zero_zero() {
    forced_float_nan(1.0, 0.0, 0.0);
}

#[test]
fn test_forced_floats_with_nan_neg_pos_zero_zero() {
    forced_float_nan(-1.0, 0.0, 0.0);
}

#[test]
fn test_forced_floats_with_nan_pos_neg_zero_neg_zero() {
    forced_float_nan(1.0, -0.0, -0.0);
}

#[test]
fn test_forced_floats_with_nan_neg_neg_zero_neg_zero() {
    forced_float_nan(-1.0, -0.0, -0.0);
}

#[test]
fn test_forced_floats_with_nan_pos_zero_hundred() {
    forced_float_nan(1.0, 0.0, 100.0);
}

#[test]
fn test_forced_floats_with_nan_neg_zero_hundred() {
    forced_float_nan(-1.0, 0.0, 100.0);
}

#[test]
fn test_forced_floats_with_nan_pos_neg_hundred_neg_zero() {
    forced_float_nan(1.0, -100.0, -0.0);
}

#[test]
fn test_forced_floats_with_nan_neg_neg_hundred_neg_zero() {
    forced_float_nan(-1.0, -100.0, -0.0);
}

#[test]
fn test_forced_floats_with_nan_pos_five_ten() {
    forced_float_nan(1.0, 5.0, 10.0);
}

#[test]
fn test_forced_floats_with_nan_neg_five_ten() {
    forced_float_nan(-1.0, 5.0, 10.0);
}

#[test]
fn test_forced_floats_with_nan_pos_neg_ten_neg_five() {
    forced_float_nan(1.0, -10.0, -5.0);
}

#[test]
fn test_forced_floats_with_nan_neg_neg_ten_neg_five() {
    forced_float_nan(-1.0, -10.0, -5.0);
}

// -- Extra roundtrip coverage for integer / bytes / string forcing --------
//
// The Python test only parameterizes these via `choice_types_constraints`,
// which we skip (see module docstring). These direct tests exercise the same
// forced → replay invariant for the three remaining choice types.

#[test]
fn test_forced_integer_roundtrip() {
    let mut ntc = fresh();
    let v = ntc.draw_integer_forced(-1000, 1000, 42).ok().unwrap();
    assert_eq!(v, 42);

    let choices = choices_of(&ntc);
    let mut replay = NativeTestCase::for_choices(&choices, None);
    assert_eq!(replay.draw_integer(-1000, 1000).ok().unwrap(), 42);
}

#[test]
fn test_forced_integer_boundary_roundtrip() {
    let mut ntc = fresh();
    let v = ntc
        .draw_integer_forced(i128::MIN, i128::MAX, i128::MAX)
        .ok()
        .unwrap();
    assert_eq!(v, i128::MAX);

    let choices = choices_of(&ntc);
    let mut replay = NativeTestCase::for_choices(&choices, None);
    assert_eq!(
        replay.draw_integer(i128::MIN, i128::MAX).ok().unwrap(),
        i128::MAX
    );
}

#[test]
fn test_forced_bytes_roundtrip() {
    let forced = b"hello".to_vec();
    let mut ntc = fresh();
    let v = ntc.draw_bytes_forced(0, 16, forced.clone()).ok().unwrap();
    assert_eq!(v, forced);

    let choices = choices_of(&ntc);
    let mut replay = NativeTestCase::for_choices(&choices, None);
    assert_eq!(replay.draw_bytes(0, 16).ok().unwrap(), forced);
}

#[test]
fn test_forced_string_roundtrip() {
    let mut ntc = fresh();
    let v = ntc
        .draw_string_forced(0, 0x10FFFF, 0, 16, "héllo")
        .ok()
        .unwrap();
    assert_eq!(v, "héllo");

    let choices = choices_of(&ntc);
    let mut replay = NativeTestCase::for_choices(&choices, None);
    assert_eq!(
        replay.draw_string(0, 0x10FFFF, 0, 16).ok().unwrap(),
        "héllo"
    );
}

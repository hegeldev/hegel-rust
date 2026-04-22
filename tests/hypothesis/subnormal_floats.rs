//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_subnormal_floats.py.
//!
//! Individually-skipped tests (rest of the file is ported):
//!
//! - `test_subnormal_validation` — uses `floats(allow_subnormal=True)`, a
//!   public-API kwarg with no hegel-rust counterpart on `gs::floats()`.
//! - `test_allow_subnormal_defaults_correctly` — same reason; depends on
//!   `floats(allow_subnormal=...)` to gate subnormal generation.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{next_down_normal, next_up_normal};

#[test]
fn test_next_float_normal() {
    assert_eq!(next_up_normal(-f64::MIN_POSITIVE, false), -0.0);
    assert_eq!(next_up_normal(0.0, false), f64::MIN_POSITIVE);
    assert_eq!(next_down_normal(f64::MIN_POSITIVE, false), 0.0);
    assert_eq!(next_down_normal(-0.0, false), -f64::MIN_POSITIVE);
}

// Witness for the pass-through branch of `next_down_normal`/`next_up_normal`:
// when `allow_subnormal` is true, or when the result is already in the normal
// range, the function returns `next_down`/`next_up` unmodified. The upstream
// `test_next_float_normal` only covers the subnormal-rounding case, so without
// this Python coverage doesn't flag the gap but Rust's ratchet does.
#[test]
fn test_next_float_normal_passthrough() {
    assert_eq!(next_down_normal(2.0, false), 2.0_f64.next_down());
    assert_eq!(next_up_normal(2.0, false), 2.0_f64.next_up());
    assert_eq!(
        next_down_normal(f64::MIN_POSITIVE, true),
        f64::MIN_POSITIVE.next_down()
    );
}

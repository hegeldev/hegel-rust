//! From hegel-zoo `rust/ordered-float`, bug ordered-float/1, tests
//! `f64_::not_nan_fract_and_mul_add_never_hold_nan` and `f32_::…`.
//!
//! `NotNan::mul_add(a, b, c)` returns a `NotNan` holding NaN when `a·b` is `0 · ∞`; `c` plays no
//! part. The test draws `a`, `b`, `c` from the zoo's `arb_f64` — a 40% coin for a table of
//! special values (zeros, infinities, NaNs with payloads, MAX/MIN/EPSILON, small integers), else
//! any float — and assumes none is NaN. Shortlex ideal `(0.0, ∞, 0.0)`.
//!
//! The zoo saw `c` left at `2.29e16`, `f32::MAX` or `-7.1e15`: a draw the failure does not depend
//! on, left at a large integer-valued float where `0.0` reproduces. Below 2^56 an integer-valued
//! float's lex index is the integer itself, so a pass that steps the index one unit at a time and
//! is re-run while it improves can spend the whole shrink budget on it.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

fn arb_f64(tc: &TestCase) -> f64 {
    if tc.draw_silent(gs::weighted_booleans(0.4)) {
        match tc.draw_silent(gs::integers::<u8>().max_value(15)) {
            0 => 0.0,
            1 => -0.0,
            2 => f64::INFINITY,
            3 => f64::NEG_INFINITY,
            4 | 5 => {
                let payload = tc.draw_silent(gs::integers::<u64>());
                let nan = f64::from_bits(payload | 0x7ff0_0000_0000_0000);
                if nan.is_nan() { nan } else { f64::NAN }
            }
            6 => f64::MAX,
            7 => f64::MIN,
            8 => f64::MIN_POSITIVE,
            9 => f64::EPSILON,
            10 => 1.0,
            11 => -1.0,
            12 => 0.5,
            13 => 2.0,
            14 => 1e-45,
            _ => tc.draw_silent(gs::integers::<i16>()) as f64,
        }
    } else {
        tc.draw_silent(gs::floats::<f64>())
    }
}

#[derive(Debug)]
struct Case {
    a: f64,
    b: f64,
    c: f64,
}

fn draw(tc: &TestCase) -> Case {
    Case {
        a: arb_f64(tc),
        b: arb_f64(tc),
        c: arb_f64(tc),
    }
}

fn a_real_method_yields_nan(c: &Case) -> bool {
    if c.a.is_nan() || c.b.is_nan() || c.c.is_nan() {
        return false;
    }
    c.a.fract().is_nan() || c.a.mul_add(c.b, c.c).is_nan()
}

fn ideal() -> Case {
    Case {
        a: 0.0,
        b: f64::INFINITY,
        c: 0.0,
    }
}

#[test]
fn the_ideal_does_fail() {
    assert!(a_real_method_yields_nan(&ideal()));
}

#[test]
fn irrelevant_float_draw_is_zeroed() {
    assert_shrinks_to(&ideal(), 30, 100, draw, a_real_method_yields_nan);
}

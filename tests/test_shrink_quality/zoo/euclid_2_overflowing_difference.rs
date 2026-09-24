//! From hegel-zoo `rust/euclid`, bug euclid/2, test
//! `angle::float::hegel_props::hegel_angle_to_is_finite_for_finite_angles`.
//!
//! `Angle::angle_to` computes `(to - from) % 2π`; when the subtraction overflows to `±inf` the
//! remainder is NaN. Each angle is drawn from
//! `one_of!(any finite f64, [1e308, MAX], [-MAX, -1e308])`. The failing set is
//! `|to - from| > f64::MAX` after rounding: a coupled pair of magnitudes, where shrinking one
//! further needs the other's magnitude raised.
//!
//! Shortlex ideal: both from the first branch, `from = 2^970` (half an ulp of `MAX`, the
//! smallest positive float whose addition to `f64::MAX` rounds up to infinity) and
//! `to = -f64::MAX`. The float passes leave `to` a few ulps short of `-MAX` with `from` far
//! above `2^970`, or the pair with the signs the wrong way round; `scale_numeric_pairs` puts
//! `to` at the end of its range (and mirrors the signs) while scaling `from` down, and the
//! bisection then finishes `from`. A human would write `from = 1e308, to = -1e308`.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

#[derive(Debug)]
struct Case {
    from: f64,
    to: f64,
}

fn extreme_angle(tc: &TestCase) -> f64 {
    match tc.draw_silent(gs::integers::<u8>().min_value(0).max_value(2)) {
        0 => tc.draw_silent(gs::floats::<f64>().allow_nan(false).allow_infinity(false)),
        1 => tc.draw_silent(gs::floats::<f64>().min_value(1e308).max_value(f64::MAX)),
        _ => tc.draw_silent(gs::floats::<f64>().min_value(-f64::MAX).max_value(-1e308)),
    }
}

fn draw(tc: &TestCase) -> Case {
    let from = extreme_angle(tc);
    let to = extreme_angle(tc);
    Case { from, to }
}

fn angle_to_is_not_finite(c: &Case) -> bool {
    let max = std::f64::consts::PI * 2.0;
    let d = (c.to - c.from) % max;
    !(2.0 * d % max - d).is_finite()
}

fn ideal() -> Case {
    Case {
        from: 2f64.powi(970),
        to: -f64::MAX,
    }
}

#[test]
fn the_ideal_is_the_smallest() {
    assert!(angle_to_is_not_finite(&ideal()));
    assert!(angle_to_is_not_finite(&Case {
        from: 1e308,
        to: -1e308,
    }));
    let below = f64::from_bits(2f64.powi(970).to_bits() - 1);
    assert!(!angle_to_is_not_finite(&Case {
        from: below,
        to: -f64::MAX,
    }));
}

#[test]
fn coupled_magnitudes_shrink_to_the_half_ulp_boundary() {
    assert_shrinks_to(&ideal(), 20, 300, draw, angle_to_is_not_finite);
}

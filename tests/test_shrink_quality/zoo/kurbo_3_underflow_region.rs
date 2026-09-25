//! From hegel-zoo `rust/kurbo`, bug kurbo/3, test `arc_from_svg_arc_underflow_breaks_internal_assertion`.
//!
//! `Arc::from_svg_arc` asserts `sum_of_sq != 0.0` where `sum_of_sq = (rx·py)² + (ry·px)²` and
//! `(px, py)` is the rotated half-difference of the endpoints. With `from` drawn from
//! `floats().min_value(-1e-165).max_value(1e-165)` (both coordinates), `to = origin`, radii in
//! `[1e-4, 1e3]`, the products are at most `5e-163` and square to `0`: every `from ≠ origin` in
//! the range fails.
//!
//! Shortlex ideal: `from = (0, 2^-549)`, radii `(1, 1)`, rotation `0`, flags `false`. Hegel
//! orders floats by Hypothesis's lex index: integers below 2^56 first, then by exponent distance
//! from 1.0, then by bit-reversed mantissa. No integer fits in `(0, 1e-165]`, so the simplest
//! value is the one with the largest exponent in range and a zero mantissa, `2^-549`.
//! `smallest_nonzero_tiny_draw` is the case reduced to that one draw: inside a bounded range
//! every index below the ideal's decodes out of range and above it in- and out-of-range values
//! interleave, so a bisection over the index is not monotone. A human would write
//! `from = (0, 1e-170)`.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

#[derive(Debug)]
#[allow(dead_code)]
struct Arc {
    from: (f64, f64),
    radii: (f64, f64),
    rotation: f64,
    large_arc: bool,
    sweep: bool,
}

fn tiny(tc: &TestCase) -> f64 {
    tc.draw_silent(gs::floats::<f64>().min_value(-1e-165).max_value(1e-165))
}

fn draw(tc: &TestCase) -> Arc {
    let from = (tiny(tc), tiny(tc));
    let radius = || gs::floats::<f64>().min_value(1e-4).max_value(1e3);
    Arc {
        from,
        radii: (tc.draw_silent(radius()), tc.draw_silent(radius())),
        rotation: tc.draw_silent(gs::floats::<f64>().min_value(-10.0).max_value(10.0)),
        large_arc: tc.draw_silent(gs::booleans()),
        sweep: tc.draw_silent(gs::booleans()),
    }
}

fn assertion_trips(a: &Arc) -> bool {
    if a.from == (0.0, 0.0) {
        return false;
    }
    if a.radii.0.abs() <= 1e-5 || a.radii.1.abs() <= 1e-5 {
        return false;
    }
    let (sin_phi, cos_phi) = (a.rotation % (2.0 * std::f64::consts::PI)).sin_cos();
    let hd_x = (a.from.0 - 0.0) * 0.5;
    let hd_y = (a.from.1 - 0.0) * 0.5;
    let px = cos_phi * hd_x + sin_phi * hd_y;
    let py = -sin_phi * hd_x + cos_phi * hd_y;
    (a.radii.0.abs() * py).powi(2) + (a.radii.1.abs() * px).powi(2) == 0.0
}

const SMALLEST_NONZERO_TINY: f64 = 5.426657103235053e-166;

fn ideal() -> Arc {
    Arc {
        from: (0.0, SMALLEST_NONZERO_TINY),
        radii: (1.0, 1.0),
        rotation: 0.0,
        large_arc: false,
        sweep: false,
    }
}

#[test]
fn the_ideal_does_fail() {
    assert_eq!(SMALLEST_NONZERO_TINY, 2f64.powi(-549));
    assert!(assertion_trips(&ideal()));
    assert!(assertion_trips(&Arc {
        from: (0.0, 1e-170),
        ..ideal()
    }));
    assert!(assertion_trips(&Arc {
        from: (0.0, 1e-165),
        radii: (1e3, 1e3),
        rotation: 1.0,
        ..ideal()
    }));
    assert!(!assertion_trips(&Arc {
        from: (0.0, 0.0),
        ..ideal()
    }));
}

#[test]
fn smallest_nonzero_tiny_draw() {
    assert_shrinks_to(&SMALLEST_NONZERO_TINY, 40, 100, tiny, |x: &f64| *x != 0.0);
}

#[test]
fn whole_domain_failure_shrinks_to_the_edge() {
    assert_shrinks_to(&ideal(), 20, 100, draw, assertion_trips);
}

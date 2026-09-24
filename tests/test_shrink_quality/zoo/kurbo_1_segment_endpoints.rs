//! From hegel-zoo `rust/kurbo`, bug kurbo/1, test
//! `bezpath::tests::pathseg_eval_at_endpoints_matches_start_and_end`.
//!
//! `Line::eval(t) = p0 + t·(p1 − p0)`: when `p1 − p0` overflows, `eval(1.0)` is `±inf` and
//! `eval(0.0)` is NaN, neither equal to the control point. Quads and cubics overflow similarly
//! in their intermediate products. Each coordinate is
//! `one_of!(any finite f64, sampled_from([MAX, -MAX, 1e308, -1e308, 6.5e307]))`.
//!
//! Shortlex ideal: a line (kind 0) with the overflow on the *y* axis so the earlier x draws stay
//! zero: `p0 = (0, 2^970)`, `p1 = (0, −MAX)` — the euclid/2 pair, which `scale_numeric_pairs`
//! now finishes. What remains is the x-axis mirror, `p0 = (2^970, 0)`, `p1 = (−MAX, 0)`: moving
//! the overflow to the y axis changes all four coordinates at once, and a quad with both `p1`
//! coordinates at `MAX / 2` where one would do. A human would write
//! `Line((0, 1e308), (0, −1e308))`.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

#[derive(Clone, Copy, Debug)]
struct P(f64, f64);

impl std::ops::Add for P {
    type Output = P;
    fn add(self, o: P) -> P {
        P(self.0 + o.0, self.1 + o.1)
    }
}

impl std::ops::Sub for P {
    type Output = P;
    fn sub(self, o: P) -> P {
        P(self.0 - o.0, self.1 - o.1)
    }
}

impl std::ops::Mul<f64> for P {
    type Output = P;
    fn mul(self, k: f64) -> P {
        P(self.0 * k, self.1 * k)
    }
}

#[derive(Debug)]
enum Seg {
    Line(P, P),
    Quad(P, P, P),
    Cubic(P, P, P, P),
}

impl Seg {
    fn eval(&self, t: f64) -> P {
        let mt = 1.0 - t;
        match *self {
            Seg::Line(p0, p1) => p0 + (p1 - p0) * t,
            Seg::Quad(p0, p1, p2) => p0 * (mt * mt) + (p1 * (mt * 2.0) + p2 * t) * t,
            Seg::Cubic(p0, p1, p2, p3) => {
                p0 * (mt * mt * mt) + (p1 * (mt * mt * 3.0) + (p2 * (mt * 3.0) + p3 * t) * t) * t
            }
        }
    }

    fn start(&self) -> P {
        match *self {
            Seg::Line(p, _) | Seg::Quad(p, _, _) | Seg::Cubic(p, _, _, _) => p,
        }
    }

    fn end(&self) -> P {
        match *self {
            Seg::Line(_, p) | Seg::Quad(_, _, p) | Seg::Cubic(_, _, _, p) => p,
        }
    }

    fn control_points(&self) -> Vec<P> {
        match *self {
            Seg::Line(a, b) => vec![a, b],
            Seg::Quad(a, b, c) => vec![a, b, c],
            Seg::Cubic(a, b, c, d) => vec![a, b, c, d],
        }
    }
}

fn coord(tc: &TestCase) -> f64 {
    if tc.draw_silent(gs::integers::<u8>().min_value(0).max_value(1)) == 0 {
        tc.draw_silent(gs::floats::<f64>().allow_nan(false).allow_infinity(false))
    } else {
        tc.draw_silent(gs::sampled_from(vec![
            f64::MAX,
            -f64::MAX,
            1e308,
            -1e308,
            6.5e307,
        ]))
    }
}

fn point(tc: &TestCase) -> P {
    P(coord(tc), coord(tc))
}

fn draw(tc: &TestCase) -> Seg {
    match tc.draw_silent(gs::integers::<u8>().max_value(2)) {
        0 => Seg::Line(point(tc), point(tc)),
        1 => Seg::Quad(point(tc), point(tc), point(tc)),
        _ => Seg::Cubic(point(tc), point(tc), point(tc), point(tc)),
    }
}

fn endpoints_disagree(seg: &Seg) -> bool {
    let scale = seg
        .control_points()
        .iter()
        .map(|p| p.0.abs().max(p.1.abs()))
        .fold(1.0, f64::max);
    let eps = 1e-15 * scale;
    let off = |d: P| {
        let err = d.0.abs().max(d.1.abs());
        err.is_nan() || err > eps
    };
    off(seg.eval(0.0) - seg.start()) || off(seg.eval(1.0) - seg.end())
}

fn ideal() -> Seg {
    Seg::Line(P(0.0, 2f64.powi(970)), P(0.0, -f64::MAX))
}

#[test]
fn the_ideal_does_fail() {
    assert!(endpoints_disagree(&ideal()));
    assert!(endpoints_disagree(&Seg::Line(
        P(0.0, 1e308),
        P(0.0, -1e308)
    )));
    let below = f64::from_bits(2f64.powi(970).to_bits() - 1);
    assert!(!endpoints_disagree(&Seg::Line(
        P(0.0, below),
        P(0.0, -f64::MAX)
    )));
}

#[test]
#[ignore = "shrinker: no pass moves a coupled pair from the x coordinates to the y coordinates"]
fn overflowing_segment_shrinks_to_one_axis() {
    assert_shrinks_to(&ideal(), 20, 300, draw, endpoints_disagree);
}

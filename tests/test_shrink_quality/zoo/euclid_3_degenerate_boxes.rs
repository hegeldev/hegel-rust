//! From hegel-zoo `rust/euclid`, bug euclid/3, test
//! `box2d::tests::hegel_props::hegel_intersects_iff_intersection_is_some`.
//!
//! `Box2D::intersects` uses strict inequalities on both axes while `Box2D::intersection` returns
//! `None` when the computed box is empty (zero width *or* height), so a degenerate box crossing
//! another box "intersects" without having an intersection.
//!
//! Draws per box: `x0`, `y0`, `bool` (x1 = x0?), `[x1]`, `bool` (y1 = y0?), `[y1]`, all
//! coordinates `i32`. Shortlex ideal, 10 draws: `a = (0,0)-(2,2)` (both booleans `false`, the
//! simpler value) and `b` the point `(1,1)` (both `true`), strictly inside `a`: `intersects` is
//! true, the intersection is the empty box `(1,1)-(1,1)`. The workbench's `a = (0,0)-(2,0)`,
//! `b = (1,-1)-(1,1)` also fails in 10 draws but has `true` at its fifth draw where the ideal
//! has `false`, so it is the larger of the two; the shrinker ends there from most seeds, and
//! getting from it to the ideal means flipping that boolean (which draws `a.y1`) while
//! dropping `b.y1` — a shape change no pass proposes. A human would write either pair.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

#[derive(Clone, Copy, Debug)]
struct Box2D {
    min: (i64, i64),
    max: (i64, i64),
}

impl Box2D {
    fn new(p0: (i64, i64), p1: (i64, i64)) -> Box2D {
        Box2D {
            min: (p0.0.min(p1.0), p0.1.min(p1.1)),
            max: (p0.0.max(p1.0), p0.1.max(p1.1)),
        }
    }

    fn is_empty(&self) -> bool {
        !(self.max.0 > self.min.0 && self.max.1 > self.min.1)
    }

    fn intersects(&self, o: &Box2D) -> bool {
        self.min.0 < o.max.0 && self.max.0 > o.min.0 && self.min.1 < o.max.1 && self.max.1 > o.min.1
    }

    fn intersection(&self, o: &Box2D) -> Option<Box2D> {
        let b = Box2D {
            min: (self.min.0.max(o.min.0), self.min.1.max(o.min.1)),
            max: (self.max.0.min(o.max.0), self.max.1.min(o.max.1)),
        };
        (!b.is_empty()).then_some(b)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct Case {
    a: Box2D,
    b: Box2D,
    /// How many choices the two boxes took: the same boxes through more draws is a worse example.
    draws: u32,
}

fn draw_coord(tc: &TestCase, n: &mut u32) -> i64 {
    *n += 1;
    tc.draw_silent(gs::integers::<i32>()) as i64
}

fn draw_box(tc: &TestCase, n: &mut u32) -> Box2D {
    let x0 = draw_coord(tc, n);
    let y0 = draw_coord(tc, n);
    *n += 1;
    let x1 = if tc.draw_silent(gs::booleans()) {
        x0
    } else {
        draw_coord(tc, n)
    };
    *n += 1;
    let y1 = if tc.draw_silent(gs::booleans()) {
        y0
    } else {
        draw_coord(tc, n)
    };
    Box2D::new((x0, y0), (x1, y1))
}

fn draw(tc: &TestCase) -> Case {
    let mut n = 0;
    let a = draw_box(tc, &mut n);
    let b = draw_box(tc, &mut n);
    Case { a, b, draws: n }
}

fn intersects_disagrees_with_intersection(c: &Case) -> bool {
    c.a.intersects(&c.b) != c.a.intersection(&c.b).is_some()
}

fn ideal() -> Case {
    Case {
        a: Box2D::new((0, 0), (2, 2)),
        b: Box2D::new((1, 1), (1, 1)),
        draws: 10,
    }
}

#[test]
fn the_ideal_does_fail() {
    assert!(intersects_disagrees_with_intersection(&ideal()));
    assert!(intersects_disagrees_with_intersection(&Case {
        a: Box2D::new((0, 0), (2, 0)),
        b: Box2D::new((1, 1), (1, -1)),
        draws: 10,
    }));
}

#[test]
#[ignore = "shrinker: no pass flips a boolean that adds a draw while deleting a later one"]
fn crossing_degenerate_boxes_shrink_to_the_origin() {
    assert_shrinks_to(
        &ideal(),
        20,
        300,
        draw,
        intersects_disagrees_with_intersection,
    );
}

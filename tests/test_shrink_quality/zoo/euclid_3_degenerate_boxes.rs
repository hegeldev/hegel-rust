//! From hegel-zoo `rust/euclid`, bug euclid/3, test
//! `box2d::tests::hegel_props::hegel_intersects_iff_intersection_is_some`.
//!
//! `Box2D::intersects` uses strict inequalities on both axes while `Box2D::intersection` returns
//! `None` when the computed box is empty (zero width *or* height), so a degenerate box crossing
//! another box "intersects" without having an intersection.
//!
//! Draws per box: `x0`, `y0`, `bool` (x1 = x0?), `[x1]`, `bool` (y1 = y0?), `[y1]`, all
//! coordinates `i32`. Shortlex ideal, 10 draws: `a = (0,0)-(2,0)` (zero height; x1 = 2 so that
//! x = 1 lies strictly inside) and `b = (1,-1)-(1,1)` (zero width, straddling y = 0). The zoo
//! also saw the same boxes through an 11th, redundant draw, and coordinates in the thousands
//! where one coordinate must move with another to keep the crossing. A human would write the
//! same boxes.

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
        a: Box2D::new((0, 0), (2, 0)),
        b: Box2D::new((1, 1), (1, -1)),
        draws: 10,
    }
}

#[test]
fn the_ideal_does_fail() {
    assert!(intersects_disagrees_with_intersection(&ideal()));
}

#[test]
#[ignore = "shrinker: no pass lowers one draw while raising another along a product bound"]
fn crossing_degenerate_boxes_shrink_to_the_origin() {
    assert_shrinks_to(
        &ideal(),
        20,
        300,
        draw,
        intersects_disagrees_with_intersection,
    );
}

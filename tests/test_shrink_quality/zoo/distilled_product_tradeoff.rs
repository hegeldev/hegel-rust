//! Distilled from chrono/5, chrono/6, euclid/2 and kurbo/1: two draws traded against each other
//! along a product bound.
//!
//! `a, k ∈ [0, 1000]`; the property fails iff `a × k ≥ 1000`. Shortlex ideal `[1, 1000]`: the
//! smallest first draw that can fail at all, with the second raised to make it fail. Lowering
//! either draw alone leaves the failing set, so the shrinker stops on some point of the
//! hyperbola (`[2, 500]`, `[4, 250]`, `[8, 125]`, …) unless a pass lowers `a` and raises `k` in
//! one move. The additive bound `a + k ≥ 1000` is the control: `redistribute_integers` handles
//! it. A human would write `1 × 1000` too, or `32 × 32`.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

fn draw(tc: &TestCase) -> Vec<i64> {
    let r = || gs::integers::<i64>().min_value(0).max_value(1000);
    vec![tc.draw_silent(r()), tc.draw_silent(r())]
}

fn product_too_big(d: &[i64]) -> bool {
    d[0] * d[1] >= 1000
}

fn sum_too_big(d: &[i64]) -> bool {
    d[0] + d[1] >= 1000
}

#[test]
fn the_ideal_is_the_smallest() {
    assert!(product_too_big(&[1, 1000]));
    assert!(!product_too_big(&[1, 999]));
    assert!(!product_too_big(&[0, 1000]));
    assert!(product_too_big(&[2, 500]) && !product_too_big(&[2, 499]));
}

#[test]
fn product_bound_shrinks_to_one_times_thousand() {
    assert_shrinks_to(&vec![1, 1000], 30, 100, draw, |d| product_too_big(d));
}

#[test]
fn sum_bound_is_handled() {
    assert_shrinks_to(&vec![0, 1000], 30, 100, draw, |d| sum_too_big(d));
}

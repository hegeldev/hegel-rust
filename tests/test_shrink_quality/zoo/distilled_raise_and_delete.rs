//! Distilled from jsonparser/9, format_num/1 and sprintf/13: a gate that must be *raised* so
//! that the draw behind it disappears, with nothing else in the way.
//!
//! `coin ∈ [0, 99]`; if `coin < 50` a `pick ∈ [0, 2]` is drawn; then `value ∈ [-1000, 1000]`.
//! The property always fails. Shortlex ideal `[50, 0]`: coin 50, no pick, value 0 — two draws,
//! where the all-minimal first test case `[0, 0, 0]` has three. A human would write the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

fn n(tc: &TestCase, lo: i64, hi: i64) -> i64 {
    tc.draw_silent(gs::integers::<i64>().min_value(lo).max_value(hi))
}

fn draw(tc: &TestCase) -> Vec<i64> {
    let coin = n(tc, 0, 99);
    let mut draws = vec![coin];
    if coin < 50 {
        draws.push(n(tc, 0, 2));
    }
    draws.push(n(tc, -1000, 1000));
    draws
}

#[test]
fn gate_is_raised_to_drop_the_pick() {
    assert_shrinks_to(&vec![50, 0], 30, 100, draw, |_| true);
}

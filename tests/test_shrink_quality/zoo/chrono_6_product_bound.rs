//! From hegel-zoo `rust/chrono`, bug chrono/6, test `multiplication_beyond_the_range_is_none`.
//!
//! `TimeDelta::checked_mul(k)` only guards the product's seconds against `i64`, not against
//! `TimeDelta::MAX` (= `i64::MAX` milliseconds), so a product between `MAX` and `i64::MAX`
//! seconds comes back `Some`. The failing set is `|a| · |k| > MAX` with `a·k` still inside `i64`
//! seconds: lowering `a` requires raising `k` and vice versa.
//!
//! Draws: `arb_delta` (a five-way branch, then that branch's values), then `k ∈ -1000..=1000`.
//! Shortlex ideal: branch 2 (`try_milliseconds`), the smallest ms whose `× 1000` exceeds `MAX`,
//! `ms = MAX.secs + 1 = 9_223_372_036_854_776`, `k = 1000`. `scale_numeric_pairs` takes `k` to
//! `1000` (mirroring a negative `a` on the way) while scaling `a` down to keep the product, and
//! the bisection then finishes `a`. A human would write `MAX × 2` (branch 4's table has `MAX` at
//! index 2, but its first draw is 4 > 2).

use super::assert_shrinks_to;
use super::chrono_delta::{TimeDelta, arb_delta};
use hegel::TestCase;
use hegel::generators as gs;

#[derive(Debug)]
struct Case {
    a: TimeDelta,
    k: i32,
}

fn draw(tc: &TestCase) -> Case {
    let a = arb_delta(tc);
    let k = tc.draw_silent(gs::integers::<i32>().min_value(-1000).max_value(1000));
    Case { a, k }
}

fn checked_mul_disagrees_with_range(c: &Case) -> bool {
    let product = c.a.total_nanos() * c.k as i128;
    let in_range =
        product >= TimeDelta::MIN.total_nanos() && product <= TimeDelta::MAX.total_nanos();
    match c.a.checked_mul(c.k) {
        Some(m) => !in_range || m < TimeDelta::MIN || m > TimeDelta::MAX,
        None => in_range,
    }
}

fn ideal() -> Case {
    Case {
        a: TimeDelta::try_milliseconds(9_223_372_036_854_776).unwrap(),
        k: 1000,
    }
}

#[test]
fn the_ideals_do_fail() {
    assert!(checked_mul_disagrees_with_range(&ideal()));
    assert!(checked_mul_disagrees_with_range(&Case {
        a: TimeDelta::MAX,
        k: 2,
    }));
    let below = Case {
        a: TimeDelta::try_milliseconds(9_223_372_036_854_775).unwrap(),
        k: 1000,
    };
    assert!(!checked_mul_disagrees_with_range(&below));
}

#[test]
fn a_and_k_must_be_traded_against_each_other() {
    assert_shrinks_to(&ideal(), 20, 100, draw, checked_mul_disagrees_with_range);
}

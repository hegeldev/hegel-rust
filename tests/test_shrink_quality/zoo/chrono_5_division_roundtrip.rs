//! From hegel-zoo `rust/chrono`, bug chrono/5, test `division_is_exact_to_the_nanosecond`.
//!
//! `TimeDelta::checked_div(k)` floors the carried seconds' share (`carry·1e9 / k`) and the
//! nanoseconds' share (`nanos / k`) separately, so it is one nanosecond short whenever the two
//! fractional parts add up to at least one. The zoo test checks `a / k` against exact arithmetic
//! and, when `a × k` is exact, that `(a × k) / k == a`; the failures are all in the round trip,
//! and need `a × k ≥ 1 s` so that there is a carry. Lowering `a` alone or `k` alone almost
//! always leaves the failing set.
//!
//! Draws: `arb_delta` (five-way branch, then values), then `k ∈ 1..=1000`; `a` is taken
//! `.abs()`. Shortlex ideal: branch 0 (`nanoseconds`), the smallest `n` with some failing
//! `k ≤ 1000`: `n = 1_001_002`, `k = 999` (checked by brute force in `the_ideal_is_the_smallest`).
//! A human would write `nanoseconds(333_333_334) × 3`.

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
    let k = tc.draw_silent(gs::integers::<i32>().min_value(1).max_value(1000));
    Case { a, k }
}

fn division_is_inexact(c: &Case) -> bool {
    let a = c.a.abs();
    let k = c.k;
    let na = a.total_nanos();
    let expected = TimeDelta::from_total_nanos(na / k as i128).unwrap();
    if a.checked_div(k) != Some(expected) {
        return true;
    }
    let exact = a
        .checked_mul(k)
        .filter(|m| m.total_nanos() == na * k as i128);
    matches!(exact, Some(m) if m.checked_div(k) != Some(a))
}

fn ideal() -> Case {
    Case {
        a: TimeDelta::nanoseconds(1_001_002),
        k: 999,
    }
}

#[test]
fn the_ideal_is_the_smallest() {
    assert!(division_is_inexact(&ideal()));
    assert!(division_is_inexact(&Case {
        a: TimeDelta::nanoseconds(333_333_334),
        k: 3,
    }));
    for n in 1_000_000..1_001_002 {
        for k in 1..=1000 {
            let c = Case {
                a: TimeDelta::nanoseconds(n),
                k,
            };
            assert!(!division_is_inexact(&c), "{n} × {k}");
        }
    }
    for k in 1..999 {
        let c = Case {
            a: TimeDelta::nanoseconds(1_001_002),
            k,
        };
        assert!(!division_is_inexact(&c), "{k}");
    }
}

#[test]
#[ignore = "shrinker: the failing (a, k) pairs along a × k ≥ 1 s are sparse; the product move lands between them"]
fn a_and_k_must_be_traded_against_each_other() {
    assert_shrinks_to(&ideal(), 20, 100, draw, division_is_inexact);
}

//! Distilled from hcl-rs/2 and hcl-rs/3: a `one_of` branch that must be *lowered* with a
//! non-minimal continuation.
//!
//! `branch ∈ [0, 1]`. Branch 1 draws `sampled_from([7, 8, 9])` and always fails. Branch 0 draws
//! `x ∈ [0, 1_000_000]` and fails iff `x ≥ T`. Shortlex ideal `[0, T]`: same length, smaller
//! first choice. Every seed's first failure is in branch 1, so the shrinker has to lower the
//! branch and land the regenerated `x` at or above `T`, then bisect it down to `T`. Two
//! thresholds: `T = 500_000` (half of all continuations do) and `T = 990_000` (one in a hundred).
//! A human would write the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

fn draw(tc: &TestCase) -> Vec<i64> {
    let branch = tc.draw_silent(gs::integers::<i64>().min_value(0).max_value(1));
    let second = if branch == 1 {
        tc.draw_silent(gs::sampled_from(vec![7i64, 8, 9]))
    } else {
        tc.draw_silent(gs::integers::<i64>().min_value(0).max_value(1_000_000))
    };
    vec![branch, second]
}

fn fails_from(t: i64) -> impl Fn(&Vec<i64>) -> bool + Send + Sync + 'static {
    move |d: &Vec<i64>| d[0] == 1 || d[1] >= t
}

#[test]
fn the_ideal_does_fail() {
    for t in [500_000, 990_000] {
        assert!(fails_from(t)(&vec![0, t]));
        assert!(!fails_from(t)(&vec![0, t - 1]));
        assert!(fails_from(t)(&vec![1, 7]));
    }
}

#[test]
fn lowered_branch_reaches_the_threshold_half() {
    assert_shrinks_to(&vec![0, 500_000], 30, 100, draw, fails_from(500_000));
}

#[test]
fn lowered_branch_reaches_the_threshold_rare() {
    assert_shrinks_to(&vec![0, 990_000], 30, 100, draw, fails_from(990_000));
}

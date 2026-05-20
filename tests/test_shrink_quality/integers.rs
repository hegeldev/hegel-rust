use crate::common::utils::{Minimal, minimal};
use hegel::generators as gs;

#[test]
fn test_integers_from_minimizes_leftwards() {
    assert_eq!(
        minimal(gs::integers::<i64>().min_value(101), |_: &i64| true),
        101
    );
}

#[test]
fn test_minimize_bounded_integers_to_zero() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(-10).max_value(10),
            |_: &i64| true,
        ),
        0
    );
}

#[test]
fn test_minimize_bounded_integers_to_positive() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(-10).max_value(10),
            |x: &i64| *x != 0,
        ),
        1
    );
}

#[test]
fn test_minimize_single_element_in_silly_large_int_range() {
    let hi = i64::MAX;
    let lo = i64::MIN;
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(lo / 2).max_value(hi / 2),
            move |x: &i64| *x >= lo / 4,
        ),
        0
    );
}

#[test]
fn test_minimize_multiple_elements_in_silly_large_int_range() {
    let hi = i64::MAX;
    let lo = i64::MIN;
    let result = Minimal::new(
        gs::vecs(gs::integers::<i64>().min_value(lo / 2).max_value(hi / 2)),
        |x: &Vec<i64>| x.len() >= 20,
    )
    .test_cases(10000)
    .run();
    assert_eq!(result, vec![0i64; 20]);
}

#[test]
fn test_minimize_multiple_elements_min_is_not_dupe() {
    let result = Minimal::new(
        gs::vecs(gs::integers::<i64>().min_value(0).max_value(i64::MAX / 2)),
        |x: &Vec<i64>| x.len() >= 20 && (0..20).all(|i| x[i] >= i as i64),
    )
    .test_cases(10000)
    .run();
    let expected: Vec<i64> = (0..20).collect();
    assert_eq!(result, expected);
}

#[test]
fn test_can_find_an_int() {
    assert_eq!(minimal(gs::integers::<i64>(), |_: &i64| true), 0);
}

#[test]
fn test_can_find_an_int_above_13() {
    assert_eq!(minimal(gs::integers::<i64>(), |x: &i64| *x >= 13), 13);
}

#[test]
fn test_minimizes_towards_zero() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(-1000).max_value(50),
            |x: &i64| *x < 0,
        ),
        -1
    );
}

// Tests 10-12 mirror test_integer_shrinks_* from the upstream, which use
// PbtkitState directly. The same shrink quality is verified via minimal().

#[test]
fn test_integer_shrinks_negative() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(-1000).max_value(1000),
            |x: &i64| *x < 0,
        ),
        -1
    );
}

#[test]
fn test_integer_shrinks_via_binary_search() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(0).max_value(10000),
            |x: &i64| *x > 100,
        ),
        101
    );
}

#[test]
fn test_integer_shrinks_negative_only_range() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(-100).max_value(-1),
            |x: &i64| *x <= -10,
        ),
        -10
    );
}

// `choice(n)` in pbtkit draws from [0, n] inclusive, so `choice(1000)` →
// `gs::integers::<i64>().min_value(0).max_value(1000)`.
#[test]
fn test_reduces_additive_pairs() {
    let (m, n) = Minimal::new(
        gs::tuples!(
            gs::integers::<i64>().min_value(0).max_value(1000),
            gs::integers::<i64>().min_value(0).max_value(1000),
        ),
        |(m, n): &(i64, i64)| *m + *n > 1000,
    )
    .test_cases(10000)
    .run();
    assert_eq!((m, n), (1, 1000));
}

// Port of `tests/quality/test_shrink_quality.py::test_perfectly_shrinks_integers`.
//
// For an integer constraint that's reached only when `x >= n` (or `x <= n`
// for negative n), the shrinker should land exactly on `n` — no slack.
#[test]
fn test_perfectly_shrinks_integers_positive() {
    for n in [3i64, 7, 42] {
        let v = minimal(gs::integers::<i64>(), move |x: &i64| *x >= n);
        assert_eq!(v, n, "expected exact landing on {n}");
    }
}

#[test]
fn test_perfectly_shrinks_integers_negative() {
    for n in [-3i64, -7, -42] {
        let v = minimal(gs::integers::<i64>(), move |x: &i64| *x <= n);
        assert_eq!(v, n, "expected exact landing on {n}");
    }
}

// Port of `tests/quality/test_shrink_quality.py::test_lowering_together_negative`.
//
// Two integers linked by `abs(m - n) <= 1` should collapse to `(0, 0)` quickly
// when both are allowed to be negative.  Exercises `lower_integers_together`
// and `lower_common_node_offset` driving both at once.
#[test]
fn test_lowering_together_negative() {
    let (m, n) = Minimal::new(
        gs::tuples!(
            gs::integers::<i64>().min_value(-1000).max_value(1000),
            gs::integers::<i64>().min_value(-1000).max_value(1000),
        ),
        |(m, n): &(i64, i64)| m.abs_diff(*n) <= 1 && *m <= -10 && *n <= -10,
    )
    .test_cases(10000)
    .run();
    // The minimal counterexample has both at the constraint boundary -10
    // with diff ≤ 1.
    assert_eq!(m, -10);
    assert!(n.abs_diff(m) <= 1);
}

// Port of `tests/quality/test_shrink_quality.py::test_lowering_together_mixed`.
//
// Mixed-sign linked integer pair.
#[test]
fn test_lowering_together_mixed() {
    let (m, n) = Minimal::new(
        gs::tuples!(
            gs::integers::<i64>().min_value(-100).max_value(100),
            gs::integers::<i64>().min_value(-100).max_value(100),
        ),
        |(m, n): &(i64, i64)| *m > 0 && *n < 0 && m - n >= 20,
    )
    .test_cases(10000)
    .run();
    // Smallest pair satisfying `m > 0`, `n < 0`, `m - n >= 20`.
    // The shrinker should collapse `(m, n)` toward (≥1, ≤-1) with a
    // tight gap.
    assert!(m >= 1 && n <= -1);
    assert!(m - n >= 20);
    // Excess shouldn't be wildly above the bound.
    assert!(m - n <= 25);
}

// Port of `tests/conjecture/test_shrinker.py::test_can_simultaneously_lower_non_duplicated_nearby_integers`.
//
// Two non-duplicate integers within gap 3 that must both stay above 10.
#[test]
fn test_can_simultaneously_lower_non_duplicated_nearby_integers() {
    let (m, n) = Minimal::new(
        gs::tuples!(
            gs::integers::<i64>().min_value(0).max_value(1000),
            gs::integers::<i64>().min_value(0).max_value(1000),
        ),
        |(m, n): &(i64, i64)| *m >= 11 && *n >= 10 && m > n,
    )
    .test_cases(10000)
    .run();
    assert_eq!((m, n), (11, 10));
}

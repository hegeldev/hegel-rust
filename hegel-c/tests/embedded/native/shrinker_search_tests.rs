//! Embedded tests for `src/native/shrinker/search.rs`.
//!
//! Each search object is checked against a reference implementation — the
//! closure-based searches these state machines replaced — for both the
//! final answer and the exact probe sequence. Probe-sequence equality
//! matters beyond correctness: every probe is a test-case execution, so a
//! divergent sequence would change shrink call counts and determinism.

use super::*;

fn reference_find_integer(mut f: impl FnMut(usize) -> bool, probes: &mut Vec<usize>) -> usize {
    let mut f = move |x: usize| {
        probes.push(x);
        f(x)
    };
    for i in 1..5 {
        if !f(i) {
            return i - 1;
        }
    }
    let mut lo = 4;
    let mut hi = 5;
    while f(hi) {
        lo = hi;
        let Some(next) = hi.checked_mul(2) else {
            return lo;
        };
        hi = next;
    }
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if f(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

fn drive_find_integer(mut f: impl FnMut(usize) -> bool, probes: &mut Vec<usize>) -> usize {
    let mut search = FindInteger::new();
    while let Some(k) = search.probe() {
        probes.push(k);
        search.record(f(k));
    }
    search.result()
}

#[test]
fn find_integer_matches_reference_on_monotone_thresholds() {
    for threshold in [0usize, 1, 2, 3, 4, 5, 6, 7, 9, 10, 100, 1000, 123_456] {
        let mut ref_probes = Vec::new();
        let mut new_probes = Vec::new();
        let expected = reference_find_integer(|k| k <= threshold, &mut ref_probes);
        let actual = drive_find_integer(|k| k <= threshold, &mut new_probes);
        assert_eq!(actual, expected, "threshold {threshold}");
        assert_eq!(actual, threshold, "threshold {threshold}");
        assert_eq!(new_probes, ref_probes, "threshold {threshold}");
    }
}

#[test]
fn find_integer_matches_reference_when_predicate_never_fails() {
    let mut ref_probes = Vec::new();
    let mut new_probes = Vec::new();
    let expected = reference_find_integer(|_| true, &mut ref_probes);
    let actual = drive_find_integer(|_| true, &mut new_probes);
    assert_eq!(actual, expected);
    assert_eq!(new_probes, ref_probes);
}

#[test]
fn find_integer_matches_reference_on_a_non_monotone_predicate() {
    let pred = |k: usize| k % 3 != 0;
    let mut ref_probes = Vec::new();
    let mut new_probes = Vec::new();
    let expected = reference_find_integer(pred, &mut ref_probes);
    let actual = drive_find_integer(pred, &mut new_probes);
    assert_eq!(actual, expected);
    assert_eq!(new_probes, ref_probes);
}

#[test]
fn find_integer_ignores_record_after_convergence() {
    let mut search = FindInteger::new();
    while let Some(k) = search.probe() {
        search.record(k <= 2);
    }
    let converged = search.result();
    search.record(true);
    assert_eq!(search.result(), converged);
    assert!(search.probe().is_none());
}

#[test]
#[should_panic(expected = "result read before the search converged")]
fn find_integer_result_panics_before_convergence() {
    FindInteger::new().result();
}

fn reference_bin_search_down(
    lo: i128,
    hi: i128,
    mut f: impl FnMut(i128) -> bool,
    probes: &mut Vec<i128>,
) -> i128 {
    let mut f = move |x: i128| {
        probes.push(x);
        f(x)
    };
    if f(lo) {
        return lo;
    }
    let mut lo = lo;
    let mut hi = hi;
    while lo.checked_add(1).is_some_and(|n| n < hi) {
        let mid = lo + (hi - lo) / 2;
        if f(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

fn drive_bin_search_down(
    lo: i128,
    hi: i128,
    mut f: impl FnMut(i128) -> bool,
    probes: &mut Vec<i128>,
) -> i128 {
    let mut search = BinSearchDown::new(lo, hi);
    while let Some(v) = search.probe() {
        probes.push(v);
        search.record(f(v));
    }
    search.result()
}

#[test]
fn bin_search_down_matches_reference_on_monotone_thresholds() {
    for (lo, hi) in [(0i128, 100), (-50, 50), (0, 1), (7, 7), (0, i128::MAX)] {
        for threshold in [lo, lo + 1, (lo + hi) / 2, hi - 1, hi] {
            let mut ref_probes = Vec::new();
            let mut new_probes = Vec::new();
            let expected = reference_bin_search_down(lo, hi, |v| v >= threshold, &mut ref_probes);
            let actual = drive_bin_search_down(lo, hi, |v| v >= threshold, &mut new_probes);
            assert_eq!(actual, expected, "lo {lo} hi {hi} threshold {threshold}");
            assert_eq!(
                new_probes, ref_probes,
                "lo {lo} hi {hi} threshold {threshold}"
            );
        }
    }
}

#[test]
fn bin_search_down_matches_reference_near_the_upper_bound() {
    let mut ref_probes = Vec::new();
    let mut new_probes = Vec::new();
    let expected = reference_bin_search_down(i128::MAX - 2, i128::MAX, |_| false, &mut ref_probes);
    let actual = drive_bin_search_down(i128::MAX - 2, i128::MAX, |_| false, &mut new_probes);
    assert_eq!(actual, expected);
    assert_eq!(new_probes, ref_probes);
}

#[test]
#[should_panic(expected = "result read before the search converged")]
fn bin_search_down_result_panics_before_convergence() {
    BinSearchDown::new(0, 10).result();
}

#[test]
fn bin_search_down_ignores_record_after_convergence() {
    let mut search = BinSearchDown::new(0, 10);
    while let Some(v) = search.probe() {
        search.record(v >= 4);
    }
    let converged = search.result();
    search.record(false);
    assert_eq!(search.result(), converged);
    assert!(search.probe().is_none());
}

fn reference_bin_search_down_big(
    lo: BigInt,
    hi: BigInt,
    mut f: impl FnMut(&BigInt) -> bool,
    probes: &mut Vec<BigInt>,
) -> BigInt {
    let mut f = move |x: &BigInt| {
        probes.push(x.clone());
        f(x)
    };
    if f(&lo) {
        return lo;
    }
    let mut lo = lo;
    let mut hi = hi;
    while &lo + 1 < hi {
        let mid = &lo + (&hi - &lo) / 2;
        if f(&mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

fn drive_bin_search_down_big(
    lo: BigInt,
    hi: BigInt,
    mut f: impl FnMut(&BigInt) -> bool,
    probes: &mut Vec<BigInt>,
) -> BigInt {
    let mut search = BinSearchDownBig::new(lo, hi);
    while let Some(v) = search.probe() {
        probes.push(v.clone());
        search.record(f(&v));
    }
    search.result()
}

#[test]
fn bin_search_down_big_matches_reference_on_monotone_thresholds() {
    for (lo, hi) in [(0i64, 100), (-50, 50), (0, 1), (9, 9)] {
        for threshold in [lo, lo + 1, (lo + hi) / 2, hi] {
            let threshold = BigInt::from(threshold);
            let mut ref_probes = Vec::new();
            let mut new_probes = Vec::new();
            let expected = reference_bin_search_down_big(
                BigInt::from(lo),
                BigInt::from(hi),
                |v| *v >= threshold,
                &mut ref_probes,
            );
            let actual = drive_bin_search_down_big(
                BigInt::from(lo),
                BigInt::from(hi),
                |v| *v >= threshold,
                &mut new_probes,
            );
            assert_eq!(actual, expected, "lo {lo} hi {hi} threshold {threshold}");
            assert_eq!(
                new_probes, ref_probes,
                "lo {lo} hi {hi} threshold {threshold}"
            );
        }
    }
}

#[test]
fn bin_search_down_big_handles_values_beyond_machine_width() {
    let huge = BigInt::from(u128::MAX) * BigInt::from(16);
    let mut probes = Vec::new();
    let found =
        drive_bin_search_down_big(BigInt::from(0), huge.clone(), |v| *v >= huge, &mut probes);
    assert_eq!(found, huge);
}

#[test]
#[should_panic(expected = "result read before the search converged")]
fn bin_search_down_big_result_panics_before_convergence() {
    BinSearchDownBig::new(BigInt::from(0), BigInt::from(10)).result();
}

#[test]
fn bin_search_down_big_ignores_record_after_convergence() {
    let mut search = BinSearchDownBig::new(BigInt::from(0), BigInt::from(10));
    while let Some(v) = search.probe() {
        search.record(v >= BigInt::from(4));
    }
    let converged = search.probe().is_none();
    search.record(false);
    assert!(converged);
    assert_eq!(search.result(), BigInt::from(4));
}

//! Ported from resources/hypothesis/hypothesis-python/tests/conjecture/test_order_shrinking.py.
//!
//! Exercises the standalone `OrderingShrinker` from
//! `src/native/shrinker/value_shrinkers.rs`.

#![cfg(feature = "native")]

use hegel::__native_test_internals::OrderingShrinker;
use hegel::generators as gs;
use hegel::{Hegel, Settings};

fn ordering_run_step(ls: Vec<i64>) -> Vec<i64> {
    let mut s = OrderingShrinker::new(ls, |_: &[i64]| true);
    s.run_step();
    s.current().to_vec()
}

fn ordering_shrink<F>(initial: Vec<i64>, predicate: F) -> Vec<i64>
where
    F: FnMut(&[i64]) -> bool,
{
    let mut s = OrderingShrinker::new(initial, predicate);
    s.run();
    s.current().to_vec()
}

fn ordering_shrink_full<F>(initial: Vec<i64>, predicate: F) -> Vec<i64>
where
    F: FnMut(&[i64]) -> bool,
{
    let mut s = OrderingShrinker::new(initial, predicate).full(true);
    s.run();
    s.current().to_vec()
}

fn sorted(ls: &[i64]) -> Vec<i64> {
    let mut v = ls.to_vec();
    v.sort();
    v
}

// `test_shrinks_down_to_sorted_the_slow_way`: a single `run_step` on a
// non-short-circuiting `Ordering` (predicate always True) is enough to
// produce `sorted(ls)`. The Python original combines `@example` rows with
// `@given(st.lists(st.integers()))`; we split @example rows into named
// tests and run the PBT case via `Hegel::new`.

#[test]
fn test_shrinks_down_to_sorted_the_slow_way_example_1() {
    let ls = vec![0i64, 1, 1, 1, 1, 1, 1, 0];
    let expected = sorted(&ls);
    assert_eq!(ordering_run_step(ls), expected);
}

#[test]
fn test_shrinks_down_to_sorted_the_slow_way_example_2() {
    let ls = vec![0i64, 0];
    let expected = sorted(&ls);
    assert_eq!(ordering_run_step(ls), expected);
}

#[test]
fn test_shrinks_down_to_sorted_the_slow_way_example_3() {
    let ls = vec![0i64, 1, -1];
    let expected = sorted(&ls);
    assert_eq!(ordering_run_step(ls), expected);
}

#[test]
fn test_shrinks_down_to_sorted_the_slow_way_pbt() {
    Hegel::new(|tc| {
        let ls: Vec<i64> = tc.draw(gs::vecs(gs::integers::<i64>()));
        let expected = sorted(&ls);
        assert_eq!(ordering_run_step(ls), expected);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_can_partially_sort_a_list() {
    let finish = ordering_shrink(vec![5i64, 4, 3, 2, 1, 0], |x: &[i64]| x[0] > x[x.len() - 1]);
    assert_eq!(finish, vec![1i64, 2, 3, 4, 5, 0]);
}

#[test]
fn test_can_partially_sort_a_list_2() {
    let finish = ordering_shrink_full(vec![5i64, 4, 3, 2, 1, 0], |x: &[i64]| x[0] > x[2]);
    // Python: `assert finish <= (1, 2, 0, 3, 4, 5)`. Tuple comparison is
    // lexicographic; `Vec<i64>`'s `PartialOrd` matches.
    assert!(finish <= vec![1i64, 2, 0, 3, 4, 5]);
}

#[test]
fn test_adaptively_shrinks_around_hole() {
    let mut initial: Vec<i64> = (1..=1000).rev().collect();
    initial[500] = 2000;

    let mut intended_result = initial.clone();
    intended_result.sort();
    let last = intended_result.pop().unwrap();
    intended_result.insert(500, last);

    let mut shrinker = OrderingShrinker::new(initial, |ls: &[i64]| ls[500] == 2000).full(true);
    shrinker.run();

    assert_eq!(shrinker.current()[500], 2000);
    assert_eq!(shrinker.current().to_vec(), intended_result);
    assert!(shrinker.calls() <= 60, "calls = {}", shrinker.calls());
}

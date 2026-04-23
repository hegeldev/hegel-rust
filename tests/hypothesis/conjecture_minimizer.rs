//! Ported from hypothesis-python/tests/conjecture/test_minimizer.py
//!
//! Exercises the standalone value shrinkers in `src/native/shrinker/value_shrinkers.rs`:
//! `IntegerShrinker`, `OrderingShrinker`, `CollectionShrinker`, `BytesShrinker`,
//! and `StringShrinker`. The last three are currently stubbed with `todo!()`
//! bodies pending a full port of the Python Collection shrink pipeline — those
//! tests fail at runtime until the stubs are filled in. The `left_is_better`
//! test on `CollectionShrinker` does not call `run()` and passes cleanly.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{
    BigUint, BytesShrinker, CollectionShrinker, IntegerShrinker, IntervalSet, OrderingShrinker,
    StringShrinker,
};

fn integer_shrink<F>(initial: BigUint, predicate: F) -> BigUint
where
    F: FnMut(&BigUint) -> bool,
{
    let mut s = IntegerShrinker::new(initial, predicate);
    s.run();
    s.current().clone()
}

fn ordering_shrink<T, F>(initial: Vec<T>, predicate: F) -> Vec<T>
where
    T: Ord + Clone + std::hash::Hash + Eq,
    F: FnMut(&[T]) -> bool,
{
    let mut s = OrderingShrinker::new(initial, predicate);
    s.run();
    s.current().to_vec()
}

fn ordering_shrink_full<T, F>(initial: Vec<T>, predicate: F) -> Vec<T>
where
    T: Ord + Clone + std::hash::Hash + Eq,
    F: FnMut(&[T]) -> bool,
{
    let mut s = OrderingShrinker::new(initial, predicate).full(true);
    s.run();
    s.current().to_vec()
}

fn bytes_set(v: &[u8]) -> std::collections::HashSet<u8> {
    v.iter().copied().collect()
}

fn bytes_counter(v: &[u8]) -> std::collections::HashMap<u8, usize> {
    let mut m = std::collections::HashMap::new();
    for &b in v {
        *m.entry(b).or_insert(0) += 1;
    }
    m
}

#[test]
fn test_shrink_to_zero() {
    let initial = BigUint::from(1u64 << 16);
    assert_eq!(integer_shrink(initial, |_| true), BigUint::from(0u32));
}

#[test]
fn test_shrink_to_smallest() {
    let initial = BigUint::from(1u64 << 16);
    let ten = BigUint::from(10u32);
    assert_eq!(
        integer_shrink(initial, move |n| *n > ten),
        BigUint::from(11u32)
    );
}

#[test]
fn test_can_sort_bytes_by_reordering() {
    let start: Vec<u8> = vec![5, 4, 3, 2, 1, 0];
    let start_set = bytes_set(&start);
    let finish = ordering_shrink(start.clone(), move |x: &[u8]| bytes_set(x) == start_set);
    assert_eq!(finish, vec![0, 1, 2, 3, 4, 5]);
}

#[test]
fn test_can_sort_bytes_by_reordering_partially() {
    let start: Vec<u8> = vec![5, 4, 3, 2, 1, 0];
    let start_set = bytes_set(&start);
    let finish = ordering_shrink(start.clone(), move |x: &[u8]| {
        bytes_set(x) == start_set && x[0] > x[x.len() - 1]
    });
    assert_eq!(finish, vec![1, 2, 3, 4, 5, 0]);
}

#[test]
fn test_can_sort_bytes_by_reordering_partially2() {
    let start: Vec<u8> = vec![5, 4, 3, 2, 1, 0];
    let start_counter = bytes_counter(&start);
    let finish = ordering_shrink_full(start.clone(), move |x: &[u8]| {
        bytes_counter(x) == start_counter && x[0] > x[2]
    });
    assert_eq!(finish, vec![1, 2, 0, 3, 4, 5]);
}

#[test]
fn test_can_sort_bytes_by_reordering_partially_not_cross_stationary_element() {
    let start: Vec<u8> = vec![5, 3, 0, 2, 1, 4];
    let start_set = bytes_set(&start);
    let finish = ordering_shrink(start.clone(), move |x: &[u8]| {
        bytes_set(x) == start_set && x[3] == 2
    });
    assert_eq!(finish, vec![0, 1, 3, 2, 4, 5]);
}

#[test]
fn test_shrink_strings_always_true() {
    // Python parametrize row: ("f" * 10, lambda s: True, intervals, "")
    let intervals = IntervalSet::new(vec![('a' as u32, 'g' as u32)]);
    let initial: String = "f".repeat(10);
    let shrunk = StringShrinker::shrink(&initial, |_s: &str| true, &intervals, 0);
    let shrunk_str: String = shrunk.into_iter().collect();
    assert_eq!(shrunk_str, "");
}

#[test]
fn test_shrink_strings_min_size_three() {
    // Python row: ("f" * 10, lambda s: len(s) >= 3, intervals, "aaa")
    let intervals = IntervalSet::new(vec![('a' as u32, 'g' as u32)]);
    let initial: String = "f".repeat(10);
    let shrunk = StringShrinker::shrink(&initial, |s: &str| s.chars().count() >= 3, &intervals, 3);
    let shrunk_str: String = shrunk.into_iter().collect();
    assert_eq!(shrunk_str, "aaa");
}

#[test]
fn test_shrink_strings_min_size_three_no_a() {
    // Python row: ("f" * 10, lambda s: len(s) >= 3 and "a" not in s, intervals, "bbb")
    let intervals = IntervalSet::new(vec![('a' as u32, 'g' as u32)]);
    let initial: String = "f".repeat(10);
    let shrunk = StringShrinker::shrink(
        &initial,
        |s: &str| s.chars().count() >= 3 && !s.contains('a'),
        &intervals,
        3,
    );
    let shrunk_str: String = shrunk.into_iter().collect();
    assert_eq!(shrunk_str, "bbb");
}

#[test]
fn test_shrink_bytes_len_two() {
    // Python row: (b"\x18\x12", lambda v: len(v) == 2, b"\x00\x00")
    let shrunk = BytesShrinker::shrink(&[0x18u8, 0x12], |v: &[u8]| v.len() == 2, 2);
    assert_eq!(shrunk, vec![0u8, 0]);
}

#[test]
fn test_shrink_bytes_always_true() {
    // Python row: (b"\x18\x12", lambda v: True, b"")
    let shrunk = BytesShrinker::shrink(&[0x18u8, 0x12], |_v: &[u8]| true, 0);
    assert_eq!(shrunk, Vec::<u8>::new());
}

#[test]
fn test_shrink_bytes_first_byte_one() {
    // Python row: (b"\x01\x10", lambda v: len(v) > 0 and v[0] == 1, b"\x01")
    let shrunk = BytesShrinker::shrink(&[0x01u8, 0x10], |v: &[u8]| !v.is_empty() && v[0] == 1, 1);
    assert_eq!(shrunk, vec![0x01u8]);
}

#[test]
fn test_shrink_bytes_sum_at_least_nine() {
    // Python row: (b"\x01\x10\x01\x92", lambda v: sum(v) >= 9, b"\x09")
    let shrunk = BytesShrinker::shrink(
        &[0x01u8, 0x10, 0x01, 0x92],
        |v: &[u8]| v.iter().map(|&b| b as u64).sum::<u64>() >= 9,
        1,
    );
    assert_eq!(shrunk, vec![0x09u8]);
}

#[test]
fn test_collection_left_is_better() {
    let shrinker: CollectionShrinker<i64, _> =
        CollectionShrinker::new(vec![1i64, 2, 3], |_v: &[i64]| true, 3);
    assert!(!shrinker.left_is_better(&[1i64, 2, 3], &[1, 2, 3]));
}

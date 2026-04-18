//! Ported from resources/pbtkit/tests/findability/test_collections.py

use crate::common::utils::find_any;
use hegel::generators::{self as gs, Generator};

fn list_and_int() -> impl Generator<(Vec<i64>, i64)> {
    gs::vecs(gs::integers::<i64>()).flat_map(|v| gs::integers::<i64>().map(move |i| (v.clone(), i)))
}

#[test]
fn test_containment() {
    for n in [0i64, 1, 10, 100, 1000] {
        let (ls, i) = find_any(list_and_int(), move |(ls, i): &(Vec<i64>, i64)| {
            *i >= n && ls.contains(i)
        });
        assert!(i >= n);
        assert!(ls.contains(&i));
    }
}

#[test]
fn test_duplicate_containment() {
    let (ls, i) = find_any(list_and_int(), |(ls, i): &(Vec<i64>, i64)| {
        ls.iter().filter(|&&x| x == *i).count() > 1
    });
    assert!(ls.iter().filter(|&&x| x == i).count() > 1);
}

#[test]
fn test_can_find_list_with_sum() {
    let result = find_any(gs::vecs(gs::integers::<i64>()), |xs: &Vec<i64>| {
        xs.iter().copied().fold(0i64, i64::saturating_add) >= 10
    });
    assert!(result.iter().copied().fold(0i64, i64::saturating_add) >= 10);
}

#[test]
fn test_can_find_dictionary_with_key_gt_value() {
    use std::collections::HashMap;
    let result = find_any(
        gs::hashmaps(gs::integers::<i64>(), gs::integers::<i64>()),
        |xs: &HashMap<i64, i64>| xs.iter().any(|(k, v)| k > v),
    );
    assert!(result.iter().any(|(k, v)| k > v));
}

#[test]
fn test_can_find_sorted_list() {
    find_any(gs::vecs(gs::integers::<i64>()), |xs: &Vec<i64>| {
        let mut sorted = xs.clone();
        sorted.sort();
        sorted != *xs
    });
}

#[test]
fn test_can_find_large_sum_list() {
    let result = find_any(
        gs::vecs(gs::integers::<i64>().min_value(0).max_value(100)),
        |xs: &Vec<i64>| xs.iter().sum::<i64>() >= 100,
    );
    assert!(result.iter().sum::<i64>() >= 100);
}

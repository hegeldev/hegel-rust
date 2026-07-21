//! Unit tests for the `Ordering` shrinker primitive used by
//! `reorder_spans`.

use super::shrink_ordering;
use crate::exchange::drive_no_yield;

#[test]
fn shrink_ordering_short_circuits_to_full_sort() {
    let mut accepted_perms: Vec<Vec<usize>> = Vec::new();
    let keys = vec![3u32, 1, 4, 1, 5];
    drive_no_yield(shrink_ordering(
        keys.len(),
        |i| keys[i],
        |perm: &[usize]| {
            accepted_perms.push(perm.to_vec());
            Ok(true)
        },
    ))
    .unwrap();
    assert_eq!(accepted_perms.len(), 1);
    let sorted_keys: Vec<u32> = accepted_perms[0].iter().map(|&i| keys[i]).collect();
    let mut expected = keys.clone();
    expected.sort();
    assert_eq!(sorted_keys, expected);
}

#[test]
fn shrink_ordering_falls_back_to_region_sort_when_full_sort_rejected() {
    let keys = [2u32, 1, 3, 1];
    let mut accepted: Vec<Vec<usize>> = Vec::new();
    drive_no_yield(shrink_ordering(
        keys.len(),
        |i| keys[i],
        |perm: &[usize]| {
            let mapped: Vec<u32> = perm.iter().map(|&i| keys[i]).collect();
            if mapped == vec![1u32, 1, 2, 3] {
                return Ok(false);
            }
            accepted.push(perm.to_vec());
            Ok(true)
        },
    ))
    .unwrap();
    assert!(!accepted.is_empty());
}

#[test]
fn shrink_ordering_returns_early_on_trivial_input() {
    let mut count = 0;
    drive_no_yield(shrink_ordering(
        1,
        |_| 0u32,
        |_: &[usize]| {
            count += 1;
            Ok(true)
        },
    ))
    .unwrap();
    assert_eq!(count, 0);
    let mut count2 = 0;
    drive_no_yield(shrink_ordering(
        0,
        |_| 0u32,
        |_: &[usize]| {
            count2 += 1;
            Ok(true)
        },
    ))
    .unwrap();
    assert_eq!(count2, 0);
}

#[test]
fn shrink_ordering_handles_predicate_that_never_accepts() {
    let keys = [5u32, 3, 1];
    drive_no_yield(shrink_ordering(
        keys.len(),
        |i| keys[i],
        |_: &[usize]| Ok(false),
    ))
    .unwrap();
}

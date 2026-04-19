//! Ported from hypothesis-python/tests/cover/test_permutations.py

use crate::common::utils::{assert_all_examples, minimal};
use hegel::generators::{self as gs};
use std::collections::HashSet;

#[test]
fn test_can_find_non_trivial_permutation() {
    let xs: Vec<i64> = (0..5).collect();
    let perm = minimal(gs::permutations(xs), |x: &Vec<i64>| x[0] != 0);
    assert_eq!(perm, vec![1, 0, 2, 3, 4]);
}

#[test]
fn test_permutation_values_are_permutations() {
    let chars: Vec<char> = "abcd".chars().collect();
    let expected: HashSet<char> = chars.iter().copied().collect();
    assert_all_examples(gs::permutations(chars), move |perm: &Vec<char>| {
        perm.len() == 4 && perm.iter().copied().collect::<HashSet<_>>() == expected
    });
}

#[test]
fn test_empty_permutations_are_empty() {
    let empty: Vec<i64> = vec![];
    assert_all_examples(gs::permutations(empty), |xs: &Vec<i64>| xs.is_empty());
}

// Omitted: test_cannot_permute_non_sequence_types — Rust's type system
// already prevents passing a non-sequence to `permutations`.

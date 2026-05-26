//! Unit tests for the `Ordering` shrinker primitive used by
//! `reorder_spans`.

use super::shrink_ordering;

#[test]
fn shrink_ordering_short_circuits_to_full_sort() {
    // The full-sort short circuit fires on the very first attempt and
    // bails immediately if accepted.
    let mut accepted_perms: Vec<Vec<usize>> = Vec::new();
    let keys = vec![3u32, 1, 4, 1, 5];
    shrink_ordering(
        keys.len(),
        |i| keys[i],
        |perm| {
            accepted_perms.push(perm.to_vec());
            true
        },
    );
    // Exactly one accept: a fully sorted permutation (1, 1, 3, 4, 5).
    assert_eq!(accepted_perms.len(), 1);
    let sorted_keys: Vec<u32> = accepted_perms[0].iter().map(|&i| keys[i]).collect();
    let mut expected = keys.clone();
    expected.sort();
    assert_eq!(sorted_keys, expected);
}

#[test]
fn shrink_ordering_falls_back_to_region_sort_when_full_sort_rejected() {
    // Predicate rejects the full sort but accepts a partial sort.  We
    // simulate by rejecting any permutation that's globally sorted but
    // accepting partial ones.
    let keys = [2u32, 1, 3, 1];
    let mut accepted: Vec<Vec<usize>> = Vec::new();
    shrink_ordering(
        keys.len(),
        |i| keys[i],
        |perm| {
            // Reject the globally-sorted permutation (1, 1, 2, 3 ⇒ keys
            // [1,3] [1,0] [2,2] etc).  Accept anything else that's an
            // improvement.
            let mapped: Vec<u32> = perm.iter().map(|&i| keys[i]).collect();
            if mapped == vec![1u32, 1, 2, 3] {
                return false;
            }
            accepted.push(perm.to_vec());
            true
        },
    );
    // Some accept happened in the fallback phases.
    assert!(!accepted.is_empty());
}

#[test]
fn shrink_ordering_returns_early_on_trivial_input() {
    // n <= 1 → nothing to do, no accepts ever issued.
    let mut count = 0;
    shrink_ordering(
        1,
        |_| 0u32,
        |_| {
            count += 1;
            true
        },
    );
    assert_eq!(count, 0);
    let mut count2 = 0;
    shrink_ordering(
        0,
        |_| 0u32,
        |_| {
            count2 += 1;
            true
        },
    );
    assert_eq!(count2, 0);
}

#[test]
fn shrink_ordering_handles_predicate_that_never_accepts() {
    // No improvement: the algorithm tries a few permutations and gives
    // up without panicking.
    let keys = [5u32, 3, 1];
    shrink_ordering(keys.len(), |i| keys[i], |_| false);
    // No assertion — just verify it terminates.
}

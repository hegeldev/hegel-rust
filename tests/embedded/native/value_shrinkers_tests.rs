use super::*;
use crate::native::bignum::BigUint;
use crate::native::intervalsets::IntervalSet;

// ── CollectionShrinker::new and calls() ───────────────────────────────────

#[test]
fn collection_shrinker_initial_calls_is_one() {
    // After construction, seen contains the initial value, so calls() == 1.
    let shrinker = CollectionShrinker::new(vec![3usize, 1, 2], |_: &[usize]| true, 0);
    assert_eq!(shrinker.calls(), 1);
    assert_eq!(shrinker.current(), &[3, 1, 2]);
}

// ── CollectionShrinker::run — short-circuit when zeros accepted ───────────

#[test]
fn collection_shrinker_run_short_circuits_to_zeros() {
    // Predicate accepts anything. The short-circuit try of [0;min_size=0] = []
    // succeeds immediately and run() returns without calling run_step.
    let mut shrinker = CollectionShrinker::new(vec![5usize, 3, 7], |_: &[usize]| true, 0);
    shrinker.run();
    // After short-circuit, current should be [] (min_size=0, all-zero = empty).
    assert_eq!(shrinker.current(), &[] as &[usize]);
}

// ── CollectionShrinker::consider — returns true when value == current ─────

#[test]
fn collection_shrinker_run_all_zero_at_same_length() {
    // Predicate: only accept [0,0,0] or [5,3,7] — forcing run_step to do
    // the all-zero pass and accept it. min_size=3 blocks shrinking to shorter.
    let mut shrinker = CollectionShrinker::new(
        vec![5usize, 3, 7],
        |v: &[usize]| v == [0, 0, 0] || v == [5, 3, 7],
        3,
    );
    shrinker.run();
    assert_eq!(shrinker.current(), &[0, 0, 0]);
}

// ── CollectionShrinker::consider — returns false when not better ──────────

#[test]
fn collection_shrinker_does_not_accept_worse_candidate() {
    // Predicate: only accept [1,2,3]. Shrinker can never find something
    // shorter because min_size=3 and [0,0,0] is not accepted.
    let mut shrinker = CollectionShrinker::new(vec![1usize, 2, 3], |v: &[usize]| v == [1, 2, 3], 3);
    shrinker.run();
    // No change possible.
    assert_eq!(shrinker.current(), &[1, 2, 3]);
}

// ── CollectionShrinker::run_step — element-by-element minimization ────────

#[test]
fn collection_shrinker_minimizes_elements() {
    // Each element can be independently lowered to its minimum.
    // Predicate: length must be exactly 3; each element >= 2.
    let mut shrinker = CollectionShrinker::new(
        vec![10usize, 20, 30],
        |v: &[usize]| v.len() == 3 && v.iter().all(|&x| x >= 2),
        3,
    );
    shrinker.run();
    // After minimization each element should be at its minimum (2).
    let cur = shrinker.current();
    assert_eq!(cur.len(), 3);
    assert!(cur.iter().all(|&x| x == 2), "expected all 2s, got {cur:?}");
}

// ── CollectionShrinker::run_step — duplicate minimization ─────────────────

#[test]
fn collection_shrinker_minimizes_duplicates() {
    // [5, 5, 5] with predicate: all elements equal and >= 3.
    // Duplicate minimization should find the minimum shared value (3).
    let mut shrinker = CollectionShrinker::new(
        vec![5usize, 5, 5],
        |v: &[usize]| v.len() == 3 && v.iter().all(|&x| x == v[0]) && v[0] >= 3,
        3,
    );
    shrinker.run();
    assert_eq!(shrinker.current(), &[3, 3, 3]);
}

// ── CollectionShrinker::consider — returns true when value == current ─────

#[test]
fn collection_shrinker_run_zeros_equal_to_current_fires_line_374() {
    // Initial value is already the all-zero vector with min_size=2.
    // run() tries zeros=[0,0] == current=[0,0] → consider returns true (line 374).
    // run() returns early on the short-circuit.
    let mut shrinker = CollectionShrinker::new(vec![0usize, 0], |_: &[usize]| true, 2);
    shrinker.run();
    assert_eq!(shrinker.current(), &[0, 0]);
}

// ── CollectionShrinker::consider — returns false when !left_is_better ─────

#[test]
fn collection_shrinker_consider_returns_false_when_not_better() {
    // Start with [5, 8], min_size=2.
    // Predicate: accept any [a,b] where a+b >= 10 (reject all-zeros).
    // run_step step 5: per-element IntegerShrinker reduces element[0] from 5 to 2
    // (since 2+8=10 ≥ 10). Current becomes [2,8]. IntegerShrinker then probes
    // value 3 for element[0]: candidate=[3,8]. left_is_better([3,8],[2,8]) =
    // false (3 > 2) → consider returns false at line 380.
    let mut shrinker = CollectionShrinker::new(
        vec![5usize, 8],
        |v: &[usize]| v.len() == 2 && v[0] + v[1] >= 10,
        2,
    );
    shrinker.run();
    // After shrinking, element[0] should be at its minimum given a+b >= 10.
    let cur = shrinker.current();
    assert_eq!(cur.len(), 2);
    assert!(cur[0] + cur[1] >= 10);
}

// ── CollectionShrinker::consider — returns false for already-seen value ───

#[test]
fn collection_shrinker_consider_returns_false_for_duplicate_seen() {
    // Start with [3, 1]. Predicate: only [3, 1] is accepted (no shrinking possible).
    // run_step will try various candidates. The ordering shrinker tries [1,3].
    // After [1,3] is rejected (not accepted by predicate), it's in seen.
    // If OrderingShrinker tries [1,3] again via a different path, consider sees
    // it as "already seen" → line 377.
    // More directly: the all-zero pass at run_step step 1 is tried, then step 3
    // ordering shrinker proposes [1,3] which is left_is_better than [3,1] but
    // the predicate rejects it. Step 5 per-element: tries candidate with changed
    // element which is in seen → line 377.
    let mut shrinker = CollectionShrinker::new(
        vec![3usize, 1],
        |v: &[usize]| v == [3, 1],
        2,
    );
    shrinker.run();
    assert_eq!(shrinker.current(), &[3, 1]);
}

// ── BytesShrinker smoke test ───────────────────────────────────────────────

#[test]
fn bytes_shrinker_shrinks_to_minimum() {
    // Always accept; BytesShrinker should shrink to all-zero bytes.
    let result = BytesShrinker::shrink(&[5u8, 10, 15], |_: &[u8]| true, 0);
    assert!(result.is_empty());
}

// ── StringShrinker tests ───────────────────────────────────────────────────

#[test]
fn string_shrinker_shrinks_to_minimum() {
    // Use interval [65,90] = 'A'..'Z'. Start with "ZZZ", predicate accepts
    // anything with at least 1 char. Should shrink to "A" (the simplest char).
    let intervals = IntervalSet::new(vec![(65u32, 90u32)]);
    let result = StringShrinker::shrink("ZZZ", |s: &str| !s.is_empty(), &intervals, 1);
    assert!(!result.is_empty());
    // The simplest char in shrink order is 'A' (index 0 in [A..Z]).
    assert_eq!(result, vec!['A']);
}

#[test]
fn string_shrinker_preserves_min_size() {
    // min_size=3: result must have at least 3 chars.
    let intervals = IntervalSet::new(vec![(65u32, 90u32)]);
    let result = StringShrinker::shrink("ZZZ", |_: &str| true, &intervals, 3);
    assert_eq!(result.len(), 3);
}

// ── IntegerShrinker tests ──────────────────────────────────────────────────

#[test]
fn integer_shrinker_reduces_large_value() {
    // Start with 100; predicate accepts anything >= 10.
    let initial = BigUint::from(100u32);
    let mut shrinker = IntegerShrinker::new(initial, |v: &BigUint| *v >= BigUint::from(10u32));
    shrinker.run();
    assert_eq!(*shrinker.current(), BigUint::from(10u32));
}

#[test]
fn integer_shrinker_short_circuits_to_zero() {
    // Predicate accepts everything; short-circuit should set current to 0.
    let initial = BigUint::from(42u32);
    let mut shrinker = IntegerShrinker::new(initial, |_: &BigUint| true);
    shrinker.run();
    assert_eq!(*shrinker.current(), BigUint::from(0u32));
}

#[test]
fn integer_shrinker_short_circuits_to_one() {
    // Predicate: reject 0, accept everything >= 1.
    let initial = BigUint::from(50u32);
    let mut shrinker = IntegerShrinker::new(initial, |v: &BigUint| *v >= BigUint::from(1u32));
    shrinker.run();
    assert_eq!(*shrinker.current(), BigUint::from(1u32));
}

#[test]
fn integer_shrinker_mask_high_bits_and_shift() {
    // Start with a large value that exercises mask_high_bits and shift_right.
    // BigUint(256) has bits = 9. short_circuit tries 0 (reject), 1 (reject).
    // mask_high_bits and shift are exercised. run_step further shrinks.
    let initial = BigUint::from(256u32);
    let mut shrinker = IntegerShrinker::new(initial, |v: &BigUint| *v >= BigUint::from(200u32));
    shrinker.run();
    // Minimum value satisfying v >= 200 starting from 256.
    assert!(*shrinker.current() >= BigUint::from(200u32));
}

// ── OrderingShrinker tests ─────────────────────────────────────────────────

#[test]
fn ordering_shrinker_sorts_sequence() {
    // Start with [3,1,2]; predicate accepts anything. Short-circuit sorts to [1,2,3].
    let mut shrinker = OrderingShrinker::new(vec![3usize, 1, 2], |_: &[usize]| true);
    shrinker.run();
    assert_eq!(shrinker.current(), &[1, 2, 3]);
}

#[test]
fn ordering_shrinker_full_mode_iterates() {
    // full=true: run() iterates run_step until no more changes.
    let mut shrinker =
        OrderingShrinker::new(vec![5usize, 3, 4, 1, 2], |_: &[usize]| true).full(true);
    shrinker.run();
    assert_eq!(shrinker.current(), &[1, 2, 3, 4, 5]);
}

#[test]
fn ordering_shrinker_does_not_change_already_sorted() {
    // Already sorted [1,2,3]; run() short-circuits (sorted == current).
    let mut shrinker = OrderingShrinker::new(vec![1usize, 2, 3], |_: &[usize]| true);
    shrinker.run();
    assert_eq!(shrinker.current(), &[1, 2, 3]);
}

#[test]
fn ordering_shrinker_left_is_better_sorts_within_predicate() {
    // Predicate: only accept sequences where first element <= second.
    // Start with [4, 2]: sorted [2,4] passes. run() short-circuits.
    let mut shrinker =
        OrderingShrinker::new(vec![4usize, 2], |v: &[usize]| v[0] <= v[1]);
    shrinker.run();
    assert_eq!(shrinker.current(), &[2, 4]);
}

#[test]
fn ordering_shrinker_sort_regions_with_gaps() {
    // A sequence with a local disorder that sort_regions_with_gaps should fix.
    // [1, 5, 2, 3, 4]: position 1 (5) is out of order with position 2 (2).
    let mut shrinker =
        OrderingShrinker::new(vec![1usize, 5, 2, 3, 4], |_: &[usize]| true);
    shrinker.run();
    assert_eq!(shrinker.current(), &[1, 2, 3, 4, 5]);
}

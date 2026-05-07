use super::*;

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

// ── BytesShrinker smoke test ───────────────────────────────────────────────

#[test]
fn bytes_shrinker_shrinks_to_minimum() {
    // Always accept; BytesShrinker should shrink to all-zero bytes.
    let result = BytesShrinker::shrink(&[5u8, 10, 15], |_: &[u8]| true, 0);
    assert!(result.is_empty());
}

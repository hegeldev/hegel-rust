//! Ported from hypothesis-python/tests/cover/test_shrink_budgeting.py
//!
//! The Python test parametrises over `(Shrinker, value)` pairs. Each runs the
//! shrinker with a predicate that only accepts the initial value (so nothing
//! can improve) and asserts `shrinker.calls <= 10` — i.e. the shrinker gives
//! up quickly when no improvement is possible.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{BigUint, IntegerShrinker, OrderingShrinker};

fn check_integer_budget(initial: BigUint) {
    let target = initial.clone();
    let mut shrinker = IntegerShrinker::new(initial, move |x| *x == target);
    shrinker.run();
    assert!(
        shrinker.calls() <= 10,
        "Integer shrinker used {} calls",
        shrinker.calls()
    );
}

fn check_ordering_budget<T>(initial: Vec<T>)
where
    T: Ord + Clone + std::hash::Hash + Eq + 'static,
{
    let target = initial.clone();
    let mut shrinker = OrderingShrinker::new(initial, move |x| x == target.as_slice());
    shrinker.run();
    assert!(
        shrinker.calls() <= 10,
        "Ordering shrinker used {} calls",
        shrinker.calls()
    );
}

#[test]
fn test_integer_budget_power_of_two() {
    // 2**16
    check_integer_budget(BigUint::from(1u32 << 16));
}

#[test]
fn test_integer_budget_float_max_as_int() {
    // int(sys.float_info.max) — far larger than u128.
    let s = b"179769313486231570814527423731704356798070567525844996598917476803157260780028538760589558632766878171540458953514382464234321326889464182768467546703537516986049910576551282076245490090389328944075868508455133942304583236903222948165808559332123348274797826204144723168738177180919299881250404026184124858368";
    let initial = BigUint::parse_bytes(s, 10).unwrap();
    check_integer_budget(initial);
}

#[test]
fn test_ordering_budget_single_long_tuple() {
    // [(100,) * 10] — a list of one 10-element "tuple".
    check_ordering_budget::<Vec<i64>>(vec![vec![100; 10]]);
}

#[test]
fn test_ordering_budget_already_sorted() {
    // [i * 100 for i in range(5)]
    check_ordering_budget::<i64>(vec![0, 100, 200, 300, 400]);
}

#[test]
fn test_ordering_budget_reverse_sorted() {
    // [i * 100 for i in reversed(range(5))]
    check_ordering_budget::<i64>(vec![400, 300, 200, 100, 0]);
}

// Witnesses below exercise shrink paths that the Python budget tests do not
// reach (they assert nothing improves). These are non-upstream; they keep
// `find_integer`'s exponential/binary-search arms, `mask_high_bits`'s k>=n
// guard, `consider`'s value==current fast-path, and `OrderingShrinker`'s
// sort_regions_with_gaps skip branch under coverage.

#[test]
fn test_integer_shrinker_permissive_shrinks_to_zero() {
    // Predicate accepts everything, so consider(0) in short_circuit succeeds
    // immediately — exercising the short_circuit early-return and run()'s
    // early-return when short_circuit returns true.
    let initial = BigUint::from(42u32);
    let mut shrinker = IntegerShrinker::new(initial, |_| true);
    shrinker.run();
    assert_eq!(*shrinker.current(), BigUint::from(0u32));
}

#[test]
fn test_integer_shrinker_improves_with_threshold_predicate() {
    // Predicate rejects 0,1 (forcing past short_circuit) and rejects <5
    // (forcing several rounds of mask/shift/shrink). The binary search lands
    // on a mask that equals the current value, hitting consider's fast-path.
    let initial = BigUint::from(1000u32);
    let threshold = BigUint::from(5u32);
    let t = threshold.clone();
    let mut shrinker = IntegerShrinker::new(initial, move |x| *x >= t);
    shrinker.run();
    assert!(*shrinker.current() >= threshold);
}

#[test]
fn test_ordering_shrinker_partial_order_continue_branch() {
    // [3, 1, 2, 4, 5] — once sort_regions fails to improve further, the
    // sort_regions_with_gaps pass skips indices where neighbours are already
    // in order (i=2, i=3).
    let initial: Vec<i64> = vec![3, 1, 2, 4, 5];
    let t = initial.clone();
    let mut shrinker = OrderingShrinker::new(initial.clone(), move |x| x == t.as_slice());
    shrinker.run();
    assert_eq!(shrinker.current(), initial.as_slice());
}

#[test]
fn test_ordering_shrinker_gap_sort_improves() {
    // [2, 3, 1, 4] with predicate "anything except the fully-sorted list":
    //   - short_circuit sorts to [1,2,3,4] → rejected.
    //   - sort_regions lands on [2,3,1,4] (fully-sorted k=3 chunk rejected).
    //   - sort_regions_with_gaps at i=1 enters body: grow_left at k=1 produces
    //     [1,3,2,4] which is accepted and < current → improvement path. k=2
    //     then triggers `k > left` guard.
    let initial: Vec<i64> = vec![2, 3, 1, 4];
    let mut shrinker = OrderingShrinker::new(initial, |x| x != [1, 2, 3, 4].as_slice());
    shrinker.run();
    assert!(shrinker.current() < [2, 3, 1, 4].as_slice());
    assert_ne!(shrinker.current(), [1, 2, 3, 4].as_slice());
}

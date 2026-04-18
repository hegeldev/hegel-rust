//! Ported from resources/pbtkit/tests/shrink_quality/test_integers.py

use crate::common::utils::{Minimal, minimal};
use hegel::generators as gs;
#[cfg(feature = "native")]
use hegel::{Hegel, Settings};

#[test]
fn test_integers_from_minimizes_leftwards() {
    assert_eq!(
        minimal(gs::integers::<i64>().min_value(101), |_: &i64| true),
        101
    );
}

#[test]
fn test_minimize_bounded_integers_to_zero() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(-10).max_value(10),
            |_: &i64| true,
        ),
        0
    );
}

#[test]
fn test_minimize_bounded_integers_to_positive() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(-10).max_value(10),
            |x: &i64| *x != 0,
        ),
        1
    );
}

#[test]
fn test_minimize_single_element_in_silly_large_int_range() {
    let hi = i64::MAX;
    let lo = i64::MIN;
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(lo / 2).max_value(hi / 2),
            move |x: &i64| *x >= lo / 4,
        ),
        0
    );
}

#[test]
fn test_minimize_multiple_elements_in_silly_large_int_range() {
    let hi = i64::MAX;
    let lo = i64::MIN;
    let result = Minimal::new(
        gs::vecs(gs::integers::<i64>().min_value(lo / 2).max_value(hi / 2)),
        |x: &Vec<i64>| x.len() >= 20,
    )
    .test_cases(10000)
    .run();
    assert_eq!(result, vec![0i64; 20]);
}

#[test]
fn test_minimize_multiple_elements_min_is_not_dupe() {
    let result = Minimal::new(
        gs::vecs(gs::integers::<i64>().min_value(0).max_value(i64::MAX / 2)),
        |x: &Vec<i64>| x.len() >= 20 && (0..20).all(|i| x[i] >= i as i64),
    )
    .test_cases(10000)
    .run();
    let expected: Vec<i64> = (0..20).collect();
    assert_eq!(result, expected);
}

#[test]
fn test_can_find_an_int() {
    assert_eq!(minimal(gs::integers::<i64>(), |_: &i64| true), 0);
}

#[test]
fn test_can_find_an_int_above_13() {
    assert_eq!(minimal(gs::integers::<i64>(), |x: &i64| *x >= 13), 13);
}

#[test]
fn test_minimizes_towards_zero() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(-1000).max_value(50),
            |x: &i64| *x < 0,
        ),
        -1
    );
}

// Tests 10-12 mirror test_integer_shrinks_* from the upstream, which use
// PbtkitState directly. The same shrink quality is verified via minimal().

#[test]
fn test_integer_shrinks_negative() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(-1000).max_value(1000),
            |x: &i64| *x < 0,
        ),
        -1
    );
}

#[test]
fn test_integer_shrinks_via_binary_search() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(0).max_value(10000),
            |x: &i64| *x > 100,
        ),
        101
    );
}

#[test]
fn test_integer_shrinks_negative_only_range() {
    assert_eq!(
        minimal(
            gs::integers::<i64>().min_value(-100).max_value(-1),
            |x: &i64| *x <= -10,
        ),
        -10
    );
}

// `choice(n)` in pbtkit draws from [0, n] inclusive, so `choice(1000)` →
// `gs::integers::<i64>().min_value(0).max_value(1000)`.
#[test]
fn test_reduces_additive_pairs() {
    let (m, n) = Minimal::new(
        gs::tuples!(
            gs::integers::<i64>().min_value(0).max_value(1000),
            gs::integers::<i64>().min_value(0).max_value(1000),
        ),
        |(m, n): &(i64, i64)| *m + *n > 1000,
    )
    .test_cases(10000)
    .run();
    assert_eq!((m, n), (1, 1000));
}

// Tests 14-15 exercise `redistribute_integers` with conditional generation
// (the number of draws depends on a prior choice). Native-gated because
// Hypothesis's server-side choice-sequence encoding differs, so the specific
// shrunk tuple can only be pinned down against the native shrinker.

#[cfg(feature = "native")]
#[test]
fn test_redistribute_stale_indices() {
    use std::sync::{Arc, Mutex};

    let found: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    let found_clone = found.clone();

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Hegel::new(move |tc| {
            let b: bool = tc.draw(gs::booleans());
            let a: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
            let c: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
            if b {
                let d: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
                if a + c + d > 200 {
                    *found_clone.lock().unwrap() = true;
                    panic!("found");
                }
            } else if a + c > 100 {
                *found_clone.lock().unwrap() = true;
                panic!("found");
            }
        })
        .settings(Settings::new().test_cases(2000).database(None))
        .run();
    }));

    assert!(*found.lock().unwrap(), "should have found a counterexample");
}

#[cfg(feature = "native")]
#[test]
fn test_redistribute_stale_indices_at_gap_two() {
    use std::sync::{Arc, Mutex};

    let shrunk: Arc<Mutex<Option<(i64, i64)>>> = Arc::new(Mutex::new(None));
    let shrunk_clone = shrunk.clone();

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Hegel::new(move |tc| {
            let gate: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(138));
            let base: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
            if gate > 46 {
                let extra: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
                if base + extra > 30 {
                    *shrunk_clone.lock().unwrap() = Some((gate, base));
                    panic!("found");
                }
            } else if base > 27 {
                *shrunk_clone.lock().unwrap() = Some((gate, base));
                panic!("found");
            }
        })
        .settings(
            Settings::new()
                .test_cases(3000)
                .database(None)
                .derandomize(true),
        )
        .run();
    }));

    let (gate, base) = shrunk
        .lock()
        .unwrap()
        .take()
        .expect("should have found counterexample");
    // Should shrink to: gate=0 (short path), base=28 (> 27)
    assert_eq!((gate, base), (0, 28));
}

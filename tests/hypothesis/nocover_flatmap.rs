//! Ported from hypothesis-python/tests/nocover/test_flatmap.py
//!
//! Individually-skipped tests:
//! - `test_flatmap_does_not_reuse_strategies`: Python `is not`
//!   object-identity check (`find_any(s) is not find_any(s)`); hegel-rust
//!   draws return owned/cloned values, so identity-distinctness is
//!   structural rather than observable.
//! - `test_flatmap_has_original_strategy_repr`: Python `repr()` output of
//!   a composed strategy; hegel-rust generators have no repr surface.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::common::utils::{Minimal, minimal};
use hegel::generators::{self as gs, Generator};
use hegel::{HealthCheck, Hegel, Settings, TestCase};

#[test]
fn test_constant_lists_are_constant() {
    Hegel::new(|tc: TestCase| {
        let x: Vec<i64> = tc.draw(gs::integers::<i64>().flat_map(|i| gs::vecs(gs::just(i))));
        tc.assume(x.len() >= 3);
        let first = x[0];
        assert!(x.iter().all(|&v| v == first));
    })
    .settings(
        Settings::new()
            .test_cases(100)
            .database(None)
            .suppress_health_check([HealthCheck::FilterTooMuch]),
    )
    .run();
}

#[test]
fn test_in_order() {
    Hegel::new(|tc: TestCase| {
        let (a, b): (i64, i64) = tc.draw(
            gs::integers::<i64>()
                .min_value(1)
                .max_value(200)
                .flat_map(|e| {
                    gs::tuples!(
                        gs::integers::<i64>().min_value(0).max_value(e - 1),
                        gs::just(e),
                    )
                }),
        );
        assert!(a < b);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_flatmap_retrieve_from_db() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().to_str().unwrap().to_string();

    let track: Arc<Mutex<Vec<Vec<f64>>>> = Arc::new(Mutex::new(Vec::new()));

    let run_test = || {
        let track = Arc::clone(&track);
        let db_path = db_path.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            Hegel::new(move |tc: TestCase| {
                let xs: Vec<f64> = tc.draw(
                    gs::floats::<f64>()
                        .min_value(0.0)
                        .max_value(1.0)
                        .flat_map(|x| gs::vecs(gs::just(x))),
                );
                if xs.iter().sum::<f64>() >= 1.0 {
                    track.lock().unwrap().push(xs);
                    panic!("expected failure");
                }
            })
            .settings(Settings::new().database(Some(db_path)).derandomize(false))
            .__database_key("test_flatmap_retrieve_from_db".to_string())
            .run();
        }));
        assert!(result.is_err(), "expected the test to fail");
    };

    run_test();
    let example = {
        let guard = track.lock().unwrap();
        assert!(!guard.is_empty());
        guard.last().unwrap().clone()
    };

    track.lock().unwrap().clear();

    run_test();

    let guard = track.lock().unwrap();
    assert_eq!(guard[0], example);
}

#[test]
fn test_mixed_list_flatmap() {
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum Value {
        Bool(bool),
        Text(String),
    }

    let result = Minimal::new(
        gs::vecs(gs::booleans().flat_map(|b| {
            if b {
                gs::booleans().map(Value::Bool).boxed()
            } else {
                gs::text().map(Value::Text).boxed()
            }
        })),
        |ls: &Vec<Value>| {
            let bools = ls.iter().filter(|x| matches!(x, Value::Bool(_))).count();
            let texts = ls.iter().filter(|x| matches!(x, Value::Text(_))).count();
            bools >= 3 && texts >= 3
        },
    )
    .test_cases(10000)
    .run();
    assert_eq!(result.len(), 6);
    let as_set: HashSet<_> = result.into_iter().collect();
    assert_eq!(
        as_set,
        HashSet::from([Value::Bool(false), Value::Text(String::new())])
    );
}

fn shrink_through_a_binding_case(n: usize) {
    let result = minimal(
        gs::integers::<i64>()
            .min_value(0)
            .max_value(100)
            .flat_map(|k| {
                gs::vecs(gs::booleans())
                    .min_size(k as usize)
                    .max_size(k as usize)
            }),
        move |x: &Vec<bool>| x.iter().filter(|&&b| b).count() >= n,
    );
    assert_eq!(result, vec![true; n]);
}

#[test]
fn test_can_shrink_through_a_binding_1() {
    shrink_through_a_binding_case(1);
}

#[test]
fn test_can_shrink_through_a_binding_2() {
    shrink_through_a_binding_case(2);
}

#[test]
fn test_can_shrink_through_a_binding_3() {
    shrink_through_a_binding_case(3);
}

#[test]
fn test_can_shrink_through_a_binding_4() {
    shrink_through_a_binding_case(4);
}

#[test]
fn test_can_shrink_through_a_binding_5() {
    shrink_through_a_binding_case(5);
}

#[test]
fn test_can_shrink_through_a_binding_6() {
    shrink_through_a_binding_case(6);
}

#[test]
fn test_can_shrink_through_a_binding_7() {
    shrink_through_a_binding_case(7);
}

#[test]
fn test_can_shrink_through_a_binding_8() {
    shrink_through_a_binding_case(8);
}

#[test]
fn test_can_shrink_through_a_binding_9() {
    shrink_through_a_binding_case(9);
}

fn delete_in_middle_of_a_binding_case(n: usize) {
    let result = minimal(
        gs::integers::<i64>()
            .min_value(1)
            .max_value(100)
            .flat_map(|k| {
                gs::vecs(gs::booleans())
                    .min_size(k as usize)
                    .max_size(k as usize)
            }),
        move |x: &Vec<bool>| {
            x.len() >= 2 && x[0] && *x.last().unwrap() && x.iter().filter(|&&b| !b).count() >= n
        },
    );
    let mut expected = vec![false; n + 2];
    expected[0] = true;
    expected[n + 1] = true;
    assert_eq!(result, expected);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_1() {
    delete_in_middle_of_a_binding_case(1);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_2() {
    delete_in_middle_of_a_binding_case(2);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_3() {
    delete_in_middle_of_a_binding_case(3);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_4() {
    delete_in_middle_of_a_binding_case(4);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_5() {
    delete_in_middle_of_a_binding_case(5);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_6() {
    delete_in_middle_of_a_binding_case(6);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_7() {
    delete_in_middle_of_a_binding_case(7);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_8() {
    delete_in_middle_of_a_binding_case(8);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_9() {
    delete_in_middle_of_a_binding_case(9);
}

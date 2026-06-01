use super::*;
use crate::backend::TestCaseResult;
use crate::cbor_utils::cbor_map;
use crate::runner::{Database, Settings};
use std::sync::atomic::{AtomicUsize, Ordering};

fn quiet_settings(test_cases: u64) -> Settings {
    let mut s = Settings::new().test_cases(test_cases);
    s.database = Database::Disabled;
    s
}

#[test]
fn run_native_invokes_callback_and_returns_passing_result() {
    let calls = AtomicUsize::new(0);
    let result = run_native(&quiet_settings(5), None, |ds, _is_final| {
        calls.fetch_add(1, Ordering::SeqCst);
        ds.mark_complete(&TestCaseResult::Valid);
    });
    assert!(result.passed);
    assert!(result.failures.is_empty());
    assert!(calls.load(Ordering::SeqCst) >= 1);
}

/// Reproduces hegel-go report #2: persists a failing example on the first
/// run, then re-runs with the same database + key and expects the first
/// test case to be a replay of the persisted value.
#[test]
fn run_native_replays_persisted_failure_on_second_run() {
    use crate::backend::Failure;

    let tempdir = tempfile::tempdir().unwrap();
    let db_path = tempdir.path().to_string_lossy().into_owned();

    let settings = Settings::new()
        .test_cases(50)
        .seed(Some(42))
        .database(Some(db_path.clone()));
    let key = Some("replay-smoke");

    // First run: any integer >= 1_000_000 is "interesting", so the engine
    // shrinks down to the boundary and persists it.
    let first_failures = std::sync::Mutex::new(Vec::<i128>::new());
    let result = run_native(&settings, key, |ds, _is_final| {
        let schema = cbor_map! {
            "type" => "integer",
            "min_value" => 0_i64,
            "max_value" => 2_000_000_i64,
        };
        match ds.generate(&schema) {
            Ok(ciborium::Value::Integer(i)) => {
                let n: i128 = i.into();
                if n >= 1_000_000 {
                    first_failures.lock().unwrap().push(n);
                    ds.mark_complete(&TestCaseResult::Interesting(Failure {
                        panic_message: "n >= 1_000_000".to_string(),
                        diagnostic: "n >= 1_000_000\n".to_string(),
                        origin: "n >= 1_000_000".to_string(),
                        reproduce_blob: None,
                    }));
                } else {
                    ds.mark_complete(&TestCaseResult::Valid);
                }
            }
            _ => ds.mark_complete(&TestCaseResult::Overrun),
        }
    });
    assert!(!result.passed, "first run must have failed");
    assert!(
        first_failures
            .lock()
            .unwrap()
            .iter()
            .any(|&n| n >= 1_000_000),
        "first run never observed n>=1_000_000"
    );

    // Second run: same settings, same key. Reuse phase must replay the
    // persisted failure as the very first test case.
    let observed_first = std::sync::Mutex::new(None::<i128>);
    let _ = run_native(&settings, key, |ds, _is_final| {
        let schema = cbor_map! {
            "type" => "integer",
            "min_value" => 0_i64,
            "max_value" => 2_000_000_i64,
        };
        if let Ok(ciborium::Value::Integer(i)) = ds.generate(&schema) {
            let n: i128 = i.into();
            let mut slot = observed_first.lock().unwrap();
            if slot.is_none() {
                *slot = Some(n);
            }
            if n >= 1_000_000 {
                ds.mark_complete(&TestCaseResult::Interesting(Failure {
                    panic_message: "n >= 1_000_000".to_string(),
                    diagnostic: "n >= 1_000_000\n".to_string(),
                    origin: "n >= 1_000_000".to_string(),
                    reproduce_blob: None,
                }));
                return;
            }
        }
        ds.mark_complete(&TestCaseResult::Valid);
    });

    let first = observed_first
        .lock()
        .unwrap()
        .expect("second run never received a test case");
    assert!(
        first >= 1_000_000,
        "expected first replayed test case to satisfy n>=1_000_000, got n={}",
        first
    );
}

/// Hegel-go report #3 regression: a `n >= 1_000_000` property over int64
/// must shrink to exactly 1_000_000 (the predicate boundary).
///
/// The hegel-go agent's follow-up report (run across 100 derandomized
/// seeds with `WithTestCases(100)`) measured a 16% hit rate over the
/// full i64 range and 39% on `[0, 2_000_000]`. We sweep 50 seeds here
/// rather than one — the bar isn't "100% of seeds reach the boundary"
/// (which would over-fit to the current shrinker), but rather "well
/// over half do." If the rate drops back to single digits this test
/// will fail and surface the shrinker regression.
#[test]
fn run_native_shrinks_predicate_boundary_seed_sweep() {
    use crate::backend::Failure;

    let mut hits = 0u32;
    let mut last_values = Vec::<i128>::new();
    for seed in 0u64..50 {
        let settings = Settings::new()
            .test_cases(100)
            .seed(Some(seed))
            .derandomize(true)
            .database(None);
        let last = std::sync::Mutex::new(None::<i128>);
        let _ = run_native(&settings, None, |ds, _is_final| {
            let schema = cbor_map! {
                "type" => "integer",
                "min_value" => i64::MIN,
                "max_value" => i64::MAX,
            };
            if let Ok(ciborium::Value::Integer(i)) = ds.generate(&schema) {
                let n: i128 = i.into();
                if n >= 1_000_000 {
                    *last.lock().unwrap() = Some(n);
                    ds.mark_complete(&TestCaseResult::Interesting(Failure {
                        panic_message: "n >= 1_000_000".to_string(),
                        diagnostic: "n >= 1_000_000\n".to_string(),
                        origin: "n >= 1_000_000".to_string(),
                        reproduce_blob: None,
                    }));
                    return;
                }
            }
            ds.mark_complete(&TestCaseResult::Valid);
        });
        let observed = last.lock().unwrap().unwrap();
        last_values.push(observed);
        if observed == 1_000_000 {
            hits += 1;
        }
    }
    eprintln!("shrinker reached boundary {hits}/50; values: {last_values:?}");
    assert!(
        hits >= 25,
        "shrinker reached the boundary only {}/50 times; observed values: {:?}",
        hits,
        last_values
    );
}

/// Single-seed version of the boundary test, retained as a fast-feedback
/// gate that surfaces total regressions on the lucky-seed path.
#[test]
fn run_native_shrinks_predicate_boundary_to_exact_value() {
    use crate::backend::Failure;

    let settings = Settings::new()
        .test_cases(200)
        .seed(Some(0xc0ffee))
        .database(None);

    let last_failure = std::sync::Mutex::new(None::<i128>);
    let _ = run_native(&settings, None, |ds, _is_final| {
        let schema = cbor_map! {
            "type" => "integer",
            "min_value" => i64::MIN,
            "max_value" => i64::MAX,
        };
        if let Ok(ciborium::Value::Integer(i)) = ds.generate(&schema) {
            let n: i128 = i.into();
            if n >= 1_000_000 {
                *last_failure.lock().unwrap() = Some(n);
                ds.mark_complete(&TestCaseResult::Interesting(Failure {
                    panic_message: "n >= 1_000_000".to_string(),
                    diagnostic: "n >= 1_000_000\n".to_string(),
                    origin: "n >= 1_000_000".to_string(),
                    reproduce_blob: None,
                }));
                return;
            }
        }
        ds.mark_complete(&TestCaseResult::Valid);
    });

    let observed = last_failure
        .lock()
        .unwrap()
        .expect("never observed failure");
    assert_eq!(
        observed, 1_000_000,
        "shrinker should reach the predicate boundary exactly; observed {}",
        observed
    );
}

/// Same as the basic replay test, but exercises i64::MIN..=i64::MAX (no
/// bounds), matching the schema shape hegel-go uses in
/// `TestDatabaseKeyReplaysFailure`. The choice encoding for unbounded
/// integers differs from the small-range case, so this regresses the path
/// that the hegel-go report specifically observed.
#[test]
fn run_native_replays_persisted_failure_with_unbounded_int_schema() {
    use crate::backend::Failure;

    let tempdir = tempfile::tempdir().unwrap();
    let db_path = tempdir.path().to_string_lossy().into_owned();
    let settings = Settings::new()
        .test_cases(100)
        .seed(Some(7))
        .database(Some(db_path));
    let key = Some("replay-unbounded");

    // Explicit i64 bounds (matches what a Go int64 generator sends).
    let schema_for = || {
        cbor_map! {
            "type" => "integer",
            "min_value" => i64::MIN,
            "max_value" => i64::MAX,
        }
    };
    let predicate = |n: i128| n >= 1_000_000;

    // Run #1: collect persisted shrink result.
    let last = std::sync::Mutex::new(None::<i128>);
    let result = run_native(&settings, key, |ds, _is_final| {
        if let Ok(ciborium::Value::Integer(i)) = ds.generate(&schema_for()) {
            let n: i128 = i.into();
            if predicate(n) {
                *last.lock().unwrap() = Some(n);
                ds.mark_complete(&TestCaseResult::Interesting(Failure {
                    panic_message: "n >= 1_000_000".to_string(),
                    diagnostic: "n >= 1_000_000\n".to_string(),
                    origin: "n >= 1_000_000".to_string(),
                    reproduce_blob: None,
                }));
                return;
            }
        }
        ds.mark_complete(&TestCaseResult::Valid);
    });
    assert!(!result.passed);
    let shrunk = last
        .lock()
        .unwrap()
        .expect("first run never observed the failure");

    // Run #2: same settings, observe the first value.
    let observed_first = std::sync::Mutex::new(None::<i128>);
    let _ = run_native(&settings, key, |ds, _is_final| {
        if let Ok(ciborium::Value::Integer(i)) = ds.generate(&schema_for()) {
            let n: i128 = i.into();
            let mut slot = observed_first.lock().unwrap();
            if slot.is_none() {
                *slot = Some(n);
            }
            if predicate(n) {
                ds.mark_complete(&TestCaseResult::Interesting(Failure {
                    panic_message: "n >= 1_000_000".to_string(),
                    diagnostic: "n >= 1_000_000\n".to_string(),
                    origin: "n >= 1_000_000".to_string(),
                    reproduce_blob: None,
                }));
                return;
            }
        }
        ds.mark_complete(&TestCaseResult::Valid);
    });

    let first = observed_first.lock().unwrap().unwrap();
    assert!(
        predicate(first),
        "expected first replayed test case to satisfy predicate (shrunk was {}), got {}",
        shrunk,
        first
    );
}

#[test]
fn run_native_callback_can_generate_via_data_source() {
    let result = run_native(&quiet_settings(3), None, |ds, _is_final| {
        let schema = cbor_map! {"type" => "boolean"};
        let value = ds.generate(&schema).expect("generate succeeded");
        assert!(matches!(value, ciborium::Value::Bool(_)));
        ds.mark_complete(&TestCaseResult::Valid);
    });
    assert!(result.passed);
}

/// A test body that marks any integer `>= 1_000_000` interesting. Used by
/// the reproduce-blob tests to provoke (and later replay) a failure.
fn mark_large_interesting(ds: &(dyn crate::backend::DataSource + Send + Sync)) {
    use crate::backend::Failure;
    let schema = cbor_map! {
        "type" => "integer",
        "min_value" => 0_i64,
        "max_value" => 2_000_000_i64,
    };
    match ds.generate(&schema) {
        Ok(ciborium::Value::Integer(i)) => {
            let n: i128 = i.into();
            if n >= 1_000_000 {
                ds.mark_complete(&TestCaseResult::Interesting(Failure {
                    panic_message: "n >= 1_000_000".to_string(),
                    diagnostic: "n >= 1_000_000\n".to_string(),
                    origin: "n >= 1_000_000".to_string(),
                    reproduce_blob: None,
                }));
            } else {
                ds.mark_complete(&TestCaseResult::Valid);
            }
        }
        _ => ds.mark_complete(&TestCaseResult::Overrun),
    }
}

/// Run the failing property once and return the reproduce blob the engine
/// attached to the (shrunk) counterexample.
fn discover_reproduce_blob() -> String {
    let settings = quiet_settings(200).seed(Some(7));
    let result = run_native(&settings, None, |ds, _is_final| {
        mark_large_interesting(&*ds)
    });
    assert!(!result.passed, "property should have failed");
    result.failures[0]
        .reproduce_blob
        .clone()
        .expect("native failure should carry a reproduce blob")
}

#[test]
fn run_native_reproduce_blob_replays_the_counterexample() {
    let blob = discover_reproduce_blob();

    // Replaying the blob runs exactly the encoded example and re-surfaces
    // the failure, carrying the same blob back.
    let settings = quiet_settings(200).reproduce_failure(Some(blob.clone()));
    let calls = AtomicUsize::new(0);
    let result = run_native(&settings, None, |ds, _is_final| {
        calls.fetch_add(1, Ordering::SeqCst);
        mark_large_interesting(&*ds);
    });

    assert!(!result.passed);
    assert_eq!(result.failures.len(), 1);
    assert_eq!(
        result.failures[0].reproduce_blob.as_deref(),
        Some(blob.as_str())
    );
    // Reproduce mode bypasses generation: it replays once (plus the final
    // replay), far fewer than the 200-case budget.
    assert!(
        calls.load(Ordering::SeqCst) == 1,
        "reproduce mode should not generate"
    );
}

#[test]
fn run_native_reproduce_blob_rejects_an_undecodable_blob() {
    // An undecodable blob is invalid input — it panics rather than producing
    // a `TestRunResult` failure.
    let result = std::panic::catch_unwind(|| {
        let settings = quiet_settings(50).reproduce_failure(Some("not-a-valid-blob".to_string()));
        run_native(&settings, None, |ds, _is_final| {
            ds.mark_complete(&TestCaseResult::Valid);
        });
    });
    let payload = result.unwrap_err();
    let msg = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        msg.contains("could not be decoded"),
        "unexpected panic message: {msg}"
    );
}

#[test]
fn run_native_reproduce_blob_that_no_longer_fails_is_reported() {
    let blob = discover_reproduce_blob();

    // A "fixed" test body that never reports interesting: replaying a stale
    // blob must surface that rather than silently passing.
    let settings = quiet_settings(50).reproduce_failure(Some(blob));
    let result = run_native(&settings, None, |ds, _is_final| {
        let schema = cbor_map! {
            "type" => "integer",
            "min_value" => 0_i64,
            "max_value" => 2_000_000_i64,
        };
        let _ = ds.generate(&schema);
        ds.mark_complete(&TestCaseResult::Valid);
    });
    assert!(!result.passed);
    assert!(
        result.failures[0]
            .diagnostic
            .contains("no longer reproduces"),
        "unexpected diagnostic: {}",
        result.failures[0].diagnostic
    );
}

#[test]
fn run_native_reproduce_blob_takes_precedence_over_single_test_case_mode() {
    use crate::runner::Mode;

    // A blob replay must work regardless of `mode`: `SingleTestCase` routes
    // through a different entry point than the generative loop, but a set
    // `reproduce_failure` takes precedence and still replays the example.
    let blob = discover_reproduce_blob();
    let settings = quiet_settings(200)
        .mode(Mode::SingleTestCase)
        .reproduce_failure(Some(blob.clone()));
    let result = run_native(&settings, None, |ds, _is_final| {
        mark_large_interesting(&*ds)
    });

    assert!(!result.passed);
    assert_eq!(result.failures.len(), 1);
    assert_eq!(
        result.failures[0].reproduce_blob.as_deref(),
        Some(blob.as_str())
    );
}

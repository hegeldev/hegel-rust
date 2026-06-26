use super::*;
use crate::backend::TestCaseResult;
use crate::cbor_utils::cbor_map;
use crate::settings::{Database, Settings};
use std::sync::atomic::{AtomicUsize, Ordering};

fn quiet_settings(test_cases: u64) -> Settings {
    let mut s = Settings::new().test_cases(test_cases);
    s.database = Database::Disabled;
    s
}

#[test]
fn run_native_invokes_callback_and_returns_passing_result() {
    let calls = AtomicUsize::new(0);
    let result = run_native(&quiet_settings(5), None, |ds| {
        calls.fetch_add(1, Ordering::SeqCst);
        ds.mark_complete(&TestCaseResult::Valid);
    });
    let result = result.unwrap();
    assert!(result.failures.is_empty());
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

    let first_failures = std::sync::Mutex::new(Vec::<i128>::new());
    let result = run_native(&settings, key, |ds| {
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
    assert!(
        !result.unwrap().failures.is_empty(),
        "first run must have failed"
    );
    assert!(
        first_failures
            .lock()
            .unwrap()
            .iter()
            .any(|&n| n >= 1_000_000),
        "first run never observed n>=1_000_000"
    );

    let observed_first = std::sync::Mutex::new(None::<i128>);
    let _ = run_native(&settings, key, |ds| {
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
    let mut hits = 0u32;
    let mut shrunk_values = Vec::<i128>::new();
    for seed in 0u64..50 {
        let settings = Settings::new()
            .test_cases(100)
            .seed(Some(seed))
            .derandomize(true)
            .database(None);
        let result = run_native(&settings, None, |ds| mark_above_million(&*ds)).unwrap();
        let blob = result.failures[0]
            .reproduce_blob
            .as_deref()
            .expect("native failure carries a reproduce blob");
        let observed = replay_above_million_blob(blob);
        shrunk_values.push(observed);
        if observed == 1_000_000 {
            hits += 1;
        }
    }
    eprintln!("shrinker reached boundary {hits}/50; values: {shrunk_values:?}");
    assert!(
        hits >= 25,
        "shrinker reached the boundary only {}/50 times; shrunk values: {:?}",
        hits,
        shrunk_values
    );
}

/// The body of the predicate-boundary tests: any `i64` `>= 1_000_000` is
/// interesting, over the full `i64` range.
fn mark_above_million(ds: &(dyn crate::backend::DataSource + Send + Sync)) {
    use crate::backend::Failure;
    let schema = cbor_map! {
        "type" => "integer",
        "min_value" => i64::MIN,
        "max_value" => i64::MAX,
    };
    if let Ok(ciborium::Value::Integer(i)) = ds.generate(&schema) {
        let n: i128 = i.into();
        if n >= 1_000_000 {
            ds.mark_complete(&TestCaseResult::Interesting(Failure {
                origin: "n >= 1_000_000".to_string(),
                reproduce_blob: None,
            }));
            return;
        }
    }
    ds.mark_complete(&TestCaseResult::Valid);
}

/// Replay the integer encoded in a full-`i64`-range reproduce blob — i.e. the
/// value the shrinker minimised to, read back out of the result's blob.
fn replay_above_million_blob(blob: &str) -> i128 {
    let ds = data_source_for_blob(&quiet_settings(1), blob).unwrap();
    let schema = cbor_map! {
        "type" => "integer",
        "min_value" => i64::MIN,
        "max_value" => i64::MAX,
    };
    let Ok(ciborium::Value::Integer(i)) = ds.generate(&schema) else {
        panic!("expected an integer draw");
    };
    ds.mark_complete(&TestCaseResult::Valid);
    i.into()
}

/// Single-seed version of the boundary test, retained as a fast-feedback
/// gate that surfaces total regressions on the lucky-seed path.
#[test]
fn run_native_shrinks_predicate_boundary_to_exact_value() {
    let settings = Settings::new()
        .test_cases(200)
        .seed(Some(0xc0ffee))
        .database(None);

    let result = run_native(&settings, None, |ds| mark_above_million(&*ds)).unwrap();
    let blob = result.failures[0]
        .reproduce_blob
        .as_deref()
        .expect("native failure carries a reproduce blob");
    let observed = replay_above_million_blob(blob);
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

    let schema_for = || {
        cbor_map! {
            "type" => "integer",
            "min_value" => i64::MIN,
            "max_value" => i64::MAX,
        }
    };
    let predicate = |n: i128| n >= 1_000_000;

    let last = std::sync::Mutex::new(None::<i128>);
    let result = run_native(&settings, key, |ds| {
        if let Ok(ciborium::Value::Integer(i)) = ds.generate(&schema_for()) {
            let n: i128 = i.into();
            if predicate(n) {
                *last.lock().unwrap() = Some(n);
                ds.mark_complete(&TestCaseResult::Interesting(Failure {
                    origin: "n >= 1_000_000".to_string(),
                    reproduce_blob: None,
                }));
                return;
            }
        }
        ds.mark_complete(&TestCaseResult::Valid);
    });
    assert!(!result.unwrap().failures.is_empty());
    let shrunk = last
        .lock()
        .unwrap()
        .expect("first run never observed the failure");

    let observed_first = std::sync::Mutex::new(None::<i128>);
    let _ = run_native(&settings, key, |ds| {
        if let Ok(ciborium::Value::Integer(i)) = ds.generate(&schema_for()) {
            let n: i128 = i.into();
            let mut slot = observed_first.lock().unwrap();
            if slot.is_none() {
                *slot = Some(n);
            }
            if predicate(n) {
                ds.mark_complete(&TestCaseResult::Interesting(Failure {
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
    let result = run_native(&quiet_settings(3), None, |ds| {
        let schema = cbor_map! {"type" => "boolean"};
        let value = ds.generate(&schema).expect("generate succeeded");
        assert!(matches!(value, ciborium::Value::Bool(_)));
        ds.mark_complete(&TestCaseResult::Valid);
    });
    assert!(result.unwrap().failures.is_empty());
}

/// A test body that marks any integer `>= 1_000_000` interesting. Used by
/// the blob tests to provoke (and later replay) a failure.
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
    let result = run_native(&settings, None, |ds| mark_large_interesting(&*ds)).unwrap();
    assert!(!result.failures.is_empty(), "property should have failed");
    result.failures[0]
        .reproduce_blob
        .clone()
        .expect("native failure should carry a reproduce blob")
}

#[test]
fn data_source_for_blob_replays_the_counterexample() {
    let blob = discover_reproduce_blob();

    let ds = data_source_for_blob(&quiet_settings(1), &blob).unwrap();
    let schema = cbor_map! {
        "type" => "integer",
        "min_value" => 0_i64,
        "max_value" => 2_000_000_i64,
    };
    let Ok(ciborium::Value::Integer(i)) = ds.generate(&schema) else {
        panic!("expected an integer draw");
    };
    let n: i128 = i.into();
    assert!(
        n >= 1_000_000,
        "replayed value {n} should still violate the property"
    );
    ds.mark_complete(&TestCaseResult::Valid);
}

#[test]
fn data_source_for_blob_logs_at_debug_verbosity() {
    let blob = discover_reproduce_blob();
    let settings = quiet_settings(1).verbosity(crate::settings::Verbosity::Debug);
    assert!(data_source_for_blob(&settings, &blob).is_some());
}

#[test]
fn data_source_for_blob_rejects_an_undecodable_blob() {
    assert!(data_source_for_blob(&quiet_settings(1), "not-a-valid-blob").is_none());
}

#[test]
fn run_native_single_test_case_reports_the_failure() {
    use crate::backend::Failure;

    let settings = quiet_settings(1).mode(crate::settings::Mode::SingleTestCase);
    let calls = AtomicUsize::new(0);
    let result = run_native(&settings, None, |ds| {
        calls.fetch_add(1, Ordering::SeqCst);
        ds.mark_complete(&TestCaseResult::Interesting(Failure {
            origin: "single-case bug".to_string(),
            reproduce_blob: None,
        }));
    })
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].origin, "single-case bug");
    assert!(result.failures[0].reproduce_blob.is_none());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the single test case runs exactly once"
    );
}

#[test]
fn run_native_single_test_case_passes_cleanly() {
    let settings = quiet_settings(1).mode(crate::settings::Mode::SingleTestCase);
    let result = run_native(&settings, None, |ds| {
        ds.mark_complete(&TestCaseResult::Valid);
    })
    .unwrap();
    assert!(result.failures.is_empty());
}

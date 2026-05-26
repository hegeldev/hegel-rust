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
/// must shrink to exactly 1_000_000 (the predicate boundary). The
/// hegel-go report saw 2^20-1 (1_048_575); the shrinker improvements that
/// landed before this PR was rebased onto main fixed it. This test
/// guards against the regression coming back.
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

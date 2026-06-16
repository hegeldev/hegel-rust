//! Embedded tests for the libhegel C-ABI boundary (`crate::ffi`).
//!
//! These drive whole runs through the safe wrappers — settings building, the
//! pull loop, every per-test-case primitive, result inspection, and blob
//! replay — so each wrapper is exercised end-to-end against the real engine in
//! `hegel-c`. They are the frontend's analogue of `hegel-c/tests/smoke.rs`,
//! but going through the Rust wrappers the rest of hegeltest will use.

use super::*;
use crate::runner::{Backend, Settings};
use ciborium::Value;

#[test]
fn ffi_settings_builds_with_each_explicit_backend() {
    // Exercises `map_backend`'s explicit arms (the default tests leave the
    // backend at AUTO). Building the settings handle is enough to hit them.
    for backend in [Backend::Default, Backend::Urandom] {
        let _sh = SettingsHandle::build(&test_settings(1).backend(backend), None);
    }
}

fn int_schema(min: i64, max: i64) -> Vec<u8> {
    let v = Value::Map(vec![
        (Value::Text("type".into()), Value::Text("integer".into())),
        (Value::Text("min_value".into()), Value::Integer(min.into())),
        (Value::Text("max_value".into()), Value::Integer(max.into())),
    ]);
    let mut buf = Vec::new();
    ciborium::ser::into_writer(&v, &mut buf).unwrap();
    buf
}

fn decode_int(bytes: &[u8]) -> i128 {
    match ciborium::de::from_reader::<Value, _>(bytes).unwrap() {
        Value::Integer(i) => i.into(),
        other => panic!("expected integer, got {other:?}"),
    }
}

/// A deterministic, database-free settings for tests.
fn test_settings(seed: u64) -> Settings {
    Settings::new()
        .test_cases(25)
        .database(None)
        .derandomize(true)
        .seed(Some(seed))
}

const VALID: hegel_c::hegel_status_t = hegel_c::hegel_status_t::HEGEL_STATUS_VALID;
const OVERRUN: hegel_c::hegel_status_t = hegel_c::hegel_status_t::HEGEL_STATUS_OVERRUN;
const INTERESTING: hegel_c::hegel_status_t = hegel_c::hegel_status_t::HEGEL_STATUS_INTERESTING;

#[test]
fn ffi_drives_a_passing_run_exercising_every_primitive() {
    let settings = test_settings(1);
    let sh = SettingsHandle::build(&settings, None);
    let run = RunHandle::start(&sh).unwrap();
    let schema = int_schema(0, 100);

    let mut cases = 0usize;
    while let Some(tc) = run.next_test_case() {
        cases += 1;
        // is_final_replay is queryable on every case (false during exploration).
        let _ = tc.is_final_replay();

        // A span wrapping a small engine-managed collection of ints, rejecting
        // any zero element so collection_reject is exercised.
        tc.start_span(hegel_c::HEGEL_LABEL_LIST).unwrap();
        let cid = tc.new_collection(0, Some(3)).unwrap();
        let mut drew_overrun = false;
        loop {
            match tc.collection_more(cid) {
                Ok(true) => {}
                Ok(false) => break,
                Err(hegel_c::HEGEL_E_STOP_TEST) => {
                    drew_overrun = true;
                    break;
                }
                Err(rc) => panic!("collection_more rc={rc}"),
            }
            match tc.generate(&schema) {
                Ok(bytes) => {
                    if decode_int(&bytes) == 0 {
                        tc.collection_reject(cid, Some("zero")).unwrap();
                    }
                }
                Err(hegel_c::HEGEL_E_STOP_TEST) => {
                    drew_overrun = true;
                    break;
                }
                Err(rc) => panic!("generate rc={rc}"),
            }
        }
        tc.stop_span(false).unwrap();
        if drew_overrun {
            tc.mark_complete(OVERRUN, None).unwrap();
            continue;
        }

        // A variable pool: register one variable and draw it back.
        let pool = tc.new_pool().unwrap();
        let added = tc.pool_add(pool).unwrap();
        match tc.pool_generate(pool, false) {
            Ok(drawn) => assert_eq!(drawn, added, "non-consuming draw returns the added id"),
            Err(hegel_c::HEGEL_E_STOP_TEST) => {
                tc.mark_complete(OVERRUN, None).unwrap();
                continue;
            }
            Err(rc) => panic!("pool_generate rc={rc}"),
        }

        // A targeting observation, then the actual draw the "property" sees.
        tc.target(0.0, "score").unwrap();
        match tc.generate(&schema) {
            Ok(bytes) => {
                let n = decode_int(&bytes);
                assert!((0..=100).contains(&n));
                tc.mark_complete(VALID, None).unwrap();
            }
            Err(hegel_c::HEGEL_E_STOP_TEST) => tc.mark_complete(OVERRUN, None).unwrap(),
            Err(rc) => panic!("generate rc={rc}"),
        }
    }
    assert!(cases >= 1);

    let result = run.result();
    assert!(result.status() == hegel_c::hegel_run_status_t::HEGEL_RUN_STATUS_PASSED);
    assert_eq!(result.failure_count(), 0);
    assert!(result.error().is_none());
}

#[test]
fn ffi_reports_failure_with_blob_then_replays_it() {
    let settings = test_settings(7);
    let sh = SettingsHandle::build(&settings, None);
    let run = RunHandle::start(&sh).unwrap();
    let schema = int_schema(0, 100);

    // Property: n must be 0. Fails for any n > 0 and shrinks to the minimal
    // counterexample, 1.
    let origin = "n != 0";
    while let Some(tc) = run.next_test_case() {
        match tc.generate(&schema) {
            Ok(bytes) => {
                if decode_int(&bytes) != 0 {
                    tc.mark_complete(INTERESTING, Some(origin)).unwrap();
                } else {
                    tc.mark_complete(VALID, None).unwrap();
                }
            }
            Err(hegel_c::HEGEL_E_STOP_TEST) => tc.mark_complete(OVERRUN, None).unwrap(),
            Err(rc) => panic!("generate rc={rc}"),
        }
    }

    let result = run.result();
    assert!(result.status() == hegel_c::hegel_run_status_t::HEGEL_RUN_STATUS_FAILED);
    assert_eq!(result.failure_count(), 1);
    let failure = result.failure(0).unwrap();
    // An out-of-range failure index yields None rather than faulting.
    assert!(
        result.failure(result.failure_count()).is_none(),
        "failure index past the end must be None"
    );
    let blob = failure
        .reproduce_blob
        .expect("a shrunk failure carries a blob");

    // Replay the blob as a standalone, caller-owned, final test case.
    let sh2 = SettingsHandle::build(&settings, None);
    let replay = CTestCase::from_blob(&sh2, &blob).unwrap();
    assert!(
        replay.is_final_replay(),
        "a blob replay is the final example"
    );
    let bytes = replay.generate(&schema).unwrap();
    assert_eq!(
        decode_int(&bytes),
        1,
        "the blob replays the minimal counterexample"
    );
    replay.mark_complete(INTERESTING, Some(origin)).unwrap();
    // `replay` is owned; dropping it frees the standalone test case.
}

#[test]
fn ffi_from_blob_rejects_undecodable_input() {
    let settings = test_settings(1);
    let sh = SettingsHandle::build(&settings, None);
    let err = match CTestCase::from_blob(&sh, "not a valid base64 hegel blob!!!") {
        Err(e) => e,
        Ok(_) => panic!("expected an undecodable blob to be rejected"),
    };
    assert!(!err.is_empty(), "an undecodable blob yields a diagnostic");
}

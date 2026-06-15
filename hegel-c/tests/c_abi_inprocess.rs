//! In-process exercise of the C ABI's error / edge paths.
//!
//! `smoke.rs` drives the C ABI the way a non-Rust caller does — by dlopening
//! the built cdylib — which is the right fidelity test but doesn't contribute
//! to coverage (the dlopened library isn't the instrumented build). These
//! tests instead call the exported `hegel_*` functions directly as ordinary
//! Rust items, so the null-handle / invalid-argument / lifecycle-misuse paths
//! they hit are measured. The happy path is covered by hegeltest driving the
//! engine over this same ABI.

use hegel_c::{
    HegelRun, HegelTestCase, HEGEL_E_INVALID_HANDLE, HEGEL_OK, hegel_backend_t,
    hegel_collection_more, hegel_collection_reject, hegel_generate, hegel_mark_complete,
    hegel_mode_t, hegel_new_collection, hegel_new_pool, hegel_next_test_case, hegel_pool_add,
    hegel_pool_generate, hegel_run_free, hegel_run_result, hegel_run_start, hegel_settings_backend,
    hegel_settings_database, hegel_settings_database_key, hegel_settings_free, hegel_settings_mode,
    hegel_settings_new, hegel_start_span, hegel_status_t, hegel_stop_span, hegel_target,
    hegel_test_case_free, hegel_test_case_from_blob, hegel_test_case_is_final_replay,
};
use std::ffi::CString;
use std::os::raw::c_char;
use std::ptr;

fn last_error() -> String {
    let p = hegel_c::hegel_last_error_message();
    if p.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr(p) }
            .to_string_lossy()
            .into_owned()
    }
}

#[test]
fn null_handles_are_rejected_without_crashing() {
    unsafe {
        // Settings setters tolerate a null handle (documented no-op).
        hegel_settings_mode(ptr::null_mut(), hegel_mode_t::HEGEL_MODE_TEST_RUN);
        hegel_settings_backend(ptr::null_mut(), hegel_backend_t::HEGEL_BACKEND_AUTO);

        // Handle-returning entry points return null + set the thread-local error.
        assert!(hegel_run_start(ptr::null()).is_null());
        assert!(!last_error().is_empty());
        assert!(hegel_next_test_case(ptr::null_mut()).is_null());
        assert!(hegel_run_result(ptr::null_mut()).is_null());
        assert!(hegel_test_case_from_blob(ptr::null(), c"AAEC".as_ptr()).is_null());

        // Free is null-safe.
        hegel_settings_free(ptr::null_mut());
        hegel_run_free(ptr::null_mut());
        hegel_test_case_free(ptr::null_mut());

        // Per-test-case primitives on a null handle report an invalid handle.
        let tc: *mut HegelTestCase = ptr::null_mut();
        let mut out_ptr: *const u8 = ptr::null();
        let mut out_len = 0usize;
        let schema = [0u8];
        assert_eq!(
            hegel_generate(tc, schema.as_ptr(), schema.len(), &mut out_ptr, &mut out_len),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(hegel_start_span(tc, 1), HEGEL_E_INVALID_HANDLE);
        assert_eq!(hegel_stop_span(tc, false), HEGEL_E_INVALID_HANDLE);
        let mut id = 0i64;
        assert_eq!(
            hegel_new_collection(tc, 0, u64::MAX, &mut id),
            HEGEL_E_INVALID_HANDLE
        );
        let mut more = false;
        assert_eq!(
            hegel_collection_more(tc, 0, &mut more),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_collection_reject(tc, 0, ptr::null()),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(hegel_new_pool(tc, &mut id), HEGEL_E_INVALID_HANDLE);
        assert_eq!(hegel_pool_add(tc, 0, &mut id), HEGEL_E_INVALID_HANDLE);
        assert_eq!(
            hegel_pool_generate(tc, 0, false, &mut id),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(hegel_target(tc, 0.0, c"x".as_ptr()), HEGEL_E_INVALID_HANDLE);
        assert_ne!(
            hegel_mark_complete(tc, hegel_status_t::HEGEL_STATUS_VALID, ptr::null()),
            HEGEL_OK
        );
        assert!(!hegel_test_case_is_final_replay(tc));
    }
}

#[test]
fn settings_string_setters_handle_bad_input() {
    unsafe {
        let s = hegel_settings_new();
        // database(null) leaves the default in place; key(null) clears it.
        hegel_settings_database(s, ptr::null());
        hegel_settings_database_key(s, ptr::null());

        // Non-UTF-8 bytes are rejected (recorded as an error), not honoured.
        let bad: [c_char; 2] = [0xFFu8 as c_char, 0];
        hegel_settings_database(s, bad.as_ptr());
        assert!(last_error().contains("not valid UTF-8"));
        hegel_settings_database_key(s, bad.as_ptr());
        assert!(last_error().contains("not valid UTF-8"));

        hegel_settings_free(s);
    }
}

#[test]
fn from_blob_rejects_bad_input() {
    unsafe {
        let s = hegel_settings_new();
        assert!(hegel_test_case_from_blob(s, ptr::null()).is_null());
        assert!(last_error().contains("null"));
        let bad: [c_char; 2] = [0xFFu8 as c_char, 0];
        assert!(hegel_test_case_from_blob(s, bad.as_ptr()).is_null());
        assert!(last_error().contains("UTF-8"));
        let garbage = CString::new("!!! not a blob !!!").unwrap();
        assert!(hegel_test_case_from_blob(s, garbage.as_ptr()).is_null());
        assert!(last_error().contains("could not be decoded"));
        hegel_settings_free(s);
    }
}

/// Drive a short passing run with the backend pinned, exercising
/// `hegel_settings_backend`'s explicit arm and the run lifecycle, plus the
/// misuse paths: reading the result before the run is drained, and asking for
/// the next case before completing the current one.
#[test]
fn explicit_backend_run_and_lifecycle_misuse() {
    unsafe {
        let s = hegel_settings_new();
        hegel_settings_backend(s, hegel_backend_t::HEGEL_BACKEND_DEFAULT);
        let empty = CString::new("").unwrap();
        hegel_settings_database(s, empty.as_ptr());
        hegel_c::hegel_settings_test_cases(s, 5);
        hegel_c::hegel_settings_seed(s, 1, true);

        let run: *mut HegelRun = hegel_run_start(s);
        assert!(!run.is_null());

        // Reading the result before the run is drained is an error.
        assert!(hegel_run_result(run).is_null());

        let schema = integer_schema();
        let tc = hegel_next_test_case(run);
        assert!(!tc.is_null());

        // Requesting the next case before completing this one is rejected.
        assert!(hegel_next_test_case(run).is_null());
        assert!(last_error().contains("not marked complete"));

        let mut out_ptr: *const u8 = ptr::null();
        let mut out_len = 0usize;
        assert_eq!(
            hegel_generate(tc, schema.as_ptr(), schema.len(), &mut out_ptr, &mut out_len),
            HEGEL_OK
        );
        assert_eq!(
            hegel_mark_complete(tc, hegel_status_t::HEGEL_STATUS_VALID, ptr::null()),
            HEGEL_OK
        );

        // Drain the rest normally.
        loop {
            let tc = hegel_next_test_case(run);
            if tc.is_null() {
                break;
            }
            let mut p: *const u8 = ptr::null();
            let mut n = 0usize;
            assert_eq!(
                hegel_generate(tc, schema.as_ptr(), schema.len(), &mut p, &mut n),
                HEGEL_OK
            );
            hegel_mark_complete(tc, hegel_status_t::HEGEL_STATUS_VALID, ptr::null());
        }

        let result = hegel_run_result(run);
        assert!(!result.is_null());
        hegel_run_free(run);
        hegel_settings_free(s);
    }
}

/// Freeing a run while a test case is still in flight (the caller bailed out
/// early) must abort and join the worker without deadlocking.
#[test]
fn run_free_with_undrained_case_does_not_deadlock() {
    unsafe {
        let s = hegel_settings_new();
        let empty = CString::new("").unwrap();
        hegel_settings_database(s, empty.as_ptr());
        let run = hegel_run_start(s);
        assert!(!run.is_null());
        let tc = hegel_next_test_case(run);
        assert!(!tc.is_null());
        // Drop everything without marking the case complete.
        hegel_run_free(run);
        hegel_settings_free(s);
    }
}

fn integer_schema() -> Vec<u8> {
    use ciborium::value::Value;
    let v = Value::Map(vec![
        (Value::Text("type".into()), Value::Text("integer".into())),
        (Value::Text("min_value".into()), Value::Integer(0.into())),
        (Value::Text("max_value".into()), Value::Integer(100.into())),
    ]);
    let mut buf = Vec::new();
    ciborium::ser::into_writer(&v, &mut buf).unwrap();
    buf
}

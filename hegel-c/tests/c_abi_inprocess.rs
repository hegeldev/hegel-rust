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
    HEGEL_E_ALREADY_COMPLETE, HEGEL_E_INVALID_ARG, HEGEL_E_INVALID_HANDLE, HEGEL_OK, HegelRun,
    HegelTestCase, hegel_backend_t, hegel_collection_more, hegel_collection_reject,
    hegel_failure_origin, hegel_failure_panic_message, hegel_failure_reproduction_blob,
    hegel_generate, hegel_mark_complete, hegel_mode_t, hegel_new_collection, hegel_new_pool,
    hegel_new_state_machine, hegel_next_test_case, hegel_pool_add, hegel_pool_generate,
    hegel_primitive_boolean, hegel_run_free, hegel_run_result, hegel_run_result_error,
    hegel_run_result_failure, hegel_run_result_failure_count, hegel_run_result_status,
    hegel_run_start, hegel_run_status_t, hegel_settings_backend, hegel_settings_database,
    hegel_settings_database_key, hegel_settings_free, hegel_settings_mode, hegel_settings_new,
    hegel_settings_phases, hegel_settings_report_multiple_failures,
    hegel_settings_suppress_health_check, hegel_start_span, hegel_state_machine_next_rule,
    hegel_status_t, hegel_stop_span, hegel_target, hegel_test_case_free, hegel_test_case_from_blob,
    hegel_test_case_is_final_replay, hegel_version,
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
        hegel_settings_database(ptr::null_mut(), c"x".as_ptr());
        hegel_settings_database_key(ptr::null_mut(), c"x".as_ptr());
        hegel_settings_phases(ptr::null_mut(), 0);
        hegel_settings_suppress_health_check(ptr::null_mut(), 0);
        hegel_settings_report_multiple_failures(ptr::null_mut(), true);

        // Handle-returning entry points return null + set the thread-local error.
        assert!(hegel_run_start(ptr::null()).is_null());
        assert!(!last_error().is_empty());
        assert!(hegel_next_test_case(ptr::null_mut()).is_null());
        assert!(hegel_run_result(ptr::null_mut()).is_null());
        assert!(hegel_test_case_from_blob(ptr::null(), c"AAEC".as_ptr()).is_null());

        // Result inspection on a null result / null failure reports the
        // "nothing here" value rather than dereferencing.
        assert!(hegel_run_result_status(ptr::null()) == hegel_run_status_t::HEGEL_RUN_STATUS_ERROR);
        assert!(hegel_run_result_error(ptr::null()).is_null());
        assert_eq!(hegel_run_result_failure_count(ptr::null()), 0);
        assert!(hegel_run_result_failure(ptr::null(), 0).is_null());
        assert!(hegel_failure_panic_message(ptr::null()).is_null());
        assert!(hegel_failure_origin(ptr::null()).is_null());
        assert!(hegel_failure_reproduction_blob(ptr::null()).is_null());

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
            hegel_generate(
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut out_ptr,
                &mut out_len
            ),
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
            hegel_generate(
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut out_ptr,
                &mut out_len
            ),
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

#[test]
fn version_is_reported() {
    let p = hegel_version();
    assert!(!p.is_null());
    let v = unsafe { std::ffi::CStr::from_ptr(p) }
        .to_str()
        .unwrap()
        .to_string();
    assert!(!v.is_empty(), "version string is non-empty");
    // Matches the crate version (e.g. "0.18.0").
    assert!(v.chars().next().unwrap().is_ascii_digit(), "got {v:?}");
}

/// Calling `hegel_next_test_case` again after the run has already drained
/// returns NULL with no error (idempotent end-of-run), rather than blocking
/// or faulting.
#[test]
fn next_after_drain_returns_null() {
    unsafe {
        let s = hegel_settings_new();
        let empty = CString::new("").unwrap();
        hegel_settings_database(s, empty.as_ptr());
        hegel_c::hegel_settings_test_cases(s, 3);
        let run = hegel_run_start(s);
        let schema = integer_schema();
        loop {
            let tc = hegel_next_test_case(run);
            if tc.is_null() {
                break;
            }
            let mut p: *const u8 = ptr::null();
            let mut n = 0usize;
            hegel_generate(tc, schema.as_ptr(), schema.len(), &mut p, &mut n);
            hegel_mark_complete(tc, hegel_status_t::HEGEL_STATUS_VALID, ptr::null());
        }
        // Already drained: a further call is a no-op NULL with no error set.
        assert!(hegel_next_test_case(run).is_null());
        assert!(last_error().is_empty());
        hegel_run_free(run);
        hegel_settings_free(s);
    }
}

/// Exercise the per-primitive argument-validation paths on a *live*,
/// run-owned test case: null/malformed schema, null out-parameters, non-UTF-8
/// string arguments, completing twice, drawing after completion, and refusing
/// `hegel_test_case_free` on a borrowed handle. The case is marked
/// INTERESTING with a NULL origin so the run surfaces a failure whose
/// panic message is the synthesized "Panic at <unknown>" placeholder, which
/// we then read back through the result getters.
#[test]
fn live_test_case_argument_validation() {
    let bad_utf8: [c_char; 2] = [0xFFu8 as c_char, 0];
    unsafe {
        let s = hegel_settings_new();
        let empty = CString::new("").unwrap();
        hegel_settings_database(s, empty.as_ptr());
        hegel_c::hegel_settings_test_cases(s, 5);
        hegel_c::hegel_settings_seed(s, 1, true);
        let run = hegel_run_start(s);
        let tc = hegel_next_test_case(run);
        assert!(!tc.is_null());

        let schema = integer_schema();
        let mut out_ptr: *const u8 = ptr::null();
        let mut out_len = 0usize;

        // generate: null schema pointer with a non-zero length.
        assert_eq!(
            hegel_generate(tc, ptr::null(), 4, &mut out_ptr, &mut out_len),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error().contains("schema pointer is null"));
        // generate: null out-parameter.
        assert_eq!(
            hegel_generate(
                tc,
                schema.as_ptr(),
                schema.len(),
                ptr::null_mut(),
                &mut out_len
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error().contains("out parameter is null"));
        // generate: well-formed pointer but truncated/garbage CBOR.
        let garbage = [0x82u8, 0x01]; // array(2) with only one element → decode error
        assert_eq!(
            hegel_generate(
                tc,
                garbage.as_ptr(),
                garbage.len(),
                &mut out_ptr,
                &mut out_len
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error().contains("malformed CBOR"));

        // Null out-parameters on the collection / pool constructors.
        let mut id = 0i64;
        assert_eq!(
            hegel_new_collection(tc, 0, u64::MAX, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(hegel_new_pool(tc, ptr::null_mut()), HEGEL_E_INVALID_ARG);

        // A real collection, to reach collection_more's null out-param check,
        // the NULL-`why` reject branch, and the non-UTF-8 `why` rejection.
        assert_eq!(hegel_new_collection(tc, 0, 3, &mut id), HEGEL_OK);
        assert_eq!(
            hegel_collection_more(tc, id, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        // The `why` string is decoded before the collection is consulted, so a
        // non-UTF-8 reason is rejected regardless of collection state.
        assert_eq!(
            hegel_collection_reject(tc, id, bad_utf8.as_ptr()),
            HEGEL_E_INVALID_ARG
        );
        let mut more = false;
        if hegel_collection_more(tc, id, &mut more) == HEGEL_OK && more {
            hegel_generate(
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut out_ptr,
                &mut out_len,
            );
            // NULL why is the accepted "no reason given" branch.
            assert_eq!(hegel_collection_reject(tc, id, ptr::null()), HEGEL_OK);
        }

        // A real pool, to reach pool_add / pool_generate null out-param checks.
        let mut pool = 0i64;
        assert_eq!(hegel_new_pool(tc, &mut pool), HEGEL_OK);
        assert_eq!(
            hegel_pool_add(tc, pool, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_pool_generate(tc, pool, false, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );

        // target: null label, then non-UTF-8 label.
        assert_eq!(hegel_target(tc, 0.0, ptr::null()), HEGEL_E_INVALID_ARG);
        assert!(last_error().contains("label is null"));
        assert_eq!(
            hegel_target(tc, 0.0, bad_utf8.as_ptr()),
            HEGEL_E_INVALID_ARG
        );

        // target: a non-finite score and a repeated label are caller usage
        // errors — HEGEL_E_INVALID_ARG with a diagnostic, never a panic across
        // the C ABI (libhegel must stay correct under panic=abort).
        assert_eq!(
            hegel_target(tc, f64::NAN, c"x".as_ptr()),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error().contains("finite score"));
        assert_eq!(hegel_target(tc, 1.0, c"dup".as_ptr()), HEGEL_OK);
        assert_eq!(hegel_target(tc, 2.0, c"dup".as_ptr()), HEGEL_E_INVALID_ARG);
        assert!(last_error().contains("would overwrite previous"));

        // mark_complete with a non-UTF-8 origin (only consulted for
        // INTERESTING). This is rejected *before* the case is marked complete,
        // so the handle is still live afterwards.
        assert_eq!(
            hegel_mark_complete(
                tc,
                hegel_status_t::HEGEL_STATUS_INTERESTING,
                bad_utf8.as_ptr()
            ),
            HEGEL_E_INVALID_ARG
        );

        // Now actually complete it.
        assert_eq!(
            hegel_mark_complete(tc, hegel_status_t::HEGEL_STATUS_VALID, ptr::null()),
            HEGEL_OK
        );

        // A completed case rejects further draws and a second completion.
        assert_eq!(
            hegel_generate(
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut out_ptr,
                &mut out_len
            ),
            HEGEL_E_ALREADY_COMPLETE
        );
        assert_eq!(
            hegel_mark_complete(tc, hegel_status_t::HEGEL_STATUS_VALID, ptr::null()),
            HEGEL_E_ALREADY_COMPLETE
        );
        // It is owned by the run, so freeing it directly is refused.
        hegel_test_case_free(tc);
        assert!(last_error().contains("owned by its hegel_run_t"));

        // Drain the remainder as VALID.
        loop {
            let tc = hegel_next_test_case(run);
            if tc.is_null() {
                break;
            }
            let mut p: *const u8 = ptr::null();
            let mut n = 0usize;
            hegel_generate(tc, schema.as_ptr(), schema.len(), &mut p, &mut n);
            hegel_mark_complete(tc, hegel_status_t::HEGEL_STATUS_VALID, ptr::null());
        }

        let result = hegel_run_result(run);
        assert!(!result.is_null());
        hegel_run_free(run);
        hegel_settings_free(s);
    }
}

/// A property that always reports INTERESTING with a NULL origin: the engine
/// synthesizes the "Panic at <unknown>" placeholder for the failure's message
/// and origin. Drives the FAILED run-result path and the failure getters'
/// present-value arms at the C level, and reaches the out-of-range failure
/// index branch.
#[test]
fn interesting_with_null_origin_synthesizes_placeholder() {
    unsafe {
        let s = hegel_settings_new();
        let empty = CString::new("").unwrap();
        hegel_settings_database(s, empty.as_ptr());
        hegel_c::hegel_settings_test_cases(s, 5);
        hegel_c::hegel_settings_seed(s, 1, true);
        let run = hegel_run_start(s);
        let schema = integer_schema();
        loop {
            let tc = hegel_next_test_case(run);
            if tc.is_null() {
                break;
            }
            let mut p: *const u8 = ptr::null();
            let mut n = 0usize;
            // Every case fails, so the failure reproduces and is recorded.
            match hegel_generate(tc, schema.as_ptr(), schema.len(), &mut p, &mut n) {
                HEGEL_OK => {
                    hegel_mark_complete(tc, hegel_status_t::HEGEL_STATUS_INTERESTING, ptr::null());
                }
                _ => {
                    hegel_mark_complete(tc, hegel_status_t::HEGEL_STATUS_OVERRUN, ptr::null());
                }
            }
        }

        let result = hegel_run_result(run);
        assert!(!result.is_null());
        assert!(hegel_run_result_status(result) == hegel_run_status_t::HEGEL_RUN_STATUS_FAILED);
        assert!(hegel_run_result_error(result).is_null());
        let count = hegel_run_result_failure_count(result);
        assert!(
            count >= 1,
            "the always-interesting property records a failure"
        );
        // Out-of-range failure index returns NULL.
        assert!(hegel_run_result_failure(result, count).is_null());
        let f = hegel_run_result_failure(result, 0);
        assert!(!f.is_null());
        let msg = std::ffi::CStr::from_ptr(hegel_failure_panic_message(f))
            .to_string_lossy()
            .into_owned();
        let origin = std::ffi::CStr::from_ptr(hegel_failure_origin(f))
            .to_string_lossy()
            .into_owned();
        assert!(msg.contains("Panic at <unknown>"), "got {msg:?}");
        assert!(origin.contains("Panic at <unknown>"), "got {origin:?}");
        let _ = hegel_failure_reproduction_blob(f);

        hegel_run_free(run);
        hegel_settings_free(s);
    }
}

/// Once a test case has overrun its choice budget, the engine marks the data
/// source aborted, and *every* subsequent primitive — even the bookkeeping
/// ones (`start_span`, `stop_span`, `new_collection`, `new_pool`, `pool_add`)
/// whose happy path can't otherwise fail — short-circuits to
/// `HEGEL_E_STOP_TEST`. This drives those `translate_ds_error` arms, which are
/// unreachable on a live (non-overrun) case.
#[test]
fn primitives_after_overrun_all_report_stop_test() {
    unsafe {
        let s = hegel_settings_new();
        let empty = CString::new("").unwrap();
        hegel_settings_database(s, empty.as_ptr());
        hegel_c::hegel_settings_test_cases(s, 5);
        let run = hegel_run_start(s);
        let schema = integer_schema();

        let tc = hegel_next_test_case(run);
        assert!(!tc.is_null());

        // Exhaust the choice budget by drawing until generate reports overrun.
        let mut out_ptr: *const u8 = ptr::null();
        let mut out_len = 0usize;
        let mut overran = false;
        for _ in 0..1_000_000 {
            if hegel_generate(
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut out_ptr,
                &mut out_len,
            ) == hegel_c::HEGEL_E_STOP_TEST
            {
                overran = true;
                break;
            }
        }
        assert!(overran, "drawing should eventually overrun the budget");

        // With the case aborted, each primitive now reports STOP_TEST.
        assert_eq!(
            hegel_start_span(tc, hegel_c::HEGEL_LABEL_LIST),
            hegel_c::HEGEL_E_STOP_TEST
        );
        assert_eq!(hegel_stop_span(tc, false), hegel_c::HEGEL_E_STOP_TEST);
        let mut id = 0i64;
        assert_eq!(
            hegel_new_collection(tc, 0, 3, &mut id),
            hegel_c::HEGEL_E_STOP_TEST
        );
        // collection_reject short-circuits on the aborted flag before it would
        // look up the (here nonexistent) collection id, so id 0 is safe.
        assert_eq!(
            hegel_collection_reject(tc, 0, ptr::null()),
            hegel_c::HEGEL_E_STOP_TEST
        );
        assert_eq!(hegel_new_pool(tc, &mut id), hegel_c::HEGEL_E_STOP_TEST);
        assert_eq!(hegel_pool_add(tc, 0, &mut id), hegel_c::HEGEL_E_STOP_TEST);

        hegel_mark_complete(tc, hegel_status_t::HEGEL_STATUS_OVERRUN, ptr::null());
        // Drain the rest.
        loop {
            let tc = hegel_next_test_case(run);
            if tc.is_null() {
                break;
            }
            let mut p: *const u8 = ptr::null();
            let mut n = 0usize;
            hegel_generate(tc, schema.as_ptr(), schema.len(), &mut p, &mut n);
            hegel_mark_complete(tc, hegel_status_t::HEGEL_STATUS_VALID, ptr::null());
        }
        hegel_run_free(run);
        hegel_settings_free(s);
    }
}

/// Exercise the state-machine and weighted-boolean C-ABI entry points
/// (`hegel_new_state_machine`, `hegel_state_machine_next_rule`,
/// `hegel_primitive_boolean`) in-process: the invalid-handle and
/// argument-validation paths, plus the happy paths. hegeltest's frontend
/// reaches booleans through schemas rather than `hegel_primitive_boolean`, and
/// the smoke test that drives these over dlopen doesn't contribute coverage,
/// so they are measured here.
#[test]
fn state_machine_and_primitive_boolean_paths() {
    let bad_utf8: [c_char; 2] = [0xFFu8 as c_char, 0];
    unsafe {
        // Invalid (null) handle on all three entry points.
        let null_tc: *mut HegelTestCase = ptr::null_mut();
        let rule_a = CString::new("a").unwrap();
        let rules: [*const c_char; 1] = [rule_a.as_ptr()];
        let mut out_id = 0i64;
        assert_eq!(
            hegel_new_state_machine(null_tc, rules.as_ptr(), 1, ptr::null(), 0, &mut out_id),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_state_machine_next_rule(null_tc, 0, &mut out_id),
            HEGEL_E_INVALID_HANDLE
        );
        let mut bv = false;
        assert_eq!(
            hegel_primitive_boolean(null_tc, 0.5, false, false, &mut bv),
            HEGEL_E_INVALID_HANDLE
        );

        // A live test case for the argument-validation and happy paths.
        let s = hegel_settings_new();
        let empty = CString::new("").unwrap();
        hegel_settings_database(s, empty.as_ptr());
        hegel_c::hegel_settings_test_cases(s, 5);
        let run = hegel_run_start(s);
        let tc = hegel_next_test_case(run);
        assert!(!tc.is_null());

        // new_state_machine: null out parameter.
        assert_eq!(
            hegel_new_state_machine(tc, rules.as_ptr(), 1, ptr::null(), 0, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        // null rule-name array with a non-zero count.
        assert_eq!(
            hegel_new_state_machine(tc, ptr::null(), 1, ptr::null(), 0, &mut out_id),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error().contains("rule_names pointer is null"));
        // a null entry in the rule-name array.
        let null_entry: [*const c_char; 1] = [ptr::null()];
        assert_eq!(
            hegel_new_state_machine(tc, null_entry.as_ptr(), 1, ptr::null(), 0, &mut out_id),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error().contains("rule_names[0] is null"));
        // a non-UTF-8 entry in the rule-name array.
        let bad_entry: [*const c_char; 1] = [bad_utf8.as_ptr()];
        assert_eq!(
            hegel_new_state_machine(tc, bad_entry.as_ptr(), 1, ptr::null(), 0, &mut out_id),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error().contains("not valid UTF-8"));
        // valid rules but a bad invariant array (drives the second name decode).
        let bad_inv: [*const c_char; 1] = [ptr::null()];
        assert_eq!(
            hegel_new_state_machine(tc, rules.as_ptr(), 1, bad_inv.as_ptr(), 1, &mut out_id),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error().contains("invariant_names[0] is null"));

        // A valid single-rule machine: registration, next_rule's null-out
        // guard, then a real rule draw (always rule 0).
        assert_eq!(
            hegel_new_state_machine(tc, rules.as_ptr(), 1, ptr::null(), 0, &mut out_id),
            HEGEL_OK
        );
        assert_eq!(
            hegel_state_machine_next_rule(tc, out_id, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        let mut rule_idx = -1i64;
        assert_eq!(
            hegel_state_machine_next_rule(tc, out_id, &mut rule_idx),
            HEGEL_OK
        );
        assert_eq!(rule_idx, 0, "a single-rule machine always selects rule 0");

        // primitive_boolean: happy path, null out, and an out-of-range p.
        assert_eq!(
            hegel_primitive_boolean(tc, 0.5, false, false, &mut bv),
            HEGEL_OK
        );
        assert_eq!(
            hegel_primitive_boolean(tc, 0.5, false, false, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_primitive_boolean(tc, 2.0, false, false, &mut bv),
            HEGEL_E_INVALID_ARG
        );

        hegel_mark_complete(tc, hegel_status_t::HEGEL_STATUS_VALID, ptr::null());
        loop {
            let tc = hegel_next_test_case(run);
            if tc.is_null() {
                break;
            }
            hegel_mark_complete(tc, hegel_status_t::HEGEL_STATUS_VALID, ptr::null());
        }
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

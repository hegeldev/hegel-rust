//! In-process exercise of the C ABI's error / edge paths.
//!
//! `smoke.rs` drives the C ABI the way a non-Rust caller does — by dlopening
//! the built cdylib — which is the right fidelity test but doesn't contribute
//! to coverage (the dlopened library isn't the instrumented build). These
//! tests instead call the exported `hegel_*` functions directly as ordinary
//! Rust items, so the null-handle / invalid-argument / lifecycle-misuse paths
//! they hit are measured. The happy path is covered by hegeltest driving the
//! engine over this same ABI.

mod common;

use common::{last_error, make_settings, next_case, ok, start, start_with_output};
use hegel_c::hegel_result_t::*;
use hegel_c::{
    HEGEL_STATE_MACHINE_DONE, HegelContext, HegelFailure, HegelRun, HegelRunResult, HegelTestCase,
    hegel_backend_t, hegel_collection_more, hegel_collection_reject, hegel_context_free,
    hegel_context_last_error, hegel_context_new, hegel_failure_free, hegel_failure_origin,
    hegel_failure_reproduction_blob, hegel_generate_boolean, hegel_generate_concurrency,
    hegel_generate_integer, hegel_label_t, hegel_mark_complete, hegel_mode_t, hegel_new_collection,
    hegel_new_pool, hegel_new_state_machine, hegel_next_test_case, hegel_pool_add,
    hegel_pool_generate, hegel_run_free, hegel_run_result, hegel_run_result_error,
    hegel_run_result_failure, hegel_run_result_failure_count, hegel_run_result_free,
    hegel_run_result_status, hegel_run_start, hegel_run_status_t, hegel_settings_free,
    hegel_settings_new, hegel_settings_set_backend, hegel_settings_set_database,
    hegel_settings_set_database_key, hegel_settings_set_mode, hegel_settings_set_phases,
    hegel_settings_set_report_multiple_failures, hegel_settings_set_suppress_health_check,
    hegel_start_span, hegel_state_machine_next_group, hegel_state_machine_next_rule,
    hegel_status_t, hegel_stop_span, hegel_target, hegel_test_case_clone, hegel_test_case_free,
    hegel_test_case_from_blob, hegel_version,
};
use std::ffi::{CString, c_void};
use std::os::raw::c_char;
use std::ptr;
use std::sync::Mutex;

unsafe fn result(ctx: *mut HegelContext, run: *mut HegelRun) -> *mut HegelRunResult {
    let mut r: *mut HegelRunResult = ptr::null_mut();
    assert_eq!(unsafe { hegel_run_result(ctx, run, &mut r) }, HEGEL_OK);
    assert!(!r.is_null());
    r
}

unsafe fn status_of(ctx: *mut HegelContext, r: *const HegelRunResult) -> hegel_run_status_t {
    let mut status = hegel_run_status_t::HEGEL_RUN_STATUS_PASSED;
    assert_eq!(
        unsafe { hegel_run_result_status(ctx, r, &mut status) },
        HEGEL_OK
    );
    status
}

unsafe fn failure_count_of(ctx: *mut HegelContext, r: *const HegelRunResult) -> usize {
    let mut n = 0usize;
    assert_eq!(
        unsafe { hegel_run_result_failure_count(ctx, r, &mut n) },
        HEGEL_OK
    );
    n
}

/// The `index`-th failure snapshot; `index` must be in range (an out-of-range
/// index is an `HEGEL_E_INVALID_ARG` error, asserted directly where tested).
unsafe fn failure_at(
    ctx: *mut HegelContext,
    r: *const HegelRunResult,
    index: usize,
) -> *mut HegelFailure {
    let mut f: *mut HegelFailure = ptr::null_mut();
    assert_eq!(
        unsafe { hegel_run_result_failure(ctx, r, index, &mut f) },
        HEGEL_OK
    );
    f
}

unsafe fn origin_of(ctx: *mut HegelContext, f: *const HegelFailure) -> *const c_char {
    let mut p: *const c_char = ptr::null();
    assert_eq!(unsafe { hegel_failure_origin(ctx, f, &mut p) }, HEGEL_OK);
    p
}

unsafe fn repro_blob_of(ctx: *mut HegelContext, f: *const HegelFailure) -> *const c_char {
    let mut p: *const c_char = ptr::null();
    assert_eq!(
        unsafe { hegel_failure_reproduction_blob(ctx, f, &mut p) },
        HEGEL_OK
    );
    p
}

unsafe fn run_error_of(ctx: *mut HegelContext, r: *const HegelRunResult) -> *const c_char {
    let mut p: *const c_char = ptr::null();
    assert_eq!(unsafe { hegel_run_result_error(ctx, r, &mut p) }, HEGEL_OK);
    p
}

#[test]
fn null_handles_are_rejected_without_crashing() {
    let ctx = hegel_context_new();
    unsafe {
        assert_eq!(
            hegel_settings_set_mode(
                ctx,
                ptr::null_mut(),
                hegel_mode_t::HEGEL_MODE_TEST_RUN as u32
            ),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_settings_set_backend(
                ctx,
                ptr::null_mut(),
                hegel_backend_t::HEGEL_BACKEND_AUTO as u32
            ),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_settings_set_database(ctx, ptr::null_mut(), c"x".as_ptr()),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_settings_set_database_key(ctx, ptr::null_mut(), c"x".as_ptr()),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_settings_set_phases(ctx, ptr::null_mut(), 0),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_settings_set_suppress_health_check(ctx, ptr::null_mut(), 0),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_settings_set_report_multiple_failures(ctx, ptr::null_mut(), true),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_c::hegel_settings_set_test_cases(ctx, ptr::null_mut(), 1),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_c::hegel_settings_set_verbosity(
                ctx,
                ptr::null_mut(),
                hegel_c::hegel_verbosity_t::HEGEL_VERBOSITY_NORMAL as u32
            ),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_c::hegel_settings_set_seed(ctx, ptr::null_mut(), 0, false),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_c::hegel_settings_set_derandomize(ctx, ptr::null_mut(), false),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_c::hegel_settings_set_nondeterministic(ctx, ptr::null_mut(), true),
            HEGEL_E_INVALID_HANDLE
        );

        let mut run: *mut HegelRun = ptr::null_mut();
        assert_eq!(
            hegel_run_start(ctx, ptr::null(), None, ptr::null_mut(), &mut run),
            HEGEL_E_INVALID_HANDLE
        );
        assert!(run.is_null());
        assert!(!last_error(ctx).is_empty());
        let mut tc: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(
            hegel_next_test_case(ctx, ptr::null_mut(), &mut tc),
            HEGEL_E_INVALID_HANDLE
        );
        let mut res: *mut HegelRunResult = ptr::null_mut();
        assert_eq!(
            hegel_run_result(ctx, ptr::null_mut(), &mut res),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_test_case_from_blob(
                ctx,
                ptr::null(),
                c"AAEC".as_ptr(),
                None,
                ptr::null_mut(),
                &mut tc
            ),
            HEGEL_E_INVALID_HANDLE
        );

        let mut status = hegel_run_status_t::HEGEL_RUN_STATUS_PASSED;
        assert_eq!(
            hegel_run_result_status(ctx, ptr::null(), &mut status),
            HEGEL_E_INVALID_HANDLE
        );
        let mut p: *const c_char = ptr::null();
        assert_eq!(
            hegel_run_result_error(ctx, ptr::null(), &mut p),
            HEGEL_E_INVALID_HANDLE
        );
        let mut n = 0usize;
        assert_eq!(
            hegel_run_result_failure_count(ctx, ptr::null(), &mut n),
            HEGEL_E_INVALID_HANDLE
        );
        let mut f: *mut HegelFailure = ptr::null_mut();
        assert_eq!(
            hegel_run_result_failure(ctx, ptr::null(), 0, &mut f),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_failure_origin(ctx, ptr::null(), &mut p),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_failure_reproduction_blob(ctx, ptr::null(), &mut p),
            HEGEL_E_INVALID_HANDLE
        );

        assert_eq!(hegel_settings_free(ctx, ptr::null_mut()), HEGEL_OK);
        assert_eq!(hegel_run_free(ctx, ptr::null_mut()), HEGEL_OK);
        assert_eq!(hegel_test_case_free(ctx, ptr::null_mut()), HEGEL_OK);
        assert_eq!(hegel_run_result_free(ctx, ptr::null_mut()), HEGEL_OK);
        assert_eq!(hegel_failure_free(ctx, ptr::null_mut()), HEGEL_OK);

        assert_eq!(
            hegel_test_case_clone(ctx, ptr::null(), ptr::null_mut()),
            HEGEL_E_INVALID_HANDLE
        );
        let mut clone_out: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(
            hegel_test_case_clone(ctx, ptr::null(), &mut clone_out),
            HEGEL_E_INVALID_HANDLE
        );
        assert!(clone_out.is_null());

        let tc: *mut HegelTestCase = ptr::null_mut();
        let mut value = 0i64;
        assert_eq!(
            hegel_generate_integer(ctx, tc, 0, 100, &mut value),
            HEGEL_E_INVALID_HANDLE
        );
        assert!(last_error(ctx).contains("test case pointer is null"));
        assert_eq!(hegel_start_span(ctx, tc, 1), HEGEL_E_INVALID_HANDLE);
        assert_eq!(hegel_stop_span(ctx, tc, false), HEGEL_E_INVALID_HANDLE);
        let mut id = 0i64;
        assert_eq!(
            hegel_new_collection(ctx, tc, 0, u64::MAX, &mut id),
            HEGEL_E_INVALID_HANDLE
        );
        let mut more = false;
        assert_eq!(
            hegel_collection_more(ctx, tc, 0, &mut more),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_collection_reject(ctx, tc, 0, ptr::null()),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(hegel_new_pool(ctx, tc, &mut id), HEGEL_E_INVALID_HANDLE);
        assert_eq!(hegel_pool_add(ctx, tc, 0, &mut id), HEGEL_E_INVALID_HANDLE);
        assert_eq!(
            hegel_pool_generate(ctx, tc, 0, false, &mut id),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_target(ctx, tc, 0.0, c"x".as_ptr()),
            HEGEL_E_INVALID_HANDLE
        );
        assert_ne!(
            hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null()
            ),
            HEGEL_OK
        );

        assert_eq!(
            hegel_run_start(
                ptr::null_mut(),
                ptr::null(),
                None,
                ptr::null_mut(),
                &mut run
            ),
            HEGEL_E_INVALID_HANDLE
        );
        assert!(hegel_context_last_error(ptr::null()).is_null());
    }
    unsafe {
        assert_eq!(hegel_context_free(ctx), HEGEL_OK);
    }
}

#[test]
fn out_parameters_are_rejected_when_null() {
    let ctx = hegel_context_new();
    unsafe {
        assert_eq!(
            hegel_settings_new(ctx, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        assert_eq!(
            hegel_run_start(ctx, s, None, ptr::null_mut(), ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        let run = start(ctx, s);
        assert_eq!(
            hegel_next_test_case(ctx, run, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_run_result(ctx, run, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_test_case_from_blob(
                ctx,
                s,
                c"AAEC".as_ptr(),
                None,
                ptr::null_mut(),
                ptr::null_mut()
            ),
            HEGEL_E_INVALID_ARG
        );

        assert_eq!(hegel_version(ctx, ptr::null_mut()), HEGEL_E_INVALID_ARG);

        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut value = 0i64;
            let _ = hegel_generate_integer(ctx, tc, 0, 100, &mut value);
            ok(hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        }
        let res = result(ctx, run);
        assert_eq!(
            hegel_run_result_status(ctx, res, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_run_result_error(ctx, res, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_run_result_failure_count(ctx, res, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_run_result_failure(ctx, res, 0, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        ok(hegel_run_result_free(ctx, res));

        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn settings_string_setters_handle_bad_input() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        assert_eq!(hegel_settings_set_database(ctx, s, ptr::null()), HEGEL_OK);
        assert_eq!(
            hegel_settings_set_database_key(ctx, s, ptr::null()),
            HEGEL_OK
        );

        let bad: [c_char; 2] = [0xFFu8 as c_char, 0];
        assert_eq!(
            hegel_settings_set_database(ctx, s, bad.as_ptr()),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("not valid UTF-8"));
        assert_eq!(
            hegel_settings_set_database_key(ctx, s, bad.as_ptr()),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("not valid UTF-8"));

        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn from_blob_rejects_bad_input() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let mut tc: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(
            hegel_test_case_from_blob(ctx, s, ptr::null(), None, ptr::null_mut(), &mut tc),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("null"));
        let bad: [c_char; 2] = [0xFFu8 as c_char, 0];
        assert_eq!(
            hegel_test_case_from_blob(ctx, s, bad.as_ptr(), None, ptr::null_mut(), &mut tc),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("UTF-8"));
        let garbage = CString::new("!!! not a blob !!!").unwrap();
        assert_eq!(
            hegel_test_case_from_blob(ctx, s, garbage.as_ptr(), None, ptr::null_mut(), &mut tc),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("could not be decoded"));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// Drive a short passing run with the backend pinned, exercising
/// `hegel_settings_set_backend`'s explicit arm and the run lifecycle, plus the
/// misuse paths: reading the result before the run is drained, and asking for
/// the next case before completing the current one.
#[test]
fn explicit_backend_run_and_lifecycle_misuse() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        ok(hegel_settings_set_backend(
            ctx,
            s,
            hegel_backend_t::HEGEL_BACKEND_DEFAULT as u32,
        ));
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 5));
        ok(hegel_c::hegel_settings_set_seed(ctx, s, 1, true));

        let run = start(ctx, s);

        let mut res: *mut HegelRunResult = ptr::null_mut();
        assert_eq!(hegel_run_result(ctx, run, &mut res), HEGEL_E_NOT_COMPLETE);
        assert!(res.is_null());

        let tc = next_case(ctx, run);
        assert!(!tc.is_null());

        let mut tc2: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(
            hegel_next_test_case(ctx, run, &mut tc2),
            HEGEL_E_NOT_COMPLETE
        );
        assert!(tc2.is_null());
        assert!(last_error(ctx).contains("not marked complete"));

        let mut value = 0i64;
        assert_eq!(
            hegel_generate_integer(ctx, tc, 0, 100, &mut value),
            HEGEL_OK
        );
        assert_eq!(
            hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null()
            ),
            HEGEL_OK
        );
        ok(hegel_test_case_free(ctx, tc));

        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut value = 0i64;
            assert_eq!(
                hegel_generate_integer(ctx, tc, 0, 100, &mut value),
                HEGEL_OK
            );
            ok(hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        }

        ok(hegel_run_result_free(ctx, result(ctx, run)));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// Freeing a run while a test case is still in flight (the caller bailed out
/// early) must abort and join the worker without deadlocking.
#[test]
fn run_free_with_undrained_case_does_not_deadlock() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert!(!tc.is_null());
        ok(hegel_run_free(ctx, run));
        // The run is gone, but the caller still owns its handle; freeing it now
        // (as a GC finaliser would) drops the family's last reference.
        ok(hegel_test_case_free(ctx, tc));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// Freeing the last handle to an *uncompleted* run-owned case does not
/// complete it: the run stays parked on the case, every subsequent
/// `hegel_next_test_case` reports `HEGEL_E_NOT_COMPLETE`, and the only way
/// out is `hegel_run_free` (which must still tear down cleanly). This is the
/// documented cost of `hegel_test_case_free` never touching run state — a
/// binding must report each case's outcome from its driving loop rather than
/// leaning on a finaliser.
#[test]
fn freeing_an_uncompleted_run_owned_handle_wedges_but_run_free_recovers() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert!(!tc.is_null());
        ok(hegel_test_case_free(ctx, tc));

        for _ in 0..2 {
            let mut next: *mut HegelTestCase = ptr::null_mut();
            assert_eq!(
                hegel_next_test_case(ctx, run, &mut next),
                HEGEL_E_NOT_COMPLETE
            );
            assert!(next.is_null());
            assert!(last_error(ctx).contains("not marked complete"));
        }

        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn version_is_reported() {
    let ctx = hegel_context_new();
    let mut p: *const c_char = ptr::null();
    assert_eq!(unsafe { hegel_version(ctx, &mut p) }, HEGEL_OK);
    assert!(!p.is_null());
    let v = unsafe { std::ffi::CStr::from_ptr(p) }
        .to_str()
        .unwrap()
        .to_string();
    assert!(!v.is_empty(), "version string is non-empty");
    assert!(v.chars().next().unwrap().is_ascii_digit(), "got {v:?}");
    ok(unsafe { hegel_context_free(ctx) });
}

/// Calling `hegel_next_test_case` again after the run has already drained
/// returns a NULL case with no error (idempotent end-of-run), rather than
/// blocking or faulting.
#[test]
fn next_after_drain_returns_null() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 3));
        let run = start(ctx, s);
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut value = 0i64;
            let _ = hegel_generate_integer(ctx, tc, 0, 100, &mut value);
            ok(hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        }
        assert!(next_case(ctx, run).is_null());
        assert!(last_error(ctx).is_empty());
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// Out-of-range values for the enum-typed uint32_t parameters are reported
/// as `HEGEL_E_INVALID_ARG` with a diagnostic (they would be undefined
/// behavior if the parameters were typed as Rust enums).
#[test]
fn out_of_range_enum_values_are_invalid_arguments() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        assert_eq!(hegel_settings_set_mode(ctx, s, 999), HEGEL_E_INVALID_ARG);
        assert!(last_error(ctx).contains("unknown mode"));
        assert_eq!(hegel_settings_set_backend(ctx, s, 999), HEGEL_E_INVALID_ARG);
        assert!(last_error(ctx).contains("unknown backend"));
        assert_eq!(
            hegel_c::hegel_settings_set_verbosity(ctx, s, 999),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("unknown verbosity"));

        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 3));
        let run = start(ctx, s);
        let mut checked_status = false;
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            if !checked_status {
                assert_eq!(
                    hegel_mark_complete(ctx, tc, 999, ptr::null()),
                    HEGEL_E_INVALID_ARG
                );
                assert!(last_error(ctx).contains("unknown status"));
                checked_status = true;
            }
            ok(hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        }
        assert!(checked_status);
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// Exercise the per-primitive argument-validation paths on a *live*,
/// run-owned test case: null out-parameters, inverted bounds, non-UTF-8
/// string arguments, completing twice, drawing after completion, and releasing
/// a run-owned handle with `hegel_test_case_free` (the caller owns its handle
/// even though the run keeps its own reference). The case is marked
/// INTERESTING with a NULL origin so the run surfaces a failure whose
/// panic message is the synthesized "Panic at <unknown>" placeholder, which
/// we then read back through the result getters.
#[test]
fn live_test_case_argument_validation() {
    let bad_utf8: [c_char; 2] = [0xFFu8 as c_char, 0];
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 5));
        ok(hegel_c::hegel_settings_set_seed(ctx, s, 1, true));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert!(!tc.is_null());

        let mut value = 0i64;

        assert_eq!(
            hegel_generate_integer(ctx, tc, 0, 100, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("out parameter is null"));
        assert_eq!(
            hegel_generate_integer(ctx, tc, 100, 0, &mut value),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("min_value"));

        let mut id = 0i64;
        assert_eq!(
            hegel_new_collection(ctx, tc, 0, u64::MAX, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_new_collection(ctx, tc, 5, 3, &mut id),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("min_size <= max_size"));
        assert!(last_error(ctx).contains("[5, 3]"));
        assert_eq!(
            hegel_new_pool(ctx, tc, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );

        assert_eq!(hegel_new_collection(ctx, tc, 0, 3, &mut id), HEGEL_OK);
        assert_eq!(
            hegel_collection_more(ctx, tc, id, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_collection_reject(ctx, tc, id, bad_utf8.as_ptr()),
            HEGEL_E_INVALID_ARG
        );
        let mut more = false;
        if hegel_collection_more(ctx, tc, id, &mut more) == HEGEL_OK && more {
            let _ = hegel_generate_integer(ctx, tc, 0, 100, &mut value);
            assert_eq!(hegel_collection_reject(ctx, tc, id, ptr::null()), HEGEL_OK);
        }

        let mut pool = 0i64;
        assert_eq!(hegel_new_pool(ctx, tc, &mut pool), HEGEL_OK);
        assert_eq!(
            hegel_pool_add(ctx, tc, pool, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_pool_generate(ctx, tc, pool, false, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );

        assert_eq!(hegel_target(ctx, tc, 0.0, ptr::null()), HEGEL_E_INVALID_ARG);
        assert!(last_error(ctx).contains("label is null"));
        assert_eq!(
            hegel_target(ctx, tc, 0.0, bad_utf8.as_ptr()),
            HEGEL_E_INVALID_ARG
        );

        assert_eq!(
            hegel_target(ctx, tc, f64::NAN, c"x".as_ptr()),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("finite score"));
        assert_eq!(hegel_target(ctx, tc, 1.0, c"dup".as_ptr()), HEGEL_OK);
        assert_eq!(
            hegel_target(ctx, tc, 2.0, c"dup".as_ptr()),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("would overwrite previous"));

        assert_eq!(
            hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_INTERESTING as u32,
                bad_utf8.as_ptr()
            ),
            HEGEL_E_INVALID_ARG
        );

        assert_eq!(
            hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null()
            ),
            HEGEL_OK
        );

        assert_eq!(
            hegel_generate_integer(ctx, tc, 0, 100, &mut value),
            HEGEL_E_ALREADY_COMPLETE
        );
        assert!(last_error(ctx).contains("already complete"));
        assert_eq!(
            hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null()
            ),
            HEGEL_E_ALREADY_COMPLETE
        );
        assert_eq!(hegel_test_case_free(ctx, tc), HEGEL_OK);

        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut value = 0i64;
            let _ = hegel_generate_integer(ctx, tc, 0, 100, &mut value);
            ok(hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        }

        ok(hegel_run_result_free(ctx, result(ctx, run)));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// A property that always reports INTERESTING with a NULL origin: the engine
/// synthesizes the "Panic at <unknown>" placeholder for the failure's message
/// and origin. Drives the FAILED run-result path and the failure getters'
/// present-value arms at the C level, and reaches the out-of-range failure
/// index branch.
#[test]
fn interesting_with_null_origin_synthesizes_placeholder() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 5));
        ok(hegel_c::hegel_settings_set_seed(ctx, s, 1, true));
        let run = start(ctx, s);
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut value = 0i64;
            match hegel_generate_integer(ctx, tc, 0, 100, &mut value) {
                HEGEL_OK => {
                    ok(hegel_mark_complete(
                        ctx,
                        tc,
                        hegel_status_t::HEGEL_STATUS_INTERESTING as u32,
                        ptr::null(),
                    ));
                }
                _ => {
                    ok(hegel_mark_complete(
                        ctx,
                        tc,
                        hegel_status_t::HEGEL_STATUS_OVERRUN as u32,
                        ptr::null(),
                    ));
                }
            }
            ok(hegel_test_case_free(ctx, tc));
        }

        let res = result(ctx, run);
        assert!(status_of(ctx, res) == hegel_run_status_t::HEGEL_RUN_STATUS_FAILED);
        assert!(run_error_of(ctx, res).is_null());
        let count = failure_count_of(ctx, res);
        assert!(
            count >= 1,
            "the always-interesting property records a failure"
        );
        let mut past_end: *mut HegelFailure = ptr::null_mut();
        assert_eq!(
            hegel_run_result_failure(ctx, res, count, &mut past_end),
            HEGEL_E_INVALID_ARG
        );
        assert!(past_end.is_null());
        assert!(last_error(ctx).contains("out of range"));
        let f = failure_at(ctx, res, 0);
        assert!(!f.is_null());
        assert_eq!(
            hegel_failure_origin(ctx, f, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_failure_reproduction_blob(ctx, f, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        let origin = std::ffi::CStr::from_ptr(origin_of(ctx, f))
            .to_string_lossy()
            .into_owned();
        assert!(origin.contains("Panic at <unknown>"), "got {origin:?}");
        let _ = repro_blob_of(ctx, f);
        ok(hegel_failure_free(ctx, f));
        ok(hegel_run_result_free(ctx, res));

        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// A `Mode::SingleTestCase` run that fails surfaces a failure with an origin
/// but no reproduce blob (there is no shrunk choice sequence to encode). This
/// drives the engine's single-case path at the C level and the
/// `hegel_failure_reproduction_blob` arm that returns NULL for a blobless
/// failure.
#[test]
fn single_test_case_failure_has_origin_but_no_blob() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_settings_set_mode(
            ctx,
            s,
            hegel_mode_t::HEGEL_MODE_SINGLE_TEST_CASE as u32,
        ));
        let run = start(ctx, s);
        let origin = CString::new("single-case bug").unwrap();

        let tc = next_case(ctx, run);
        assert!(!tc.is_null());
        let mut value = 0i64;
        assert_eq!(
            hegel_generate_integer(ctx, tc, 0, 100, &mut value),
            HEGEL_OK
        );
        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_INTERESTING as u32,
            origin.as_ptr(),
        ));
        ok(hegel_test_case_free(ctx, tc));
        assert!(next_case(ctx, run).is_null());

        let res = result(ctx, run);
        assert!(status_of(ctx, res) == hegel_run_status_t::HEGEL_RUN_STATUS_FAILED);
        let f = failure_at(ctx, res, 0);
        assert!(!f.is_null());
        let origin_back = std::ffi::CStr::from_ptr(origin_of(ctx, f))
            .to_string_lossy()
            .into_owned();
        assert!(
            origin_back.contains("single-case bug"),
            "got {origin_back:?}"
        );
        assert!(repro_blob_of(ctx, f).is_null());
        ok(hegel_failure_free(ctx, f));
        ok(hegel_run_result_free(ctx, res));

        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// A full run declared nondeterministic via
/// `hegel_settings_set_nondeterministic` stops at the first bug and surfaces
/// it with an origin but no reproduce blob: with replay and shrinking off,
/// there is no shrunk choice sequence to encode.
#[test]
fn nondeterministic_run_failure_has_origin_but_no_blob() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_nondeterministic(ctx, s, true));
        let run = start(ctx, s);
        let origin = CString::new("nondeterministic bug").unwrap();

        let mut cases = 0usize;
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            cases += 1;
            let mut value = 0i64;
            assert_eq!(
                hegel_generate_integer(ctx, tc, 0, 100, &mut value),
                HEGEL_OK
            );
            ok(hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_INTERESTING as u32,
                origin.as_ptr(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        }
        assert_eq!(cases, 1, "a nondeterministic run stops at the first bug");

        let res = result(ctx, run);
        assert!(status_of(ctx, res) == hegel_run_status_t::HEGEL_RUN_STATUS_FAILED);
        assert_eq!(failure_count_of(ctx, res), 1);
        let f = failure_at(ctx, res, 0);
        assert!(!f.is_null());
        let origin_back = std::ffi::CStr::from_ptr(origin_of(ctx, f))
            .to_string_lossy()
            .into_owned();
        assert!(
            origin_back.contains("nondeterministic bug"),
            "got {origin_back:?}"
        );
        assert!(repro_blob_of(ctx, f).is_null());
        ok(hegel_failure_free(ctx, f));
        ok(hegel_run_result_free(ctx, res));

        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
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
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 5));
        let run = start(ctx, s);

        let tc = next_case(ctx, run);
        assert!(!tc.is_null());

        let mut value = 0i64;
        let mut overran = false;
        for _ in 0..1_000_000 {
            if hegel_generate_integer(ctx, tc, 0, 100, &mut value) == HEGEL_E_STOP_TEST {
                overran = true;
                break;
            }
        }
        assert!(overran, "drawing should eventually overrun the budget");

        assert_eq!(
            hegel_start_span(ctx, tc, hegel_label_t::HEGEL_LABEL_LIST as u64),
            HEGEL_E_STOP_TEST
        );
        assert_eq!(hegel_stop_span(ctx, tc, false), HEGEL_E_STOP_TEST);
        let mut id = 0i64;
        assert_eq!(
            hegel_new_collection(ctx, tc, 0, 3, &mut id),
            HEGEL_E_STOP_TEST
        );
        assert_eq!(
            hegel_collection_reject(ctx, tc, 0, ptr::null()),
            HEGEL_E_STOP_TEST
        );
        assert_eq!(hegel_new_pool(ctx, tc, &mut id), HEGEL_E_STOP_TEST);
        assert_eq!(hegel_pool_add(ctx, tc, 0, &mut id), HEGEL_E_STOP_TEST);

        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_OVERRUN as u32,
            ptr::null(),
        ));
        ok(hegel_test_case_free(ctx, tc));
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut value = 0i64;
            let _ = hegel_generate_integer(ctx, tc, 0, 100, &mut value);
            ok(hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        }
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// Exercise the state-machine, concurrency-draw, and weighted-boolean C-ABI
/// entry points (`hegel_new_state_machine`,
/// `hegel_state_machine_next_group`, `hegel_state_machine_next_rule`,
/// `hegel_generate_concurrency`, `hegel_generate_boolean`) in-process: the
/// invalid-handle and argument-validation paths, plus the happy paths. The
/// smoke test that drives these over dlopen doesn't contribute coverage, so
/// they are measured here.
#[test]
fn state_machine_and_primitive_boolean_paths() {
    let bad_utf8: [c_char; 2] = [0xFFu8 as c_char, 0];
    let ctx = hegel_context_new();
    unsafe {
        let null_tc: *mut HegelTestCase = ptr::null_mut();
        let rule_a = CString::new("a").unwrap();
        let rules: [*const c_char; 1] = [rule_a.as_ptr()];
        let rule_groups: [i64; 1] = [0];
        let mut out_id = 0i64;
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                null_tc,
                1,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                &mut out_id,
            ),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_state_machine_next_rule(ctx, null_tc, 0, 0, &mut out_id),
            HEGEL_E_INVALID_HANDLE
        );
        let mut group_idx = 0i64;
        assert_eq!(
            hegel_state_machine_next_group(ctx, null_tc, 0, &mut group_idx),
            HEGEL_E_INVALID_HANDLE
        );
        let mut level = 0i64;
        assert_eq!(
            hegel_generate_concurrency(ctx, null_tc, 3, &mut level),
            HEGEL_E_INVALID_HANDLE
        );
        let mut bv = false;
        assert_eq!(
            hegel_generate_boolean(ctx, null_tc, 0.5, false, false, &mut bv),
            HEGEL_E_INVALID_HANDLE
        );

        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 5));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert!(!tc.is_null());

        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                1,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                ptr::null_mut(),
            ),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                1,
                ptr::null(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                &mut out_id,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("rule_names pointer is null"));
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                1,
                rules.as_ptr(),
                ptr::null(),
                1,
                ptr::null(),
                0,
                1,
                &mut out_id,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("rule_groups is null"));
        let null_entry: [*const c_char; 1] = [ptr::null()];
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                1,
                null_entry.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                &mut out_id,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("rule_names[0] is null"));
        let bad_entry: [*const c_char; 1] = [bad_utf8.as_ptr()];
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                1,
                bad_entry.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                &mut out_id,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("not valid UTF-8"));
        let bad_inv: [*const c_char; 1] = [ptr::null()];
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                1,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                bad_inv.as_ptr(),
                1,
                1,
                &mut out_id,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("invariant_names[0] is null"));
        let oor_groups: [i64; 1] = [1];
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                1,
                rules.as_ptr(),
                oor_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                &mut out_id,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("rule_groups[0] must be in [0, 1)"));
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                1,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                0,
                &mut out_id,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("concurrency must be at least 1"));

        assert_eq!(
            hegel_generate_concurrency(ctx, tc, 3, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_generate_concurrency(ctx, tc, 0, &mut level),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("max_value >= 1"));
        assert_eq!(hegel_generate_concurrency(ctx, tc, 1, &mut level), HEGEL_OK);
        assert_eq!(level, 1);
        assert_eq!(hegel_generate_concurrency(ctx, tc, 3, &mut level), HEGEL_OK);
        assert!((1..=3).contains(&level));

        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                1,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                &mut out_id,
            ),
            HEGEL_OK
        );
        assert_eq!(
            hegel_state_machine_next_rule(ctx, tc, out_id, 0, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_state_machine_next_group(ctx, tc, out_id, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        let mut rule_idx = -1i64;
        assert_eq!(
            hegel_state_machine_next_rule(ctx, tc, out_id, 0, &mut rule_idx),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("before the first next_group"));
        let mut rounds = 0;
        loop {
            assert_eq!(
                hegel_state_machine_next_group(ctx, tc, out_id, &mut group_idx),
                HEGEL_OK
            );
            if group_idx == HEGEL_STATE_MACHINE_DONE {
                break;
            }
            assert_eq!(group_idx, 0, "a single-group machine is always in group 0");
            rounds += 1;
            assert!(rounds <= 50, "the engine's round cap never exceeds 50");
            assert_eq!(
                hegel_state_machine_next_rule(ctx, tc, out_id, 1, &mut rule_idx),
                HEGEL_E_INVALID_ARG
            );
            assert!(last_error(ctx).contains("worker_index must be in [0, 1)"));
            assert_eq!(
                hegel_state_machine_next_rule(ctx, tc, out_id, 0, &mut rule_idx),
                HEGEL_OK
            );
            assert_eq!(rule_idx, 0, "a single-rule machine always selects rule 0");
            assert_eq!(
                hegel_state_machine_next_rule(ctx, tc, out_id, 0, &mut rule_idx),
                HEGEL_OK
            );
            assert_eq!(
                rule_idx, HEGEL_STATE_MACHINE_DONE,
                "a sequential machine hands out one rule per round"
            );
        }
        assert!(rounds >= 1);

        assert_eq!(
            hegel_generate_boolean(ctx, tc, 0.5, false, false, &mut bv),
            HEGEL_OK
        );
        assert_eq!(
            hegel_generate_boolean(ctx, tc, 0.5, false, false, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_generate_boolean(ctx, tc, 2.0, false, false, &mut bv),
            HEGEL_E_INVALID_ARG
        );

        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_VALID as u32,
            ptr::null(),
        ));
        ok(hegel_test_case_free(ctx, tc));
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            ok(hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        }
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// Run a small always-interesting property to completion and return an owned
/// copy of its single shrunk failure's base64 reproduce blob. The property
/// draws `draws` integers per test case (all must succeed for the case to be
/// interesting), so the returned blob replays a choice sequence of exactly
/// `draws` values.
unsafe fn shrunk_failure_blob_with_draws(ctx: *mut HegelContext, draws: usize) -> CString {
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 5));
        ok(hegel_c::hegel_settings_set_seed(ctx, s, 1, true));
        let run = start(ctx, s);
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut value = 0i64;
            let mut status = hegel_status_t::HEGEL_STATUS_INTERESTING as u32;
            for _ in 0..draws {
                if hegel_generate_integer(ctx, tc, 0, 100, &mut value) != HEGEL_OK {
                    status = hegel_status_t::HEGEL_STATUS_OVERRUN as u32;
                    break;
                }
            }
            ok(hegel_mark_complete(ctx, tc, status, ptr::null()));
            ok(hegel_test_case_free(ctx, tc));
        }
        let res = result(ctx, run);
        let f = failure_at(ctx, res, 0);
        assert!(!f.is_null());
        let blob_ptr = repro_blob_of(ctx, f);
        assert!(!blob_ptr.is_null(), "a shrunk failure carries a blob");
        let blob = std::ffi::CStr::from_ptr(blob_ptr).to_owned();
        ok(hegel_failure_free(ctx, f));
        ok(hegel_run_result_free(ctx, res));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        blob
    }
}

/// Result and failure snapshots are independent of the run: here they are
/// read, the run (and settings) are freed, and only then are the status,
/// count, origin, and blob inspected — the snapshots and the strings read off
/// them stay valid until their own frees. This is what lets a GC binding free
/// each wrapper from its finaliser in any order.
#[test]
fn result_and_failure_snapshots_outlive_the_run() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 5));
        ok(hegel_c::hegel_settings_set_seed(ctx, s, 1, true));
        let run = start(ctx, s);
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut value = 0i64;
            let status = if hegel_generate_integer(ctx, tc, 0, 100, &mut value) == HEGEL_OK {
                hegel_status_t::HEGEL_STATUS_INTERESTING as u32
            } else {
                hegel_status_t::HEGEL_STATUS_OVERRUN as u32
            };
            ok(hegel_mark_complete(ctx, tc, status, ptr::null()));
            ok(hegel_test_case_free(ctx, tc));
        }

        let res = result(ctx, run);
        let f = failure_at(ctx, res, 0);
        assert!(!f.is_null());
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));

        assert!(status_of(ctx, res) == hegel_run_status_t::HEGEL_RUN_STATUS_FAILED);
        assert!(failure_count_of(ctx, res) >= 1);
        let origin = std::ffi::CStr::from_ptr(origin_of(ctx, f))
            .to_string_lossy()
            .into_owned();
        assert!(!origin.is_empty());
        assert!(!repro_blob_of(ctx, f).is_null());

        ok(hegel_failure_free(ctx, f));
        ok(hegel_run_result_free(ctx, res));
        ok(hegel_context_free(ctx));
    }
}

/// A clone shares the underlying test case with its root: it draws from the
/// same source, and completion is first-caller-wins and family-wide. The first
/// `hegel_mark_complete` anywhere in the family records the outcome; completing
/// a *different* handle afterward is a safe no-op (so racing clones don't
/// error), while completing the *same* handle twice is a usage error. A clone
/// can be made after completion (and is immediately complete). Every handle —
/// root or clone, run-owned or not — is released independently with
/// `hegel_test_case_free`.
#[test]
fn clones_share_a_run_owned_family() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 5));
        ok(hegel_c::hegel_settings_set_seed(ctx, s, 1, true));
        let run = start(ctx, s);
        let root = next_case(ctx, run);
        assert!(!root.is_null());

        assert_eq!(
            hegel_test_case_clone(ctx, root, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        let mut c1: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(hegel_test_case_clone(ctx, root, &mut c1), HEGEL_OK);
        assert!(!c1.is_null());

        let mut value = 0i64;
        assert_eq!(
            hegel_generate_integer(ctx, root, 0, 100, &mut value),
            HEGEL_OK
        );
        assert_eq!(
            hegel_generate_integer(ctx, c1, 0, 100, &mut value),
            HEGEL_OK
        );

        let mut c1a: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(hegel_test_case_clone(ctx, c1, &mut c1a), HEGEL_OK);
        assert_eq!(
            hegel_generate_integer(ctx, c1a, 0, 100, &mut value),
            HEGEL_OK
        );

        // The first handle to complete the family wins and records the outcome.
        assert_eq!(
            hegel_mark_complete(
                ctx,
                c1,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null()
            ),
            HEGEL_OK
        );
        // Completing a *different* handle in the now-complete family is a safe
        // no-op (first-caller-wins), so racing clones don't error.
        assert_eq!(
            hegel_mark_complete(
                ctx,
                root,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null()
            ),
            HEGEL_OK
        );
        // But completing the *same* handle twice is a usage error.
        assert_eq!(
            hegel_mark_complete(
                ctx,
                c1,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null()
            ),
            HEGEL_E_ALREADY_COMPLETE
        );
        // Primitives on any family handle still report the family complete.
        assert_eq!(
            hegel_generate_integer(ctx, root, 0, 100, &mut value),
            HEGEL_E_ALREADY_COMPLETE
        );

        let mut c2: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(
            hegel_test_case_clone(ctx, root, &mut c2),
            HEGEL_E_ALREADY_COMPLETE
        );
        assert!(c2.is_null());

        assert_eq!(hegel_test_case_free(ctx, c1), HEGEL_OK);
        assert_eq!(hegel_test_case_free(ctx, c1a), HEGEL_OK);
        assert_eq!(hegel_test_case_free(ctx, root), HEGEL_OK);

        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            ok(hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        }
        ok(hegel_run_result_free(ctx, result(ctx, run)));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// Every handle in a standalone (`from_blob`) family — the root and two
/// clones of it — is freed independently, in any order. The underlying
/// test case stays alive until its last handle is freed: a clone keeps
/// working after its sibling (and even the root) has been freed. Run
/// under Miri this proves there is no leak, double-free, or use-after-free
/// across the drop orders.
#[test]
fn standalone_handles_are_freed_independently() {
    let ctx = hegel_context_new();
    unsafe {
        let blob = shrunk_failure_blob_with_draws(ctx, 2);
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));

        let mut root: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(
            hegel_test_case_from_blob(ctx, s, blob.as_ptr(), None, ptr::null_mut(), &mut root),
            HEGEL_OK
        );
        assert!(!root.is_null());

        let mut c1: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(hegel_test_case_clone(ctx, root, &mut c1), HEGEL_OK);
        let mut c2: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(hegel_test_case_clone(ctx, root, &mut c2), HEGEL_OK);

        // A non-consuming span op proves a handle is live and reaches its
        // stream; the blob's finite choice sequence means we can't keep
        // drawing, so we don't draw here.
        let alive = |tc: *mut HegelTestCase| {
            assert_eq!(hegel_start_span(ctx, tc, 1), HEGEL_OK);
            assert_eq!(hegel_stop_span(ctx, tc, false), HEGEL_OK);
        };

        // Freeing a clone drops only its own reference; the root and the other
        // clone stay live.
        assert_eq!(hegel_test_case_free(ctx, c1), HEGEL_OK);
        alive(root);
        alive(c2);

        // Freeing the root no longer frees its clones: c2 keeps its reference
        // (and the data source) alive and is still usable.
        assert_eq!(hegel_test_case_free(ctx, root), HEGEL_OK);
        alive(c2);

        // The last handle releases the data source.
        assert_eq!(hegel_test_case_free(ctx, c2), HEGEL_OK);

        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// Two clones drive the same test case from two threads at once. Because each
/// handle has its own lock and its own independent stream, neither draw is
/// rejected with `HEGEL_E_CONCURRENT_USE` — that is reserved for using a
/// *single* handle from two threads.
#[test]
fn two_clones_draw_concurrently_without_concurrent_use_errors() {
    use std::sync::{Arc, Barrier};

    struct SendPtr(*mut HegelTestCase);
    // SAFETY: each clone is a distinct handle with its own lock; the threads
    // are joined before the handles are freed.
    unsafe impl Send for SendPtr {}

    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 5));
        ok(hegel_c::hegel_settings_set_seed(ctx, s, 1, true));
        let run = start(ctx, s);
        let root = next_case(ctx, run);
        assert!(!root.is_null());

        let mut c1: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(hegel_test_case_clone(ctx, root, &mut c1), HEGEL_OK);
        let mut c2: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(hegel_test_case_clone(ctx, root, &mut c2), HEGEL_OK);

        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = [SendPtr(c1), SendPtr(c2)]
            .into_iter()
            .map(|cp| {
                let b = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    // Capture the whole `SendPtr` (disjoint closure capture
                    // would otherwise grab the non-`Send` raw pointer field).
                    let cp = cp;
                    let tctx = hegel_context_new();
                    let mut value = 0i64;
                    b.wait();
                    let rc = hegel_generate_integer(tctx, cp.0, 0, 100, &mut value);
                    ok(hegel_context_free(tctx));
                    rc
                })
            })
            .collect();
        for h in handles {
            let rc = h.join().unwrap();
            assert_ne!(
                rc, HEGEL_E_CONCURRENT_USE,
                "two distinct clones must not block each other"
            );
        }

        ok(hegel_mark_complete(
            ctx,
            root,
            hegel_status_t::HEGEL_STATUS_VALID as u32,
            ptr::null(),
        ));
        ok(hegel_test_case_free(ctx, c1));
        ok(hegel_test_case_free(ctx, c2));
        ok(hegel_test_case_free(ctx, root));
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            ok(hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        }
        ok(hegel_run_result_free(ctx, result(ctx, run)));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// Output callback for the `hegel_run_start` / `hegel_test_case_from_blob`
/// output tests: `user_data` points at a `Mutex<Vec<String>>` that collects
/// every line, checking on the way that `line` is NUL-terminated UTF-8 whose
/// length matches `len`.
unsafe extern "C" fn capture_output(user_data: *mut c_void, line: *const c_char, len: usize) {
    let lines = unsafe { &*user_data.cast::<Mutex<Vec<String>>>() };
    let text = unsafe { std::ffi::CStr::from_ptr(line) }
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(text.len(), len);
    lines.lock().unwrap().push(text);
}

/// A debug-verbosity failing run started with an output callback delivers the
/// engine's progress lines (phase edges, per-case traces, shrink progress,
/// the final summary) to the callback, passing the caller's `user_data`
/// through.
#[test]
fn output_callback_receives_engine_output() {
    let lines: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 5));
        ok(hegel_c::hegel_settings_set_seed(ctx, s, 1, true));
        ok(hegel_c::hegel_settings_set_verbosity(
            ctx,
            s,
            hegel_c::hegel_verbosity_t::HEGEL_VERBOSITY_DEBUG as u32,
        ));
        let run = start_with_output(
            ctx,
            s,
            Some(capture_output),
            (&raw const lines).cast_mut().cast(),
        );
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut value = 0i64;
            let status = if hegel_generate_integer(ctx, tc, 0, 100, &mut value) == HEGEL_OK {
                hegel_status_t::HEGEL_STATUS_INTERESTING as u32
            } else {
                hegel_status_t::HEGEL_STATUS_OVERRUN as u32
            };
            ok(hegel_mark_complete(ctx, tc, status, ptr::null()));
            ok(hegel_test_case_free(ctx, tc));
        }
        ok(hegel_run_result_free(ctx, result(ctx, run)));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
    let lines = lines.into_inner().unwrap();
    let all = lines.join("\n");
    assert!(
        lines.iter().any(|l| l == "Starting phase: Generate"),
        "got {all:?}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("test case #")),
        "got {all:?}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("Shrinking:")),
        "got {all:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l == "Test done. interesting_test_cases=1"),
        "got {all:?}"
    );
}

/// A run started with a NULL callback writes its output to stderr and
/// delivers nothing to any callback — exercising the stderr default of
/// `hegel_run_start`.
#[test]
fn null_output_callback_writes_to_stderr() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 2));
        ok(hegel_c::hegel_settings_set_seed(ctx, s, 1, true));
        ok(hegel_c::hegel_settings_set_verbosity(
            ctx,
            s,
            hegel_c::hegel_verbosity_t::HEGEL_VERBOSITY_DEBUG as u32,
        ));
        let run = start_with_output(ctx, s, None, ptr::null_mut());
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut value = 0i64;
            let status = if hegel_generate_integer(ctx, tc, 0, 100, &mut value) == HEGEL_OK {
                hegel_status_t::HEGEL_STATUS_VALID as u32
            } else {
                hegel_status_t::HEGEL_STATUS_OVERRUN as u32
            };
            ok(hegel_mark_complete(ctx, tc, status, ptr::null()));
            ok(hegel_test_case_free(ctx, tc));
        }
        ok(hegel_run_result_free(ctx, result(ctx, run)));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// `hegel_test_case_from_blob` routes its output to the supplied callback:
/// at debug verbosity the blob-replay trace line is delivered to the
/// callback instead of stderr.
#[test]
fn from_blob_replay_trace_goes_to_the_output_callback() {
    let lines: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let ctx = hegel_context_new();
    unsafe {
        let blob = shrunk_failure_blob_with_draws(ctx, 2);
        let s = make_settings(ctx);
        ok(hegel_c::hegel_settings_set_verbosity(
            ctx,
            s,
            hegel_c::hegel_verbosity_t::HEGEL_VERBOSITY_DEBUG as u32,
        ));
        let mut tc: *mut HegelTestCase = ptr::null_mut();
        ok(hegel_test_case_from_blob(
            ctx,
            s,
            blob.as_ptr(),
            Some(capture_output),
            (&raw const lines).cast_mut().cast(),
            &mut tc,
        ));
        assert!(!tc.is_null());
        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_VALID as u32,
            ptr::null(),
        ));
        ok(hegel_test_case_free(ctx, tc));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
    let lines = lines.into_inner().unwrap();
    assert_eq!(lines, ["replaying failure blob: choices = 2"]);
}

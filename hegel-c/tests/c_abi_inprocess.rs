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
    HEGEL_STATE_MACHINE_DONE, HegelCollection, HegelContext, HegelFailure, HegelPool,
    HegelRecursion, HegelRun, HegelRunResult, HegelStateMachine, HegelTestCase, hegel_backend_t,
    hegel_collection_free, hegel_collection_more, hegel_collection_reject, hegel_context_free,
    hegel_context_last_error, hegel_context_new, hegel_failure_free, hegel_failure_origin,
    hegel_failure_reproduction_blob, hegel_generate_boolean, hegel_generate_integer, hegel_label_t,
    hegel_mark_complete, hegel_new_collection, hegel_new_pool, hegel_new_recursion,
    hegel_new_state_machine, hegel_next_test_case, hegel_pool_add, hegel_pool_free,
    hegel_pool_generate, hegel_recursion_branch, hegel_recursion_finish, hegel_recursion_free,
    hegel_recursion_leaf, hegel_recursion_retry, hegel_run_free, hegel_run_result,
    hegel_run_result_error, hegel_run_result_failure, hegel_run_result_failure_count,
    hegel_run_result_free, hegel_run_result_status, hegel_run_start, hegel_run_status_t,
    hegel_settings_free, hegel_settings_new, hegel_settings_set_backend,
    hegel_settings_set_database, hegel_settings_set_database_key, hegel_settings_set_phases,
    hegel_settings_set_report_multiple_failures, hegel_settings_set_suppress_health_check,
    hegel_start_span, hegel_state_machine_free, hegel_state_machine_next_group,
    hegel_state_machine_next_rule, hegel_state_machine_rule_rejected,
    hegel_state_machine_should_check_invariant, hegel_status_t, hegel_stop_span, hegel_target,
    hegel_test_case_clone, hegel_test_case_free, hegel_test_case_from_blob,
    hegel_test_case_is_nondeterministic, hegel_version,
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
            hegel_c::hegel_settings_set_stateful_step_count(ctx, ptr::null_mut(), 1),
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
        let mut is_nondeterministic = false;
        assert_eq!(
            hegel_test_case_is_nondeterministic(ctx, ptr::null(), &mut is_nondeterministic),
            HEGEL_E_INVALID_HANDLE
        );

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
        let mut collection: *mut HegelCollection = ptr::null_mut();
        assert_eq!(
            hegel_new_collection(ctx, tc, 0, u64::MAX, &mut collection),
            HEGEL_E_INVALID_HANDLE
        );
        let mut more = false;
        assert_eq!(
            hegel_collection_more(ctx, tc, ptr::null_mut(), &mut more),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_collection_reject(ctx, tc, ptr::null_mut(), ptr::null()),
            HEGEL_E_INVALID_HANDLE
        );
        let mut pool: *mut HegelPool = ptr::null_mut();
        assert_eq!(hegel_new_pool(ctx, tc, &mut pool), HEGEL_E_INVALID_HANDLE);
        assert_eq!(
            hegel_pool_add(ctx, tc, ptr::null_mut(), &mut id),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_pool_generate(ctx, tc, ptr::null_mut(), false, &mut id),
            HEGEL_E_INVALID_HANDLE
        );
        let mut recursion: *mut HegelRecursion = ptr::null_mut();
        assert_eq!(
            hegel_new_recursion(ctx, tc, 4, 100, &mut recursion),
            HEGEL_E_INVALID_HANDLE
        );
        assert!(recursion.is_null());
        let mut branch = false;
        assert_eq!(
            hegel_recursion_branch(ctx, tc, ptr::null_mut(), 0, &mut branch),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_recursion_leaf(ctx, tc, ptr::null_mut()),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_recursion_retry(ctx, tc, ptr::null_mut()),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_recursion_finish(ctx, tc, ptr::null_mut()),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(hegel_recursion_free(ctx, ptr::null_mut()), HEGEL_OK);
        assert_eq!(hegel_collection_free(ctx, ptr::null_mut()), HEGEL_OK);
        assert_eq!(hegel_pool_free(ctx, ptr::null_mut()), HEGEL_OK);
        assert_eq!(hegel_state_machine_free(ctx, ptr::null_mut()), HEGEL_OK);
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

        let mut is_nondeterministic = true;
        ok(hegel_test_case_is_nondeterministic(
            ctx,
            tc,
            &mut is_nondeterministic,
        ));
        assert!(!is_nondeterministic);
        assert_eq!(
            hegel_test_case_is_nondeterministic(ctx, tc, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );

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
        assert_eq!(hegel_settings_set_backend(ctx, s, 999), HEGEL_E_INVALID_ARG);
        assert!(last_error(ctx).contains("unknown backend"));
        assert_eq!(
            hegel_c::hegel_settings_set_verbosity(ctx, s, 999),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("unknown verbosity"));
        assert_eq!(
            hegel_c::hegel_settings_set_stateful_step_count(ctx, s, 0),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("step count must be at least 1"));
        assert_eq!(
            hegel_c::hegel_settings_set_stateful_step_count(ctx, s, -3),
            HEGEL_E_INVALID_ARG
        );

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

        let mut collection: *mut HegelCollection = ptr::null_mut();
        assert_eq!(
            hegel_new_collection(ctx, tc, 0, u64::MAX, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_new_collection(ctx, tc, 5, 3, &mut collection),
            HEGEL_E_INVALID_ARG
        );
        assert!(collection.is_null());
        assert!(last_error(ctx).contains("min_size <= max_size"));
        assert!(last_error(ctx).contains("[5, 3]"));
        assert_eq!(
            hegel_new_pool(ctx, tc, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );

        let mut null_more = false;
        assert_eq!(
            hegel_collection_more(ctx, tc, ptr::null_mut(), &mut null_more),
            HEGEL_E_INVALID_HANDLE
        );
        assert!(last_error(ctx).contains("collection handle is null"));
        assert_eq!(
            hegel_collection_reject(ctx, tc, ptr::null_mut(), ptr::null()),
            HEGEL_E_INVALID_HANDLE
        );
        let mut var_id = 0i64;
        assert_eq!(
            hegel_pool_add(ctx, tc, ptr::null_mut(), &mut var_id),
            HEGEL_E_INVALID_HANDLE
        );
        assert!(last_error(ctx).contains("pool handle is null"));
        assert_eq!(
            hegel_pool_generate(ctx, tc, ptr::null_mut(), false, &mut var_id),
            HEGEL_E_INVALID_HANDLE
        );

        assert_eq!(
            hegel_new_collection(ctx, tc, 0, 3, &mut collection),
            HEGEL_OK
        );
        assert!(!collection.is_null());
        assert_eq!(
            hegel_collection_more(ctx, tc, collection, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_collection_reject(ctx, tc, collection, bad_utf8.as_ptr()),
            HEGEL_E_INVALID_ARG
        );
        let mut more = false;
        if hegel_collection_more(ctx, tc, collection, &mut more) == HEGEL_OK && more {
            let _ = hegel_generate_integer(ctx, tc, 0, 100, &mut value);
            assert_eq!(
                hegel_collection_reject(ctx, tc, collection, ptr::null()),
                HEGEL_OK
            );
        }
        assert_eq!(hegel_collection_free(ctx, collection), HEGEL_OK);

        let mut pool: *mut HegelPool = ptr::null_mut();
        assert_eq!(hegel_new_pool(ctx, tc, &mut pool), HEGEL_OK);
        assert!(!pool.is_null());
        assert_eq!(
            hegel_pool_add(ctx, tc, pool, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_pool_generate(ctx, tc, pool, false, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_pool_generate(ctx, tc, pool, false, &mut var_id),
            HEGEL_E_ASSUME
        );
        assert_eq!(hegel_pool_free(ctx, pool), HEGEL_OK);

        let mut recursion: *mut HegelRecursion = ptr::null_mut();
        assert_eq!(
            hegel_new_recursion(ctx, tc, 4, 100, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("out parameter is null"));
        let mut branch = false;
        assert_eq!(
            hegel_recursion_branch(ctx, tc, ptr::null_mut(), 0, &mut branch),
            HEGEL_E_INVALID_HANDLE
        );
        assert!(last_error(ctx).contains("recursion handle is null"));
        assert_eq!(
            hegel_recursion_leaf(ctx, tc, ptr::null_mut()),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_recursion_retry(ctx, tc, ptr::null_mut()),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_recursion_finish(ctx, tc, ptr::null_mut()),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_new_recursion(ctx, tc, 4, 100, &mut recursion),
            HEGEL_OK
        );
        assert!(!recursion.is_null());
        assert_eq!(
            hegel_recursion_branch(ctx, tc, recursion, 0, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("out parameter is null"));
        assert_eq!(hegel_recursion_free(ctx, recursion), HEGEL_OK);

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

/// Drives the recursion protocol at the C level: the depth limit forces the
/// branch decision to `false`, the leaf budget trips `HEGEL_E_RETRY`,
/// `hegel_recursion_retry` discards the attempt (closing the spans it left
/// open) and resets the leaf budget, `hegel_recursion_finish` accepts a
/// value whose pricing matches its observed arities, and exhausting the
/// retries concludes the test case invalid, after which every recursion
/// call reports `HEGEL_E_ASSUME`.
#[test]
fn recursion_budget_retry_and_depth_limit() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_seed(ctx, s, 1, true));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert!(!tc.is_null());

        let mut recursion: *mut HegelRecursion = ptr::null_mut();
        ok(hegel_new_recursion(ctx, tc, 0, 2, &mut recursion));
        assert!(!recursion.is_null());

        let mut branch = true;
        ok(hegel_recursion_branch(ctx, tc, recursion, 0, &mut branch));
        assert!(!branch);
        branch = true;
        ok(hegel_recursion_branch(ctx, tc, recursion, 7, &mut branch));
        assert!(!branch);

        ok(hegel_start_span(
            ctx,
            tc,
            hegel_label_t::HEGEL_LABEL_RECURSIVE as u64,
        ));
        ok(hegel_recursion_leaf(ctx, tc, recursion));
        ok(hegel_recursion_leaf(ctx, tc, recursion));
        assert_eq!(hegel_recursion_leaf(ctx, tc, recursion), HEGEL_E_RETRY);
        assert!(last_error(ctx).contains("max_leaves = 2"));
        ok(hegel_recursion_retry(ctx, tc, recursion));

        ok(hegel_recursion_leaf(ctx, tc, recursion));
        ok(hegel_recursion_finish(ctx, tc, recursion));

        for _ in 0..7 {
            ok(hegel_recursion_retry(ctx, tc, recursion));
        }
        assert_eq!(hegel_recursion_retry(ctx, tc, recursion), HEGEL_E_ASSUME);
        assert_eq!(hegel_recursion_leaf(ctx, tc, recursion), HEGEL_E_ASSUME);
        assert_eq!(hegel_recursion_finish(ctx, tc, recursion), HEGEL_E_ASSUME);
        assert_eq!(hegel_recursion_free(ctx, recursion), HEGEL_OK);

        let mut recursion2: *mut HegelRecursion = ptr::null_mut();
        assert_eq!(
            hegel_new_recursion(ctx, tc, 4, 100, &mut recursion2),
            HEGEL_E_ASSUME
        );
        assert!(recursion2.is_null());

        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_INVALID as u32,
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

/// A full run whose test case creates a state machine with
/// `max_concurrency > 1` becomes nondeterministic. The first case's
/// creation is rejected with `HEGEL_E_ASSUME` — the case is discarded like
/// a failed assumption while the run flips — and from the next case on the
/// creation succeeds. The run stops at the first bug and reports
/// `HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC`, surfacing the bug with an
/// origin but no reproduce blob: with replay and shrinking off, there is no
/// shrunk choice sequence to encode, and the caller reports the bug from
/// its own captured output instead.
#[test]
fn nondeterministic_run_failure_has_origin_but_no_blob() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        let run = start(ctx, s);
        let origin = CString::new("nondeterministic bug").unwrap();
        let rule = CString::new("only").unwrap();

        let mut cases = 0usize;
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            cases += 1;
            let mut is_nondeterministic = false;
            ok(hegel_test_case_is_nondeterministic(
                ctx,
                tc,
                &mut is_nondeterministic,
            ));
            assert_eq!(is_nondeterministic, cases > 1);
            let rules = [rule.as_ptr()];
            let rule_groups: [i64; 1] = [0];
            let mut machine: *mut HegelStateMachine = ptr::null_mut();
            let mut out_concurrency = 0i64;
            let rc = hegel_new_state_machine(
                ctx,
                tc,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                2,
                2,
                &mut machine,
                &mut out_concurrency,
            );
            if rc == HEGEL_E_ASSUME {
                assert_eq!(cases, 1, "only the flipping case is rejected");
                assert!(machine.is_null());
                ok(hegel_mark_complete(
                    ctx,
                    tc,
                    hegel_status_t::HEGEL_STATUS_INVALID as u32,
                    ptr::null(),
                ));
                ok(hegel_test_case_free(ctx, tc));
                continue;
            }
            assert_eq!(rc, HEGEL_OK);
            ok(hegel_state_machine_free(ctx, machine));
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
        assert_eq!(
            cases, 2,
            "the discarded flipping case, then the run stops at the first bug"
        );

        let res = result(ctx, run);
        assert!(
            status_of(ctx, res) == hegel_run_status_t::HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC
        );
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

        let mut collection: *mut HegelCollection = ptr::null_mut();
        ok(hegel_new_collection(ctx, tc, 0, 3, &mut collection));
        let mut pool: *mut HegelPool = ptr::null_mut();
        ok(hegel_new_pool(ctx, tc, &mut pool));
        let rule = CString::new("only").unwrap();
        let rules = [rule.as_ptr()];
        let rule_groups: [i64; 1] = [0];
        let mut machine: *mut HegelStateMachine = ptr::null_mut();
        let mut out_concurrency = 0i64;
        ok(hegel_new_state_machine(
            ctx,
            tc,
            rules.as_ptr(),
            rule_groups.as_ptr(),
            1,
            ptr::null(),
            0,
            1,
            1,
            &mut machine,
            &mut out_concurrency,
        ));

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
        let mut post_overrun: *mut HegelCollection = ptr::null_mut();
        assert_eq!(
            hegel_new_collection(ctx, tc, 0, 3, &mut post_overrun),
            HEGEL_E_STOP_TEST
        );
        assert!(post_overrun.is_null());
        let mut more = false;
        assert_eq!(
            hegel_collection_more(ctx, tc, collection, &mut more),
            HEGEL_E_STOP_TEST
        );
        assert_eq!(
            hegel_collection_reject(ctx, tc, collection, ptr::null()),
            HEGEL_E_STOP_TEST
        );
        let mut post_overrun_pool: *mut HegelPool = ptr::null_mut();
        assert_eq!(
            hegel_new_pool(ctx, tc, &mut post_overrun_pool),
            HEGEL_E_STOP_TEST
        );
        assert!(post_overrun_pool.is_null());
        assert_eq!(hegel_pool_add(ctx, tc, pool, &mut id), HEGEL_E_STOP_TEST);
        assert_eq!(
            hegel_pool_generate(ctx, tc, pool, false, &mut id),
            HEGEL_E_STOP_TEST
        );
        let mut post_overrun_machine: *mut HegelStateMachine = ptr::null_mut();
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                1,
                &mut post_overrun_machine,
                &mut out_concurrency,
            ),
            HEGEL_E_STOP_TEST
        );
        assert!(post_overrun_machine.is_null());
        assert_eq!(
            hegel_state_machine_next_group(ctx, tc, machine, &mut id),
            HEGEL_E_STOP_TEST
        );
        assert_eq!(
            hegel_state_machine_next_rule(ctx, tc, machine, 0, &mut id),
            HEGEL_E_STOP_TEST
        );
        ok(hegel_collection_free(ctx, collection));
        ok(hegel_pool_free(ctx, pool));
        ok(hegel_state_machine_free(ctx, machine));

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

/// Exercise the state-machine and weighted-boolean C-ABI entry points
/// (`hegel_new_state_machine`, `hegel_state_machine_next_group`,
/// `hegel_state_machine_next_rule`, `hegel_state_machine_rule_rejected`,
/// `hegel_state_machine_should_check_invariant`, `hegel_generate_boolean`)
/// in-process: the invalid-handle and
/// argument-validation paths, plus the happy paths. The smoke test that
/// drives these over dlopen doesn't contribute coverage, so they are
/// measured here.
#[test]
fn state_machine_and_primitive_boolean_paths() {
    let bad_utf8: [c_char; 2] = [0xFFu8 as c_char, 0];
    let ctx = hegel_context_new();
    unsafe {
        let null_tc: *mut HegelTestCase = ptr::null_mut();
        let rule_a = CString::new("a").unwrap();
        let rules: [*const c_char; 1] = [rule_a.as_ptr()];
        let rule_groups: [i64; 1] = [0];
        let mut machine: *mut HegelStateMachine = ptr::null_mut();
        let mut out_id = 0i64;
        let mut out_concurrency = 0i64;
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                null_tc,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                1,
                &mut machine,
                &mut out_concurrency,
            ),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_state_machine_next_rule(ctx, null_tc, ptr::null_mut(), 0, &mut out_id),
            HEGEL_E_INVALID_HANDLE
        );
        let mut group_idx = 0i64;
        assert_eq!(
            hegel_state_machine_next_group(ctx, null_tc, ptr::null_mut(), &mut group_idx),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_state_machine_rule_rejected(ctx, null_tc, ptr::null_mut(), 0),
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
        ok(hegel_c::hegel_settings_set_stateful_step_count(ctx, s, 10));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert!(!tc.is_null());

        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                1,
                ptr::null_mut(),
                &mut out_concurrency,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                1,
                &mut machine,
                ptr::null_mut(),
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("out parameter is null"));
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                ptr::null(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                1,
                &mut machine,
                &mut out_concurrency,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(machine.is_null());
        assert!(last_error(ctx).contains("rule_names pointer is null"));
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                rules.as_ptr(),
                ptr::null(),
                1,
                ptr::null(),
                0,
                1,
                1,
                &mut machine,
                &mut out_concurrency,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("rule_groups is null"));
        let null_entry: [*const c_char; 1] = [ptr::null()];
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                null_entry.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                1,
                &mut machine,
                &mut out_concurrency,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("rule_names[0] is null"));
        let bad_entry: [*const c_char; 1] = [bad_utf8.as_ptr()];
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                bad_entry.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                1,
                &mut machine,
                &mut out_concurrency,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("not valid UTF-8"));
        let bad_inv: [*const c_char; 1] = [ptr::null()];
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                bad_inv.as_ptr(),
                1,
                1,
                1,
                &mut machine,
                &mut out_concurrency,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("invariant_names[0] is null"));
        let reserved_groups: [i64; 1] = [HEGEL_STATE_MACHINE_DONE];
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                rules.as_ptr(),
                reserved_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                1,
                &mut machine,
                &mut out_concurrency,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("reserved as the termination sentinel"));
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                0,
                1,
                &mut machine,
                &mut out_concurrency,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("concurrency bounds must satisfy 1 <= min <= max"));
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                3,
                2,
                &mut machine,
                &mut out_concurrency,
            ),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("concurrency bounds must satisfy 1 <= min <= max"));

        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                1,
                1,
                &mut machine,
                &mut out_concurrency,
            ),
            HEGEL_OK
        );
        assert!(!machine.is_null());
        assert_eq!(
            out_concurrency, 1,
            "fixed bounds yield the fixed level without consuming entropy"
        );
        assert_eq!(
            hegel_state_machine_next_rule(ctx, tc, machine, 0, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_state_machine_next_group(ctx, tc, machine, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_state_machine_next_rule(ctx, tc, ptr::null_mut(), 0, &mut out_id),
            HEGEL_E_INVALID_HANDLE
        );
        assert!(last_error(ctx).contains("state machine handle is null"));
        assert_eq!(
            hegel_state_machine_next_group(ctx, tc, ptr::null_mut(), &mut group_idx),
            HEGEL_E_INVALID_HANDLE
        );
        assert!(last_error(ctx).contains("state machine handle is null"));
        assert_eq!(
            hegel_state_machine_rule_rejected(ctx, tc, ptr::null_mut(), 0),
            HEGEL_E_INVALID_HANDLE
        );
        assert!(last_error(ctx).contains("state machine handle is null"));
        assert_eq!(
            hegel_state_machine_rule_rejected(ctx, tc, machine, 0),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("no outstanding rule"));
        let mut rule_idx = -1i64;
        assert_eq!(
            hegel_state_machine_next_rule(ctx, tc, machine, 0, &mut rule_idx),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("before the first next_group"));
        let mut rounds = 0;
        let mut rejected_once = false;
        loop {
            assert_eq!(
                hegel_state_machine_next_group(ctx, tc, machine, &mut group_idx),
                HEGEL_OK
            );
            if group_idx == HEGEL_STATE_MACHINE_DONE {
                break;
            }
            assert_eq!(group_idx, 0, "a single-group machine is always in group 0");
            rounds += 1;
            assert!(
                rounds <= 11,
                "at most stateful_step_count counted rounds plus one rejected round"
            );
            assert_eq!(
                hegel_state_machine_next_rule(ctx, tc, machine, 1, &mut rule_idx),
                HEGEL_E_INVALID_ARG
            );
            assert!(last_error(ctx).contains("worker_index must be in [0, 1)"));
            assert_eq!(
                hegel_state_machine_next_rule(ctx, tc, machine, 0, &mut rule_idx),
                HEGEL_OK
            );
            assert_eq!(rule_idx, 0, "a single-rule machine always selects rule 0");
            if !rejected_once {
                rejected_once = true;
                assert_eq!(
                    hegel_state_machine_rule_rejected(ctx, tc, machine, 0),
                    HEGEL_OK
                );
                assert_eq!(
                    hegel_state_machine_rule_rejected(ctx, tc, machine, 0),
                    HEGEL_E_INVALID_ARG
                );
                assert!(last_error(ctx).contains("no outstanding rule"));
            }
            assert_eq!(
                hegel_state_machine_next_rule(ctx, tc, machine, 0, &mut rule_idx),
                HEGEL_OK
            );
            assert_eq!(
                rule_idx, HEGEL_STATE_MACHINE_DONE,
                "a sequential machine hands out one rule per round"
            );
        }
        assert!(rounds >= 1);
        assert_eq!(hegel_state_machine_free(ctx, machine), HEGEL_OK);

        let inv_a = CString::new("inv").unwrap();
        let invariants: [*const c_char; 1] = [inv_a.as_ptr()];
        let mut checked: *mut HegelStateMachine = ptr::null_mut();
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                invariants.as_ptr(),
                1,
                1,
                1,
                &mut checked,
                &mut out_concurrency,
            ),
            HEGEL_OK
        );
        let mut should_check = false;
        assert_eq!(
            hegel_state_machine_should_check_invariant(ctx, null_tc, checked, 0, &mut should_check),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_state_machine_should_check_invariant(
                ctx,
                tc,
                ptr::null_mut(),
                0,
                &mut should_check
            ),
            HEGEL_E_INVALID_HANDLE
        );
        assert!(last_error(ctx).contains("state machine handle is null"));
        assert_eq!(
            hegel_state_machine_should_check_invariant(ctx, tc, checked, 0, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("out parameter is null"));
        assert_eq!(
            hegel_state_machine_should_check_invariant(ctx, tc, checked, 1, &mut should_check),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("invariant_index must be in [0, 1)"));
        for _ in 0..20 {
            assert_eq!(
                hegel_state_machine_should_check_invariant(ctx, tc, checked, 0, &mut should_check),
                HEGEL_OK
            );
        }
        assert_eq!(hegel_state_machine_free(ctx, checked), HEGEL_OK);

        let mut ranged: *mut HegelStateMachine = ptr::null_mut();
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                2,
                4,
                &mut ranged,
                &mut out_concurrency,
            ),
            HEGEL_E_ASSUME,
            "the first concurrent creation of a run is rejected while the run flips"
        );
        assert!(ranged.is_null());

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

        let tc = next_case(ctx, run);
        assert!(!tc.is_null());
        assert_eq!(
            hegel_new_state_machine(
                ctx,
                tc,
                rules.as_ptr(),
                rule_groups.as_ptr(),
                1,
                ptr::null(),
                0,
                2,
                4,
                &mut ranged,
                &mut out_concurrency,
            ),
            HEGEL_OK,
            "once the run is nondeterministic, concurrent creations succeed"
        );
        assert!(!ranged.is_null());
        assert!(
            (2..=4).contains(&out_concurrency),
            "the drawn level respects the bounds, got {out_concurrency}"
        );
        assert_eq!(hegel_state_machine_free(ctx, ranged), HEGEL_OK);
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
        for tc in [root, c1, c1a] {
            let mut is_nondeterministic = true;
            ok(hegel_test_case_is_nondeterministic(
                ctx,
                tc,
                &mut is_nondeterministic,
            ));
            assert!(!is_nondeterministic);
        }
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
        let mut is_nondeterministic = false;
        ok(hegel_test_case_is_nondeterministic(
            ctx,
            root,
            &mut is_nondeterministic,
        ));
        assert!(!is_nondeterministic);

        let mut c1: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(hegel_test_case_clone(ctx, root, &mut c1), HEGEL_OK);
        let mut c2: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(hegel_test_case_clone(ctx, root, &mut c2), HEGEL_OK);
        for tc in [c1, c2] {
            is_nondeterministic = false;
            ok(hegel_test_case_is_nondeterministic(
                ctx,
                tc,
                &mut is_nondeterministic,
            ));
            assert!(!is_nondeterministic);
        }

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

/// Collection, pool, and state-machine handles are independent of the test
/// case and run they were created under: they outlive `hegel_test_case_free`
/// and `hegel_run_free`, and the frees themselves are safe in any order —
/// here the run and settings go first and the object handles are released
/// last, in an order unrelated to creation order.
#[test]
fn object_handles_are_freed_safely_after_the_run() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 1));
        ok(hegel_c::hegel_settings_set_seed(ctx, s, 1, true));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert!(!tc.is_null());

        let mut collection: *mut HegelCollection = ptr::null_mut();
        ok(hegel_new_collection(ctx, tc, 0, 3, &mut collection));
        let mut pool: *mut HegelPool = ptr::null_mut();
        ok(hegel_new_pool(ctx, tc, &mut pool));
        let mut var_id = 0i64;
        ok(hegel_pool_add(ctx, tc, pool, &mut var_id));
        let rule = CString::new("only").unwrap();
        let rules = [rule.as_ptr()];
        let rule_groups: [i64; 1] = [0];
        let mut machine: *mut HegelStateMachine = ptr::null_mut();
        let mut out_concurrency = 0i64;
        ok(hegel_new_state_machine(
            ctx,
            tc,
            rules.as_ptr(),
            rule_groups.as_ptr(),
            1,
            ptr::null(),
            0,
            1,
            1,
            &mut machine,
            &mut out_concurrency,
        ));

        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_VALID as u32,
            ptr::null(),
        ));
        ok(hegel_test_case_free(ctx, tc));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));

        ok(hegel_state_machine_free(ctx, machine));
        ok(hegel_collection_free(ctx, collection));
        ok(hegel_pool_free(ctx, pool));
        ok(hegel_context_free(ctx));
    }
}

/// A collection created through the root test-case handle is driven through
/// a clone handle: the continue/stop decisions draw from the clone's stream
/// and the collection still respects its size bounds.
#[test]
fn collection_created_on_the_root_is_driven_via_a_clone() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 1));
        ok(hegel_c::hegel_settings_set_seed(ctx, s, 7, true));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert!(!tc.is_null());

        let mut collection: *mut HegelCollection = ptr::null_mut();
        ok(hegel_new_collection(ctx, tc, 1, 4, &mut collection));

        let mut clone: *mut HegelTestCase = ptr::null_mut();
        ok(hegel_test_case_clone(ctx, tc, &mut clone));
        assert!(!clone.is_null());

        let mut n = 0u64;
        loop {
            let mut more = false;
            ok(hegel_collection_more(ctx, clone, collection, &mut more));
            if !more {
                break;
            }
            let mut value = false;
            ok(hegel_generate_boolean(
                ctx, clone, 0.5, false, false, &mut value,
            ));
            n += 1;
        }
        assert!((1..=4).contains(&n), "collection produced {n} elements");

        ok(hegel_collection_free(ctx, collection));
        ok(hegel_test_case_free(ctx, clone));
        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_VALID as u32,
            ptr::null(),
        ));
        ok(hegel_test_case_free(ctx, tc));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// A raw pointer wrapper so test threads can share libhegel handles; sound
/// here because pools serialize internally and each thread drives its own
/// test-case clone.
struct SendPtr<T>(*mut T);
impl<T> Copy for SendPtr<T> {}
impl<T> Clone for SendPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}
unsafe impl<T> Send for SendPtr<T> {}
unsafe impl<T> Sync for SendPtr<T> {}

/// One pool shared between two clone handles driven from parallel threads:
/// the pool's internal lock serializes concurrent `hegel_pool_add` /
/// `hegel_pool_generate` calls, every drawn id is one that was added, and
/// consumed ids are handed out exactly once.
#[test]
fn pool_is_shared_across_two_clone_threads() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 1));
        ok(hegel_c::hegel_settings_set_seed(ctx, s, 11, true));
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert!(!tc.is_null());

        let mut pool: *mut HegelPool = ptr::null_mut();
        ok(hegel_new_pool(ctx, tc, &mut pool));

        let mut clone_a: *mut HegelTestCase = ptr::null_mut();
        ok(hegel_test_case_clone(ctx, tc, &mut clone_a));
        let mut clone_b: *mut HegelTestCase = ptr::null_mut();
        ok(hegel_test_case_clone(ctx, tc, &mut clone_b));

        let pool = SendPtr(pool);
        let consumed = Mutex::new(Vec::<i64>::new());
        std::thread::scope(|scope| {
            for clone in [SendPtr(clone_a), SendPtr(clone_b)] {
                let consumed = &consumed;
                scope.spawn(move || {
                    let clone = clone;
                    let pool = pool;
                    let worker_ctx = hegel_context_new();
                    let mut added = Vec::new();
                    for _ in 0..8 {
                        let mut var_id = 0i64;
                        ok(hegel_pool_add(worker_ctx, clone.0, pool.0, &mut var_id));
                        added.push(var_id);
                    }
                    for _ in 0..4 {
                        let mut drawn = 0i64;
                        ok(hegel_pool_generate(
                            worker_ctx, clone.0, pool.0, true, &mut drawn,
                        ));
                        assert!(drawn >= 0, "drew an id that was never added: {drawn}");
                        consumed.lock().unwrap().push(drawn);
                    }
                    ok(hegel_context_free(worker_ctx));
                });
            }
        });
        let mut consumed = consumed.into_inner().unwrap();
        let n = consumed.len();
        consumed.sort_unstable();
        consumed.dedup();
        assert_eq!(n, 8, "each consuming draw returns one variable");
        assert_eq!(consumed.len(), 8, "no consumed variable is drawn twice");
        assert!(consumed.iter().all(|id| (0..16).contains(id)));

        ok(hegel_test_case_free(ctx, clone_a));
        ok(hegel_test_case_free(ctx, clone_b));
        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_VALID as u32,
            ptr::null(),
        ));
        ok(hegel_test_case_free(ctx, tc));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_pool_free(ctx, pool.0));
        ok(hegel_context_free(ctx));
    }
}

/// Repeated multi-case runs where each test case drives one pool from two
/// concurrent clone streams. The fresh-id window is anchored on the family
/// registry of ids ever drawn, so racing clone streams can skew each other's
/// recorded ranges (kind drift inside clone records is tolerated) and the
/// data tree must never report the run as non-deterministic.
#[test]
fn concurrent_clone_pools_do_not_trip_nondeterminism_detection() {
    for seed in [3u64, 4, 5] {
        let ctx = hegel_context_new();
        unsafe {
            let s = make_settings(ctx);
            let empty = CString::new("").unwrap();
            ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
            ok(hegel_c::hegel_settings_set_test_cases(ctx, s, 10));
            ok(hegel_c::hegel_settings_set_seed(ctx, s, seed, true));
            let run = start(ctx, s);
            loop {
                let tc = next_case(ctx, run);
                if tc.is_null() {
                    assert_eq!(last_error(ctx), "");
                    break;
                }
                let mut pool: *mut HegelPool = ptr::null_mut();
                ok(hegel_new_pool(ctx, tc, &mut pool));
                let mut clone_a: *mut HegelTestCase = ptr::null_mut();
                ok(hegel_test_case_clone(ctx, tc, &mut clone_a));
                let mut clone_b: *mut HegelTestCase = ptr::null_mut();
                ok(hegel_test_case_clone(ctx, tc, &mut clone_b));
                let pool = SendPtr(pool);
                std::thread::scope(|scope| {
                    for clone in [SendPtr(clone_a), SendPtr(clone_b)] {
                        scope.spawn(move || {
                            let clone = clone;
                            let pool = pool;
                            let worker_ctx = hegel_context_new();
                            for _ in 0..4 {
                                let mut var_id = 0i64;
                                ok(hegel_pool_add(worker_ctx, clone.0, pool.0, &mut var_id));
                                assert!(var_id >= 0);
                                let mut drawn = 0i64;
                                ok(hegel_pool_generate(
                                    worker_ctx, clone.0, pool.0, false, &mut drawn,
                                ));
                                assert!(drawn >= 0);
                            }
                            ok(hegel_context_free(worker_ctx));
                        });
                    }
                });
                ok(hegel_test_case_free(ctx, clone_a));
                ok(hegel_test_case_free(ctx, clone_b));
                ok(hegel_mark_complete(
                    ctx,
                    tc,
                    hegel_status_t::HEGEL_STATUS_VALID as u32,
                    ptr::null(),
                ));
                ok(hegel_test_case_free(ctx, tc));
                ok(hegel_pool_free(ctx, pool.0));
            }
            let res = result(ctx, run);
            assert!(status_of(ctx, res) == hegel_run_status_t::HEGEL_RUN_STATUS_PASSED);
            assert!(run_error_of(ctx, res).is_null());
            ok(hegel_run_result_free(ctx, res));
            ok(hegel_run_free(ctx, run));
            ok(hegel_settings_free(ctx, s));
            ok(hegel_context_free(ctx));
        }
    }
}

//! Miri-targeted C-ABI tests.
//!
//! Miri's job here is to catch use-after-free, double-free, leaks, and data
//! races across the C-ABI boundary. The exhaustive behaviour and every error
//! path live in `c_abi_inprocess.rs`; running that whole suite under Miri is
//! intractable (chiefly the test that draws up to a million times to force an
//! overrun). This file is the tractable subset: the reference-counted
//! clone/free handle lifecycle (driven with span ops, which don't consume the
//! choice budget, off a single passing case) plus one *complete* run that
//! generates, fails, and shrinks — so the run loop, the engine, and the
//! result/failure/blob readers are all exercised through the raw C ABI under
//! Miri, not just the handle pointers.

mod common;

use common::ok;
use hegel_c::hegel_result_t::*;
use hegel_c::{
    HegelContext, HegelRun, HegelRunResult, HegelSettings, HegelTestCase, hegel_context_free,
    hegel_context_new, hegel_failure_free, hegel_failure_reproduction_blob, hegel_generate_integer,
    hegel_mark_complete, hegel_next_test_case, hegel_run_free, hegel_run_result,
    hegel_run_result_failure, hegel_run_result_failure_count, hegel_run_result_free,
    hegel_run_result_status, hegel_run_start, hegel_run_status_t, hegel_settings_free,
    hegel_settings_new, hegel_settings_set_database, hegel_settings_set_seed,
    hegel_settings_set_test_cases, hegel_start_span, hegel_status_t, hegel_stop_span,
    hegel_test_case_clone, hegel_test_case_free,
};
use std::ffi::CString;
use std::ptr;

/// Carries a test-case handle into a spawned thread.
struct SendPtr(*mut HegelTestCase);
// SAFETY: every use wraps a distinct clone handle with its own lock, and the
// spawning test joins its threads before any handle is freed.
unsafe impl Send for SendPtr {}

/// Start a database-free single-case run and hand back its first run-owned
/// handle, keeping the owning context/settings/run alive.
unsafe fn one_case_run() -> (
    *mut HegelContext,
    *mut HegelSettings,
    *mut HegelRun,
    *mut HegelTestCase,
) {
    unsafe {
        let ctx = hegel_context_new();
        let mut s: *mut HegelSettings = ptr::null_mut();
        assert_eq!(hegel_settings_new(ctx, &mut s), HEGEL_OK);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_settings_set_test_cases(ctx, s, 1));
        ok(hegel_settings_set_seed(ctx, s, 1, true));
        let mut run: *mut HegelRun = ptr::null_mut();
        assert_eq!(hegel_run_start(ctx, s, &mut run), HEGEL_OK);
        let mut tc: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(hegel_next_test_case(ctx, run, &mut tc), HEGEL_OK);
        assert!(!tc.is_null());
        (ctx, s, run, tc)
    }
}

/// A span op proves a handle is live and reaches its stream without
/// consuming the choice budget.
unsafe fn alive(ctx: *mut HegelContext, tc: *mut HegelTestCase) {
    unsafe {
        assert_eq!(hegel_start_span(ctx, tc, 1), HEGEL_OK);
        assert_eq!(hegel_stop_span(ctx, tc, false), HEGEL_OK);
    }
}

/// A root, a clone, and a clone-of-a-clone are independent handles onto one
/// reference-counted test case: freeing any one drops only its own reference,
/// and the case stays alive (and the surviving handles usable) until the last
/// handle is freed. Run under Miri this proves the orders are free of
/// use-after-free, double-free, and leaks.
#[test]
fn clones_and_root_free_independently_in_any_order() {
    unsafe {
        let (ctx, s, run, root) = one_case_run();

        let mut c1: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(hegel_test_case_clone(ctx, root, &mut c1), HEGEL_OK);
        let mut c2: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(hegel_test_case_clone(ctx, c1, &mut c2), HEGEL_OK);

        alive(ctx, root);
        alive(ctx, c1);
        alive(ctx, c2);

        // Free a clone; the root and the other clone stay live.
        assert_eq!(hegel_test_case_free(ctx, c1), HEGEL_OK);
        alive(ctx, root);
        alive(ctx, c2);

        // Free the root; the surviving clone keeps the case alive and usable.
        assert_eq!(hegel_test_case_free(ctx, root), HEGEL_OK);
        alive(ctx, c2);

        // Complete via the surviving clone, then free it (the last handle).
        assert_eq!(
            hegel_mark_complete(ctx, c2, hegel_status_t::HEGEL_STATUS_VALID, ptr::null()),
            HEGEL_OK
        );
        assert_eq!(hegel_test_case_free(ctx, c2), HEGEL_OK);

        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// Two clones are driven from two threads at once (each handle has its own
/// lock), then every handle is freed. Run under Miri this checks the concurrent
/// access to the shared family and the cross-thread free are race- and
/// UB-free.
#[test]
fn two_clones_used_concurrently_then_freed() {
    use std::sync::{Arc, Barrier};

    unsafe {
        let (ctx, s, run, root) = one_case_run();

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
                    let cp = cp;
                    let tctx = hegel_context_new();
                    b.wait();
                    let rc = hegel_start_span(tctx, cp.0, 1);
                    let rc2 = hegel_stop_span(tctx, cp.0, false);
                    ok(hegel_context_free(tctx));
                    (rc, rc2)
                })
            })
            .collect();
        for h in handles {
            let (rc, rc2) = h.join().unwrap();
            assert_eq!(rc, HEGEL_OK);
            assert_eq!(rc2, HEGEL_OK);
        }

        ok(hegel_mark_complete(
            ctx,
            root,
            hegel_status_t::HEGEL_STATUS_VALID,
            ptr::null(),
        ));
        ok(hegel_test_case_free(ctx, c1));
        ok(hegel_test_case_free(ctx, c2));
        ok(hegel_test_case_free(ctx, root));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// Two clones race to complete the same test case from two threads. Completion
/// is first-caller-wins and family-wide, so *neither* call errors — the winner
/// records the outcome and the loser is a safe no-op (both return `HEGEL_OK`).
/// Run under Miri this checks the concurrent `compare_exchange` / `ds`
/// completion / ack path is race- and UB-free.
#[test]
fn concurrent_mark_complete_from_two_clones_is_safe() {
    use std::sync::{Arc, Barrier};

    unsafe {
        let (ctx, s, run, root) = one_case_run();

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
                    let cp = cp;
                    let tctx = hegel_context_new();
                    b.wait();
                    let rc = hegel_mark_complete(
                        tctx,
                        cp.0,
                        hegel_status_t::HEGEL_STATUS_VALID,
                        ptr::null(),
                    );
                    ok(hegel_context_free(tctx));
                    rc
                })
            })
            .collect();
        for h in handles {
            // Neither racing clone errors: winner sets the result, loser no-ops.
            assert_eq!(h.join().unwrap(), HEGEL_OK);
        }

        ok(hegel_test_case_free(ctx, c1));
        ok(hegel_test_case_free(ctx, c2));
        ok(hegel_test_case_free(ctx, root));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// Drive one complete run that always fails and therefore shrinks, through the
/// raw C ABI: each case draws an integer and is marked INTERESTING, so the
/// engine reports a failure and runs its full shrink. The result and failure
/// snapshots are read back — status, failure count, origin, reproduction blob
/// — *after* the run and settings have been freed, so Miri checks both the
/// run loop / shrinker and the snapshots' independence from the run (the
/// handle-lifecycle tests above never generate or shrink). A small example
/// count and the minimal `[0, 100]` integer keep the shrink tractable for
/// Miri's interpreter, as the engine/shrinking tests in `test_miri` do.
#[test]
fn full_run_generates_fails_and_shrinks() {
    unsafe {
        let ctx = hegel_context_new();
        let mut s: *mut HegelSettings = ptr::null_mut();
        ok(hegel_settings_new(ctx, &mut s));
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_settings_set_test_cases(ctx, s, 5));
        ok(hegel_settings_set_seed(ctx, s, 1, true));
        let mut run: *mut HegelRun = ptr::null_mut();
        ok(hegel_run_start(ctx, s, &mut run));

        loop {
            let mut tc: *mut HegelTestCase = ptr::null_mut();
            ok(hegel_next_test_case(ctx, run, &mut tc));
            if tc.is_null() {
                break;
            }
            let mut value = 0i64;
            // Always interesting when a value is drawn; OVERRUN otherwise. This
            // makes the engine shrink toward the minimal failing example.
            let status = if hegel_generate_integer(ctx, tc, 0, 100, &mut value) == HEGEL_OK {
                hegel_status_t::HEGEL_STATUS_INTERESTING
            } else {
                hegel_status_t::HEGEL_STATUS_OVERRUN
            };
            ok(hegel_mark_complete(ctx, tc, status, ptr::null()));
            ok(hegel_test_case_free(ctx, tc));
        }

        let mut res: *mut HegelRunResult = ptr::null_mut();
        ok(hegel_run_result(ctx, run, &mut res));
        let mut f: *mut hegel_c::HegelFailure = ptr::null_mut();
        ok(hegel_run_result_failure(ctx, res, 0, &mut f));
        assert!(!f.is_null());
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));

        let mut run_status = hegel_run_status_t::HEGEL_RUN_STATUS_PASSED;
        ok(hegel_run_result_status(ctx, res, &mut run_status));
        assert!(run_status == hegel_run_status_t::HEGEL_RUN_STATUS_FAILED);
        let mut count = 0usize;
        ok(hegel_run_result_failure_count(ctx, res, &mut count));
        assert!(
            count >= 1,
            "an always-interesting property records a failure"
        );
        let mut blob: *const std::os::raw::c_char = ptr::null();
        ok(hegel_failure_reproduction_blob(ctx, f, &mut blob));
        assert!(
            !blob.is_null(),
            "a shrunk failure carries a reproduction blob"
        );
        ok(hegel_failure_free(ctx, f));
        ok(hegel_run_result_free(ctx, res));
        ok(hegel_context_free(ctx));
    }
}

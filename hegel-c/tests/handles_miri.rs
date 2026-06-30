//! Fast, Miri-targeted test-case handle-lifecycle checks.
//!
//! Miri's job here is to catch use-after-free, double-free, and leaks in the
//! reference-counted clone/free pointer logic — and that needs only a handle or
//! two, not a full property run. The exhaustive C-ABI behaviour (shrinking,
//! overrun, every error path) lives in `c_abi_inprocess.rs`, which is far too
//! slow to interpret under Miri. So these tests deliberately use a single
//! passing test case and span ops (which don't consume the choice budget) and
//! avoid generation/shrinking entirely, keeping the Miri run tractable while
//! still exercising clone, free-in-any-order, shared completion, and two clones
//! used concurrently from two threads.

use hegel_c::hegel_result_t::*;
use hegel_c::{
    HegelContext, HegelRun, HegelSettings, HegelTestCase, hegel_context_free, hegel_context_new,
    hegel_mark_complete, hegel_next_test_case, hegel_run_free, hegel_run_start,
    hegel_settings_free, hegel_settings_new, hegel_settings_set_database, hegel_settings_set_seed,
    hegel_settings_set_test_cases, hegel_start_span, hegel_status_t, hegel_stop_span,
    hegel_test_case_clone, hegel_test_case_free,
};
use std::ffi::CString;
use std::ptr;

/// Assert a call that should always succeed for these tests returned `HEGEL_OK`.
fn ok(rc: hegel_c::hegel_result_t) {
    assert_eq!(rc, HEGEL_OK);
}

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

/// A span op proves a handle is live and reaches the shared data source without
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

    struct SendPtr(*mut HegelTestCase);
    // SAFETY: each clone is a distinct handle with its own lock; the threads are
    // joined before any handle is freed.
    unsafe impl Send for SendPtr {}

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

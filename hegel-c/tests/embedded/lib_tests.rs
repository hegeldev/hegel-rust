//! Embedded tests for the libhegel C-ABI lib that need access to private
//! internals — chiefly the per-handle `local` lock that backs concurrent-use
//! detection. Public-surface behavior is covered by `tests/c_abi_inprocess.rs`.

use super::*;
use std::ffi::CString;
use std::ptr;

/// Start a database-free, single-seed run and hand back its first live test
/// case (a run-owned root), keeping the owning context/settings/run alive.
unsafe fn start_run_and_first_case() -> (
    *mut HegelContext,
    *mut HegelSettings,
    *mut HegelRun,
    *mut HegelTestCase,
) {
    let ctx = hegel_context_new();
    let mut s: *mut HegelSettings = ptr::null_mut();
    assert_eq!(unsafe { hegel_settings_new(ctx, &mut s) }, HEGEL_OK);
    let empty = CString::new("").unwrap();
    unsafe { hegel_settings_set_database(ctx, s, empty.as_ptr()) };
    unsafe { hegel_settings_set_seed(ctx, s, 1, true) };
    let mut run: *mut HegelRun = ptr::null_mut();
    assert_eq!(unsafe { hegel_run_start(ctx, s, &mut run) }, HEGEL_OK);
    let mut tc: *mut HegelTestCase = ptr::null_mut();
    assert_eq!(unsafe { hegel_next_test_case(ctx, run, &mut tc) }, HEGEL_OK);
    assert!(!tc.is_null());
    (ctx, s, run, tc)
}

/// Mark the in-flight case valid and tear the run down (draining any case the
/// worker has queued behind it).
unsafe fn finish(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    run: *mut HegelRun,
    tc: *mut HegelTestCase,
) {
    unsafe {
        hegel_mark_complete(ctx, tc, hegel_status_t::HEGEL_STATUS_VALID, ptr::null());
        hegel_test_case_free(ctx, tc);
        hegel_run_free(ctx, run);
        hegel_settings_free(ctx, s);
        hegel_context_free(ctx);
    }
}

/// A single handle held by one thread rejects use from another. We stand in for
/// "another thread is mid-draw" by holding the handle's own `local` lock on
/// this thread: `parking_lot::Mutex` is not reentrant, so `try_lock` observes
/// contention identically to a real second thread — but deterministically, with
/// no race to lose.
#[test]
fn concurrent_use_of_one_handle_is_rejected() {
    unsafe {
        let (ctx, s, run, tc) = start_run_and_first_case();

        let held = (&*tc).local.lock();
        assert_eq!(hegel_start_span(ctx, tc, 1), HEGEL_E_CONCURRENT_USE);
        assert_eq!(
            hegel_mark_complete(ctx, tc, hegel_status_t::HEGEL_STATUS_VALID, ptr::null()),
            HEGEL_E_CONCURRENT_USE
        );
        drop(held);

        // With the lock free the handle works again.
        assert_eq!(hegel_start_span(ctx, tc, 1), HEGEL_OK);
        assert_eq!(hegel_stop_span(ctx, tc, false), HEGEL_OK);

        finish(ctx, s, run, tc);
    }
}

/// Completion is checked before the per-handle lock, so an already-complete
/// handle reports `ALREADY_COMPLETE` even when its lock is held — completion
/// wins over contention.
#[test]
fn completion_is_reported_before_concurrent_use() {
    unsafe {
        let (ctx, s, run, tc) = start_run_and_first_case();

        assert_eq!(
            hegel_mark_complete(ctx, tc, hegel_status_t::HEGEL_STATUS_VALID, ptr::null()),
            HEGEL_OK
        );

        let held = (&*tc).local.lock();
        assert_eq!(hegel_start_span(ctx, tc, 1), HEGEL_E_ALREADY_COMPLETE);
        drop(held);

        hegel_test_case_free(ctx, tc);
        hegel_run_free(ctx, run);
        hegel_settings_free(ctx, s);
        hegel_context_free(ctx);
    }
}

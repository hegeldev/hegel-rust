//! Embedded tests for the libhegel C-ABI lib that need access to private
//! internals — chiefly the per-handle `local` lock that backs concurrent-use
//! detection. Public-surface behavior is covered by `tests/c_abi_inprocess.rs`.

use super::*;
use std::ffi::CString;
use std::ptr;

/// Assert a call that should always succeed for these tests returned `HEGEL_OK`.
fn ok(rc: hegel_result_t) {
    assert_eq!(rc, HEGEL_OK);
}

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
    ok(unsafe { hegel_settings_set_database(ctx, s, empty.as_ptr()) });
    ok(unsafe { hegel_settings_set_seed(ctx, s, 1, true) });
    let mut run: *mut HegelRun = ptr::null_mut();
    assert_eq!(
        unsafe { hegel_run_start(ctx, s, None, ptr::null_mut(), &mut run) },
        HEGEL_OK
    );
    let mut tc: *mut HegelTestCase = ptr::null_mut();
    assert_eq!(unsafe { hegel_next_test_case(ctx, run, &mut tc) }, HEGEL_OK);
    assert!(!tc.is_null());
    (ctx, s, run, tc)
}

/// Mark the in-flight case valid and tear the run down.
unsafe fn finish(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    run: *mut HegelRun,
    tc: *mut HegelTestCase,
) {
    unsafe {
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

/// A single handle held by one thread rejects draw primitives from another.
/// We stand in for "another thread is mid-draw" by holding the handle's own
/// `local` lock on this thread: the engine's mutex is not reentrant, so
/// `try_lock` observes contention identically to a real second thread — but
/// deterministically, with no race to lose.
#[test]
fn concurrent_use_of_one_handle_is_rejected() {
    unsafe {
        let (ctx, s, run, tc) = start_run_and_first_case();

        let held = (&*tc).local.lock();
        assert_eq!(hegel_start_span(ctx, tc, 1), HEGEL_E_CONCURRENT_USE);
        drop(held);

        // With the lock free the handle works again.
        assert_eq!(hegel_start_span(ctx, tc, 1), HEGEL_OK);
        assert_eq!(hegel_stop_span(ctx, tc, false), HEGEL_OK);

        finish(ctx, s, run, tc);
    }
}

/// `hegel_mark_complete` never reports `HEGEL_E_CONCURRENT_USE`: completion is
/// first-caller-wins and always succeeds, so it waits for an in-flight
/// operation on the same handle instead of erroring. A worker thread holds the
/// handle's own `local` lock (standing in for a draw in progress) and releases
/// it shortly after signalling; `hegel_mark_complete`, called while the lock
/// is provably held, blocks until then and completes the case.
#[test]
fn mark_complete_waits_for_an_in_flight_operation() {
    unsafe {
        let (ctx, s, run, tc) = start_run_and_first_case();

        let handle = &*tc;
        std::thread::scope(|scope| {
            let (locked_tx, locked_rx) = std::sync::mpsc::channel();
            scope.spawn(move || {
                let held = handle.local.lock();
                locked_tx.send(()).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(50));
                drop(held);
            });
            locked_rx.recv().unwrap();
            ok(hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null(),
            ));
        });

        ok(hegel_test_case_free(ctx, tc));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
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
            hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null()
            ),
            HEGEL_OK
        );

        let held = (&*tc).local.lock();
        assert_eq!(hegel_start_span(ctx, tc, 1), HEGEL_E_ALREADY_COMPLETE);
        drop(held);

        ok(hegel_test_case_free(ctx, tc));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// A collection used from two threads at once reports
/// `HEGEL_E_CONCURRENT_USE`. As in
/// [`concurrent_use_of_one_handle_is_rejected`], "another thread is
/// mid-operation" is stood in for by holding the collection's own state lock
/// on this thread — the two test-case handles are distinct clones, so the
/// contention observed is the collection's, not the test case's.
#[test]
fn concurrent_use_of_one_collection_is_rejected() {
    unsafe {
        let (ctx, s, run, tc) = start_run_and_first_case();

        let mut collection: *mut HegelCollection = ptr::null_mut();
        ok(hegel_new_collection(ctx, tc, 0, 3, &mut collection));
        let mut clone: *mut HegelTestCase = ptr::null_mut();
        ok(hegel_test_case_clone(ctx, tc, &mut clone));

        let held = (&*collection).state.try_lock().unwrap();
        let mut more = false;
        assert_eq!(
            hegel_collection_more(ctx, clone, collection, &mut more),
            HEGEL_E_CONCURRENT_USE
        );
        assert_eq!(
            hegel_collection_reject(ctx, clone, collection, ptr::null()),
            HEGEL_E_CONCURRENT_USE
        );
        drop(held);

        loop {
            ok(hegel_collection_more(ctx, clone, collection, &mut more));
            if !more {
                break;
            }
            let mut value = false;
            ok(hegel_generate_boolean(
                ctx, clone, 0.5, false, false, &mut value,
            ));
        }

        ok(hegel_collection_free(ctx, collection));
        ok(hegel_test_case_free(ctx, clone));
        finish(ctx, s, run, tc);
    }
}

#[test]
fn size_arg_is_lossless_within_usize_and_saturates_beyond() {
    assert_eq!(size_arg(0), 0);
    assert_eq!(size_arg(255), 255);
    assert_eq!(size_arg(u64::MAX), usize::MAX);
    assert_eq!(
        size_arg(usize::MAX as u64),
        usize::MAX,
        "usize::MAX converts exactly on every target"
    );
}

/// An engine future that suspends without storing a case in the exchange
/// violates the alternation protocol. `hegel_next_test_case` finishes the
/// run with a run-level error instead of panicking; the protocol makes this
/// unreachable for the real engine, so the misbehaving future is injected
/// directly.
#[test]
fn engine_suspending_without_an_offer_becomes_a_run_error() {
    let ctx = hegel_context_new();
    let exchange = Arc::new(CaseExchange::new());
    let engine: EngineFuture = Box::pin(std::future::pending());
    let run = Box::into_raw(Box::new(HegelRun {
        inner: Mutex::new(RunInner {
            engine: Some(engine),
            exchange,
            in_flight: Vec::new(),
            max_open: 1,
            result: None,
        }),
    }));

    unsafe {
        let mut tc: *mut HegelTestCase = ptr::null_mut();
        ok(hegel_next_test_case(ctx, run, &mut tc));
        assert!(tc.is_null(), "a protocol-violating run offers no test case");

        let mut result: *mut HegelRunResult = ptr::null_mut();
        ok(hegel_run_result(ctx, run, &mut result));
        let mut status = hegel_run_status_t::HEGEL_RUN_STATUS_PASSED;
        ok(hegel_run_result_status(ctx, result, &mut status));
        assert!(matches!(status, hegel_run_status_t::HEGEL_RUN_STATUS_ERROR));
        let mut message: *const c_char = ptr::null();
        ok(hegel_run_result_error(ctx, result, &mut message));
        let message = CStr::from_ptr(message).to_str().unwrap();
        assert!(
            message.contains("suspended without offering a test case"),
            "{message}"
        );
        assert!(message.contains("bug in hegel"), "{message}");

        ok(hegel_run_result_free(ctx, result));
        ok(hegel_run_free(ctx, run));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn translate_ds_error_maps_internal_to_hegel_e_internal_with_diagnostic() {
    let ctx = hegel_context_new();
    let e = crate::control::InternalError::new(format_args!("draw invariant violated"));
    let rc = translate_ds_error(ctx, DataSourceError::Internal(e));
    assert_eq!(rc, HEGEL_E_INTERNAL);
    unsafe {
        let msg = CStr::from_ptr(hegel_context_last_error(ctx))
            .to_str()
            .unwrap();
        assert!(msg.contains("draw invariant violated"), "{msg}");
        assert!(msg.contains("bug in hegel"), "{msg}");
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn translate_construct_error_distinguishes_internal_from_invalid_argument() {
    let ctx = hegel_context_new();
    let e = crate::control::InternalError::new(format_args!("constructor invariant"));
    let rc = translate_construct_error(ctx, crate::native::core::EngineError::Internal(e));
    assert_eq!(rc, HEGEL_E_INTERNAL);
    unsafe {
        let msg = CStr::from_ptr(hegel_context_last_error(ctx))
            .to_str()
            .unwrap();
        assert!(msg.contains("constructor invariant"), "{msg}");
    }
    let rc = translate_construct_error(
        ctx,
        crate::native::core::EngineError::InvalidArgument("bad bound".to_string()),
    );
    assert_eq!(rc, HEGEL_E_INVALID_ARG);
    unsafe {
        let msg = CStr::from_ptr(hegel_context_last_error(ctx))
            .to_str()
            .unwrap();
        assert!(msg.contains("bad bound"), "{msg}");
        ok(hegel_context_free(ctx));
    }
}

/// An internal error raised inside the engine's exploration surfaces as a
/// run-level error through `hegel_run_result_error`, exactly where a caught
/// panic's message goes.
#[test]
fn internal_run_error_surfaces_through_run_result_error() {
    let ctx = hegel_context_new();
    let exchange = Arc::new(CaseExchange::new());
    let engine: EngineFuture = Box::pin(async {
        Err(crate::backend::RunError::Internal(
            crate::control::InternalError::new(format_args!("exploration invariant violated")),
        ))
    });
    let run = Box::into_raw(Box::new(HegelRun {
        inner: Mutex::new(RunInner {
            engine: Some(engine),
            exchange,
            in_flight: Vec::new(),
            max_open: 1,
            result: None,
        }),
    }));

    unsafe {
        let mut tc: *mut HegelTestCase = ptr::null_mut();
        ok(hegel_next_test_case(ctx, run, &mut tc));
        assert!(tc.is_null(), "an errored run offers no test case");

        let mut result: *mut HegelRunResult = ptr::null_mut();
        ok(hegel_run_result(ctx, run, &mut result));
        let mut status = hegel_run_status_t::HEGEL_RUN_STATUS_PASSED;
        ok(hegel_run_result_status(ctx, result, &mut status));
        assert!(matches!(status, hegel_run_status_t::HEGEL_RUN_STATUS_ERROR));
        let mut message: *const c_char = ptr::null();
        ok(hegel_run_result_error(ctx, result, &mut message));
        let message = CStr::from_ptr(message).to_str().unwrap();
        assert!(
            message.contains("exploration invariant violated"),
            "{message}"
        );
        assert!(message.contains("bug in hegel"), "{message}");

        ok(hegel_run_result_free(ctx, result));
        ok(hegel_run_free(ctx, run));
        ok(hegel_context_free(ctx));
    }
}

/// The run handle's internal lock turns a concurrent call on the same run
/// into `HEGEL_E_CONCURRENT_USE` instead of a data race. As in
/// [`concurrent_use_of_one_handle_is_rejected`], "another thread is
/// mid-call" is stood in for by holding the run's own lock on this thread.
#[test]
fn concurrent_use_of_a_run_handle_is_rejected() {
    unsafe {
        let (ctx, s, run, tc) = start_run_and_first_case();

        let held = (&*run).inner.try_lock().unwrap();
        let mut next: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(
            hegel_next_test_case(ctx, run, &mut next),
            HEGEL_E_CONCURRENT_USE
        );
        let mut result: *mut HegelRunResult = ptr::null_mut();
        assert_eq!(
            hegel_run_result(ctx, run, &mut result),
            HEGEL_E_CONCURRENT_USE
        );
        drop(held);

        finish(ctx, s, run, tc);
    }
}

/// A worker preempted inside `hegel_mark_complete` — the family's completion
/// claim has landed but the conclusion is not yet written to the data source
/// — must not make the pump misreport the run. The prune in
/// `hegel_next_test_case` may only drop a family once its conclusion is
/// visible to the engine; until then the case counts as open, so the pump
/// gets `HEGEL_E_PENDING`, not a false "internal error" run conclusion.
#[test]
fn half_completed_family_reports_pending_not_a_run_error() {
    let ctx = hegel_context_new();
    unsafe {
        let mut s: *mut HegelSettings = ptr::null_mut();
        assert_eq!(hegel_settings_new(ctx, &mut s), HEGEL_OK);
        let empty = CString::new("").unwrap();
        ok(hegel_settings_set_database(ctx, s, empty.as_ptr()));
        ok(hegel_settings_set_seed(ctx, s, 1, true));
        ok(hegel_settings_set_test_cases(ctx, s, 3));
        ok(hegel_settings_set_max_open_test_cases(ctx, s, 2));
        let mut run: *mut HegelRun = ptr::null_mut();
        assert_eq!(
            hegel_run_start(ctx, s, None, ptr::null_mut(), &mut run),
            HEGEL_OK
        );

        let next = |out: &mut *mut HegelTestCase| -> hegel_result_t {
            *out = ptr::null_mut();
            hegel_next_test_case(ctx, run, out)
        };
        let draw = |tc: *mut HegelTestCase| {
            let mut n: i64 = 0;
            let rc = hegel_generate_integer(ctx, tc, 0, 1000, &mut n);
            assert!(rc == HEGEL_OK || rc == HEGEL_E_STOP_TEST, "rc={rc:?}");
        };
        let complete_valid = |tc: *mut HegelTestCase| {
            ok(hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        };

        let mut probe: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(next(&mut probe), HEGEL_OK);
        assert!(!probe.is_null());
        draw(probe);
        complete_valid(probe);

        let mut a: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(next(&mut a), HEGEL_OK);
        assert!(!a.is_null());
        draw(a);
        let mut b: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(next(&mut b), HEGEL_OK);
        assert!(!b.is_null());
        draw(b);

        // Simulate the preemption: B's completion claim has landed, but the
        // conclusion write has not happened yet.
        let b_ref = &*b;
        b_ref
            .family
            .completed
            .store(true, std::sync::atomic::Ordering::Release);

        complete_valid(a);

        let mut tc: *mut HegelTestCase = ptr::null_mut();
        assert_eq!(
            next(&mut tc),
            HEGEL_E_PENDING,
            "B is still open: the pump must wait for it, not conclude the run"
        );
        assert!(tc.is_null());

        ok(hegel_test_case_free(ctx, b));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

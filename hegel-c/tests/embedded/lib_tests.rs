//! Embedded tests for the libhegel C-ABI lib that need access to private
//! internals — chiefly the per-handle `local` lock that backs concurrent-use
//! detection. Public-surface behavior is covered by `tests/c_abi_inprocess.rs`.

use super::*;
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
    ok(unsafe { hegel_settings_set_database(ctx, s, &"".into()) });
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
        engine: Some(engine),
        exchange,
        current_family: None,
        result: None,
        reporter: None,
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
        let mut message = ptr::null();
        ok(hegel_run_result_error(ctx, result, &mut message));
        let message = (*message).as_str().unwrap();
        assert!(
            message.contains("suspended without offering a test case"),
            "{message}"
        );
        assert!(message.contains("bug in hegel"), "{message}");
        assert_eq!(
            error_kind_of(ctx, result),
            (hegel_run_error_kind_t::HEGEL_RUN_ERROR_INTERNAL, 0)
        );

        ok(hegel_run_result_free(ctx, result));
        ok(hegel_run_free(ctx, run));
        ok(hegel_context_free(ctx));
    }
}

unsafe fn error_kind_of(
    ctx: *mut HegelContext,
    result: *const HegelRunResult,
) -> (hegel_run_error_kind_t, u32) {
    let mut kind = hegel_run_error_kind_t::HEGEL_RUN_ERROR_NONE;
    let mut health_check = u32::MAX;
    ok(unsafe { hegel_run_result_error_kind(ctx, result, &mut kind, &mut health_check) });
    (kind, health_check)
}

/// Every `RunError` the engine can end a run with maps to its own kind
/// through `hegel_run_result_error_kind`, a failed health check also naming
/// the check as the same `hegel_health_check_t` bit
/// `hegel_settings_set_suppress_health_check` takes, with the message
/// unchanged by the classification.
#[test]
fn every_run_error_maps_to_its_kind_through_the_abi() {
    use hegel_health_check_t::*;
    use hegel_run_error_kind_t::*;
    let cases: Vec<(RunError, hegel_run_error_kind_t, u32)> = vec![
        (
            RunError::HealthCheck(HealthCheck::FilterTooMuch, "m".to_string()),
            HEGEL_RUN_ERROR_HEALTH_CHECK,
            HEGEL_HC_FILTER_TOO_MUCH as u32,
        ),
        (
            RunError::HealthCheck(HealthCheck::TooSlow, "m".to_string()),
            HEGEL_RUN_ERROR_HEALTH_CHECK,
            HEGEL_HC_TOO_SLOW as u32,
        ),
        (
            RunError::HealthCheck(HealthCheck::TestCasesTooLarge, "m".to_string()),
            HEGEL_RUN_ERROR_HEALTH_CHECK,
            HEGEL_HC_TEST_CASES_TOO_LARGE as u32,
        ),
        (
            RunError::HealthCheck(HealthCheck::LargeInitialTestCase, "m".to_string()),
            HEGEL_RUN_ERROR_HEALTH_CHECK,
            HEGEL_HC_LARGE_INITIAL_TEST_CASE as u32,
        ),
        (RunError::Flaky("m".to_string()), HEGEL_RUN_ERROR_FLAKY, 0),
        (
            RunError::NonDeterministic("m".to_string()),
            HEGEL_RUN_ERROR_NONDETERMINISTIC,
            0,
        ),
        (
            RunError::Unsatisfiable("m".to_string()),
            HEGEL_RUN_ERROR_UNSATISFIABLE,
            0,
        ),
        (
            RunError::UsageError("m".to_string()),
            HEGEL_RUN_ERROR_USAGE,
            0,
        ),
        (
            RunError::Internal(crate::control::InternalError::new(format_args!("m"))),
            HEGEL_RUN_ERROR_INTERNAL,
            0,
        ),
    ];
    for (error, expected_kind, expected_check) in cases {
        let ctx = hegel_context_new();
        let expected_message = error.to_string();
        let engine: EngineFuture = Box::pin(async move { Err(error) });
        let run = Box::into_raw(Box::new(HegelRun {
            engine: Some(engine),
            exchange: Arc::new(CaseExchange::new()),
            current_family: None,
            result: None,
            reporter: None,
        }));
        unsafe {
            let mut tc: *mut HegelTestCase = ptr::null_mut();
            ok(hegel_next_test_case(ctx, run, &mut tc));
            assert!(tc.is_null());
            let mut result: *mut HegelRunResult = ptr::null_mut();
            ok(hegel_run_result(ctx, run, &mut result));
            let mut status = hegel_run_status_t::HEGEL_RUN_STATUS_PASSED;
            ok(hegel_run_result_status(ctx, result, &mut status));
            assert!(matches!(status, hegel_run_status_t::HEGEL_RUN_STATUS_ERROR));
            assert_eq!(
                error_kind_of(ctx, result),
                (expected_kind, expected_check),
                "{expected_message}"
            );
            let mut message = ptr::null();
            ok(hegel_run_result_error(ctx, result, &mut message));
            assert_eq!((*message).as_str().unwrap(), expected_message);
            ok(hegel_run_result_free(ctx, result));
            ok(hegel_run_free(ctx, run));
            ok(hegel_context_free(ctx));
        }
    }
}

#[test]
fn translate_ds_error_maps_internal_to_hegel_e_internal_with_diagnostic() {
    let ctx = hegel_context_new();
    let e = crate::control::InternalError::new(format_args!("draw invariant violated"));
    let rc = translate_ds_error(ctx, DataSourceError::Internal(e));
    assert_eq!(rc, HEGEL_E_INTERNAL);
    unsafe {
        let msg = hegel_context_last_error(ctx).as_str().unwrap();
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
        let msg = hegel_context_last_error(ctx).as_str().unwrap();
        assert!(msg.contains("constructor invariant"), "{msg}");
    }
    let rc = translate_construct_error(
        ctx,
        crate::native::core::EngineError::InvalidArgument("bad bound".to_string()),
    );
    assert_eq!(rc, HEGEL_E_INVALID_ARG);
    unsafe {
        let msg = hegel_context_last_error(ctx).as_str().unwrap();
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
        engine: Some(engine),
        exchange,
        current_family: None,
        result: None,
        reporter: None,
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
        let mut message = ptr::null();
        ok(hegel_run_result_error(ctx, result, &mut message));
        let message = (*message).as_str().unwrap();
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

/// A printer handle held by one thread rejects operations from another. We
/// stand in for "another thread is mid-operation" by setting the handle's
/// own `busy` flag on this thread, which the guard observes identically to a
/// real second thread — but deterministically, with no race to lose.
#[test]
fn concurrent_use_of_one_printer_handle_is_rejected() {
    unsafe {
        let ctx = hegel_context_new();
        let mut p: *mut HegelPrinter = ptr::null_mut();
        ok(hegel_printer_new(ctx, ptr::null(), &mut p));

        (*p).busy.store(true, Ordering::Release);
        assert_eq!(
            hegel_printer_text(ctx, p, "a".into()),
            HEGEL_E_CONCURRENT_USE
        );
        (*p).busy.store(false, Ordering::Release);

        ok(hegel_printer_text(ctx, p, "a".into()));
        ok(hegel_printer_free(ctx, p));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn last_error_view_preserves_interior_nuls() {
    let ctx = hegel_context_new();
    set_last_error(ctx, "before\0after");
    let message = unsafe { hegel_context_last_error(ctx) };
    assert_eq!(unsafe { *message.data.add(message.len) }, 0);
    assert_eq!(
        unsafe { std::slice::from_raw_parts(message.data.cast::<u8>(), message.len) },
        b"before\0after"
    );
    unsafe { ok(hegel_context_free(ctx)) };
}

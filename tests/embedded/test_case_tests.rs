use super::*;
use crate::ffi::{RunHandle, SettingsHandle};
use crate::generators as gs;
use crate::runner::{Mode, Settings};

/// Start a real engine run and hand back its first live test case wrapped in an
/// emitting `TestCase` (`emit = true`, the path a failing test's final replay
/// takes), keeping the owning [`RunHandle`] alive alongside it.
///
/// There is no `DataSource` trait to stub against any more — `TestCase` drives
/// libhegel through a concrete handle — so the unit tests below that only
/// exercise frontend bookkeeping (which never calls into the engine) use a
/// genuine, if unused, handle from a fresh run. The run is dropped at the end
/// of each test; `hegel_run_free` tolerates the un-marked in-flight case.
fn emitting_test_case() -> (RunHandle, TestCase) {
    let settings = Settings::new().database(None);
    let c_settings = SettingsHandle::build(&settings, None);
    let run = RunHandle::start(&c_settings, None).expect("the engine starts");
    let c_tc = run
        .next_test_case()
        .expect("the engine schedules at least one case");
    let tc = TestCase::new(Arc::new(c_tc), true, Mode::TestRun, current_output_sink());
    (run, tc)
}

#[test]
fn debug_is_non_exhaustive() {
    let (_run, tc) = emitting_test_case();
    assert_eq!(format!("{:?}", tc), "TestCase { .. }");
}

/// Cloning a `TestCase` hands back an independent handle onto the same test
/// case (`hegel_test_case_clone`), so a clone can be moved to another thread
/// and drawn from there while the original keeps drawing. The spawn/join gives
/// the happens-before that keeps this draw-at-a-time pattern well-defined.
#[test]
fn a_clone_can_draw_from_another_thread() {
    let (_run, tc) = emitting_test_case();
    let worker = tc.clone();
    std::thread::spawn(move || worker.draw(gs::integers::<i64>()))
        .join()
        .unwrap();
    tc.draw(gs::booleans());
}

#[test]
fn repeatable_display_name_skips_a_taken_name() {
    let (_run, tc) = emitting_test_case();
    tc.record_named_draw(&false, "x_1", false);
    tc.record_named_draw(&false, "x", true);
    tc.record_named_draw(&false, "x", true);

    let mut names: Vec<String> = tc
        .with_draw_state(|draw_state| draw_state.allocated_display_names.iter().cloned().collect());
    names.sort();
    assert_eq!(names, vec!["x_1", "x_2", "x_3"]);
}

/// Recover a panic payload's message as a `String`.
fn panic_payload_message(err: Box<dyn std::any::Any + Send>) -> String {
    err.downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

/// An invalid-argument (usage) error is raised carrying its diagnostic but no
/// internal marker. Outside a test context `raise_invalid_argument` panics
/// directly with the message; this is the primitive every `HEGEL_E_INVALID_ARG`
/// from the engine ultimately routes through ([`raise_for_rc`]).
#[test]
fn invalid_argument_error_is_raised_as_a_usage_error() {
    let err = std::panic::catch_unwind(|| {
        raise_invalid_argument(std::format_args!("bad generator configuration"))
    })
    .unwrap_err();
    let msg = panic_payload_message(err);
    assert!(msg.contains("bad generator configuration"), "{msg}");
    assert!(!msg.contains("__HEGEL"), "marker leaked: {msg}");
}

#[test]
fn stop_test_code_unwinds_as_stop_test() {
    let payload =
        std::panic::catch_unwind(|| raise_for_rc(hegel_c::hegel_result_t::HEGEL_E_STOP_TEST))
            .unwrap_err();
    assert!(
        payload.downcast_ref::<crate::control::StopTest>().is_some(),
        "expected a StopTest control unwind"
    );
}

#[test]
fn assume_code_unwinds_as_assume_failed() {
    let payload =
        std::panic::catch_unwind(|| raise_for_rc(hegel_c::hegel_result_t::HEGEL_E_ASSUME))
            .unwrap_err();
    assert!(
        payload
            .downcast_ref::<crate::control::AssumeFailed>()
            .is_some(),
        "expected an AssumeFailed control unwind"
    );
}

/// The typed uuid draw surfaces invalid arguments through
/// [`raise_for_rc`], like every other draw.
#[test]
fn uuid_invalid_version_is_raised_as_a_usage_error() {
    use std::panic::AssertUnwindSafe;
    let (_run, tc) = emitting_test_case();

    let err =
        std::panic::catch_unwind(AssertUnwindSafe(|| tc.generate_uuid(Some(16)))).unwrap_err();
    let msg = panic_payload_message(err);
    assert!(msg.contains("hex nibble"), "{msg}");
}

/// Draw integers straight off `tc` until the engine's choice budget is
/// exhausted, catching the resulting `StopTest` unwind so the underlying
/// handle is left aborted (every subsequent primitive then reports STOP_TEST).
fn drive_to_overrun(tc: &TestCase) {
    use std::panic::AssertUnwindSafe;
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..10_000_000 {
            let _: i64 = tc.draw(gs::integers::<i64>());
        }
    }));
    assert!(
        result.is_err(),
        "drawing should eventually overrun the budget"
    );
}

#[test]
fn span_calls_after_overrun_unwind_as_stop_test() {
    use std::panic::AssertUnwindSafe;
    let (_run, tc) = emitting_test_case();

    tc.start_span(gs::labels::LIST);
    drive_to_overrun(&tc);

    let payload = std::panic::catch_unwind(AssertUnwindSafe(|| tc.stop_span(false))).unwrap_err();
    assert!(
        payload.downcast_ref::<crate::control::StopTest>().is_some(),
        "stop_span after overrun should unwind as StopTest"
    );

    let payload =
        std::panic::catch_unwind(AssertUnwindSafe(|| tc.start_span(gs::labels::LIST))).unwrap_err();
    assert!(
        payload.downcast_ref::<crate::control::StopTest>().is_some(),
        "start_span after overrun should unwind as StopTest"
    );
}

#[test]
fn ip_draws_after_overrun_unwind_as_stop_test() {
    use std::panic::AssertUnwindSafe;
    let (_run, tc) = emitting_test_case();

    drive_to_overrun(&tc);

    let payload = std::panic::catch_unwind(AssertUnwindSafe(|| tc.generate_ipv4())).unwrap_err();
    assert!(
        payload.downcast_ref::<crate::control::StopTest>().is_some(),
        "generate_ipv4 after overrun should unwind as StopTest"
    );

    let payload = std::panic::catch_unwind(AssertUnwindSafe(|| tc.generate_ipv6())).unwrap_err();
    assert!(
        payload.downcast_ref::<crate::control::StopTest>().is_some(),
        "generate_ipv6 after overrun should unwind as StopTest"
    );
}

#[test]
fn unexpected_code_unwinds_as_an_internal_error() {
    let payload =
        std::panic::catch_unwind(|| raise_for_rc(hegel_c::hegel_result_t::HEGEL_E_BACKEND))
            .unwrap_err();
    let msg = panic_payload_message(payload);
    assert!(msg.contains("unexpected code -3"), "{msg}");
    assert!(msg.contains("Internal error in hegel"), "{msg}");
}

/// Once a collection has answered `false`, further `more()` calls keep
/// answering `false` without touching the engine.
#[test]
fn collection_more_after_finished_stays_false() {
    let (_run, tc) = emitting_test_case();
    let mut collection = Collection::new(&tc, 0, Some(3));
    while collection.more() {
        tc.draw(gs::booleans());
    }
    assert!(!collection.more());
}

/// `reject()` drops the last element from a live collection's size budget,
/// and is a no-op once the collection has finished.
#[test]
fn collection_reject_live_and_after_finished() {
    let (_run, tc) = emitting_test_case();
    let mut collection = Collection::new(&tc, 1, Some(3));
    assert!(
        collection.more(),
        "min_size 1 guarantees a first element before any rejection"
    );
    tc.draw(gs::booleans());
    collection.reject(Some("rejected by the test"));
    while collection.more() {
        tc.draw(gs::booleans());
    }
    collection.reject(Some("after finished"));
    assert!(!collection.more());
}

#[test]
fn with_output_override_restores_the_sink_on_panic() {
    let sink: OutputSink = std::sync::Arc::new(|_line: &str| {});
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        with_output_override(sink, || panic!("boom"));
    }));
    assert!(result.is_err());
    assert!(
        current_output_sink().is_none(),
        "a panicking capture closure must not leave its sink installed"
    );
}

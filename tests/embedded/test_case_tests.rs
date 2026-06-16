use super::*;
use crate::ffi::{RunHandle, SettingsHandle};
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
    let run = RunHandle::start(&c_settings).expect("the engine starts");
    let c_tc = run
        .next_test_case()
        .expect("the engine schedules at least one case");
    let tc = TestCase::new(c_tc, true, Mode::TestRun);
    (run, tc)
}

#[test]
fn debug_is_non_exhaustive() {
    let (_run, tc) = emitting_test_case();
    assert_eq!(format!("{:?}", tc), "TestCase { .. }");
}

#[test]
fn repeatable_display_name_skips_a_taken_name() {
    let (_run, tc) = emitting_test_case();
    // A non-repeatable draw named "x_1" claims the display name "x_1".
    tc.record_named_draw(&false, "x_1", false);
    // Two repeatable draws named "x" want "x_1" then "x_2"; the first collides
    // with the explicit "x_1" above and must advance the counter, so they end
    // up as "x_2" and "x_3".
    tc.record_named_draw(&false, "x", true);
    tc.record_named_draw(&false, "x", true);

    let mut names: Vec<String> = tc.with_shared(|shared| {
        shared
            .draw_state
            .allocated_display_names
            .iter()
            .cloned()
            .collect()
    });
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

// ── error-code translation ───────────────────────────────────────────────
//
// `raise_for_rc` maps libhegel's `c_int` return codes to control-flow
// unwinds. The per-primitive `TestCase` methods (start_span, stop_span,
// generate, the pool/collection calls) are thin wrappers that call it on any
// non-OK code, so a span call after the engine runs out of data concludes the
// case as `Overrun` rather than corrupting the span structure.

#[test]
fn stop_test_code_unwinds_as_stop_test() {
    let payload =
        std::panic::catch_unwind(|| raise_for_rc(hegel_c::HEGEL_E_STOP_TEST)).unwrap_err();
    assert!(
        payload.downcast_ref::<crate::control::StopTest>().is_some(),
        "expected a StopTest control unwind"
    );
}

#[test]
fn assume_code_unwinds_as_assume_failed() {
    let payload = std::panic::catch_unwind(|| raise_for_rc(hegel_c::HEGEL_E_ASSUME)).unwrap_err();
    assert!(
        payload
            .downcast_ref::<crate::control::AssumeFailed>()
            .is_some(),
        "expected an AssumeFailed control unwind"
    );
}

/// Draw integers straight off `tc` until the engine's choice budget is
/// exhausted, catching the resulting `StopTest` unwind so the underlying
/// handle is left aborted (every subsequent primitive then reports STOP_TEST).
fn drive_to_overrun(tc: &TestCase) {
    use std::panic::AssertUnwindSafe;
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..10_000_000 {
            let _: i64 = tc.draw(crate::generators::integers::<i64>());
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

    // Open a span up front (succeeds while the budget is intact), then
    // exhaust the budget.
    tc.start_span(crate::generators::labels::LIST);
    drive_to_overrun(&tc);

    // stop_span on the aborted case: it still rolls back the span-depth
    // bookkeeping (asserting depth > 0 first), then unwinds as StopTest.
    let payload = std::panic::catch_unwind(AssertUnwindSafe(|| tc.stop_span(false))).unwrap_err();
    assert!(
        payload.downcast_ref::<crate::control::StopTest>().is_some(),
        "stop_span after overrun should unwind as StopTest"
    );

    // start_span on the aborted case: the depth bump is rolled back before the
    // StopTest unwind, so the bookkeeping stays balanced.
    let payload = std::panic::catch_unwind(AssertUnwindSafe(|| {
        tc.start_span(crate::generators::labels::LIST)
    }))
    .unwrap_err();
    assert!(
        payload.downcast_ref::<crate::control::StopTest>().is_some(),
        "start_span after overrun should unwind as StopTest"
    );
}

#[test]
fn unexpected_code_unwinds_as_an_internal_error() {
    // Any libhegel return code the frontend doesn't model is a framework
    // invariant violation, surfaced as an internal error (not a shrinkable
    // failure). 4242 is a code the engine never returns.
    let payload = std::panic::catch_unwind(|| raise_for_rc(4242)).unwrap_err();
    let msg = panic_payload_message(payload);
    assert!(msg.contains("unexpected code 4242"), "{msg}");
    assert!(msg.contains("Internal error in hegel"), "{msg}");
}

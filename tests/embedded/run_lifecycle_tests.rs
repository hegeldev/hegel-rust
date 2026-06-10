//! Embedded tests for `src/run_lifecycle.rs`.

use super::*;
use crate::backend::DataSourceError;
use ciborium::Value;

// `panic_message` downcasts the panic payload to `&str` or `String`; for
// any other type it falls through to the `"Unknown panic"` branch.  Real
// `panic!("...")` and `assert!(false, "...")` produce string payloads, so
// production paths rarely hit the fallback — but `std::panic::panic_any`
// with a non-string payload does.

#[test]
fn panic_message_returns_unknown_panic_for_non_string_payload() {
    let result = std::panic::catch_unwind(|| {
        std::panic::panic_any(42i32);
    });
    let payload = result.unwrap_err();
    assert_eq!(panic_message(&payload), "Unknown panic");
}

// `filter_short_backtrace` renumbers frames whose trimmed form matches
// `digit … : …` (the standard `Backtrace` frame line shape).  Anomalous
// frame lines that start with a digit but contain no colon — they exist
// in non-standard backtrace formats and as continuation lines from some
// runtimes — fall to a "preserve as-is" branch.  All other lines
// (non-digit-leading) go through the analogous preserve-as-is branch.

#[test]
fn filter_short_backtrace_preserves_digit_lines_without_colons() {
    // No __rust_end_short_backtrace / __rust_begin_short_backtrace markers,
    // so the whole input is filtered. The middle line starts with a digit
    // but has no colon → line 167 preserve-as-is branch fires. The first
    // and third lines exercise the `digit:` renumber branch (line 162) and
    // we verify their numbers were rewritten.
    let input = "  5: first_frame at /tmp/a.rs:1\n\
                 10 anomalous_no_colon line\n\
                 6: third_frame at /tmp/b.rs:2";
    let out = filter_short_backtrace(input);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3);
    // First line renumbered to frame 0.
    assert!(
        lines[0].starts_with("   0: first_frame"),
        "expected renumbered first frame, got {:?}",
        lines[0]
    );
    // Middle line preserved verbatim — line 167 branch.
    assert_eq!(lines[1], "10 anomalous_no_colon line");
    // Third line renumbered to frame 1.
    assert!(
        lines[2].starts_with("   1: third_frame"),
        "expected renumbered third frame, got {:?}",
        lines[2]
    );
}

// `unknown_panic_info` returns the placeholder quadruple used when the
// cross-backend panic hook didn't capture info for a panic (e.g.
// `init_panic_hook` wasn't called yet).  The production path always
// calls `init_panic_hook` before `run_test_case`, so this fallback is
// only reached via direct test entry — testing the placeholder shape
// here avoids needing a contrived hook-bypass setup at `run_test_case`.

#[test]
fn unknown_panic_info_returns_unknown_placeholders() {
    let (thread_name, thread_id, location, backtrace) = unknown_panic_info();
    assert_eq!(thread_name, "<unknown>");
    assert_eq!(thread_id, "?");
    assert_eq!(location, "<unknown>");
    assert_eq!(
        backtrace.status(),
        std::backtrace::BacktraceStatus::Disabled
    );
}

#[test]
fn filter_short_backtrace_preserves_non_digit_leading_lines() {
    // The non-digit-leading branch (line 170) for a header line.
    let input = "stack backtrace:\n  0: real_frame at /tmp/x.rs:5";
    let out = filter_short_backtrace(input);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "stack backtrace:");
    assert!(lines[1].starts_with("   0: real_frame"));
}

// ── run_lifecycle: filter_short_backtrace marker handling ────────────────
//
// `filter_short_backtrace` trims the captured backtrace down to the
// frames between `__rust_end_short_backtrace` and
// `__rust_begin_short_backtrace`, matching the default Rust panic
// handler's "short" output.  Real backtraces only contain those markers
// when captured during a real panic; the embedded tests below feed
// hand-crafted strings to exercise both ends of the trim.

#[test]
fn filter_short_backtrace_trims_at_end_marker() {
    let input = "  0: noisy_frame at /tmp/a.rs:1\n\
                 stuff: __rust_end_short_backtrace\n  \
                 1: real_frame at /tmp/b.rs:5\n  \
                 2: another_frame at /tmp/c.rs:7";
    let out = filter_short_backtrace(input);
    // The first "noisy" frame is gone; renumbering starts at 0 for the
    // first frame after the end marker.
    assert!(
        !out.contains("noisy_frame"),
        "expected noisy_frame to be trimmed, got {:?}",
        out
    );
    assert!(
        out.contains("   0: real_frame"),
        "expected renumbered real_frame, got {:?}",
        out
    );
    assert!(
        out.contains("   1: another_frame"),
        "expected renumbered another_frame, got {:?}",
        out
    );
}

#[test]
fn filter_short_backtrace_trims_at_begin_marker() {
    let input = "  0: real_frame at /tmp/a.rs:1\n  \
                 1: trailing_frame at /tmp/b.rs:5\n\
                 stuff: __rust_begin_short_backtrace\n  \
                 2: noisy_frame at /tmp/c.rs:7";
    let out = filter_short_backtrace(input);
    assert!(
        out.contains("   0: real_frame"),
        "expected renumbered real_frame, got {:?}",
        out
    );
    assert!(
        !out.contains("noisy_frame"),
        "expected noisy_frame after begin marker to be trimmed, got {:?}",
        out
    );
}

// ── run_lifecycle: format_backtrace forwards to filter_short_backtrace ───
//
// `format_backtrace(bt, true)` returns the raw `Display` of the backtrace
// verbatim; `format_backtrace(bt, false)` runs it through the short-form
// trimmer.  `Backtrace::disabled()` Displays as `disabled backtrace`, so
// the test below distinguishes the branches by the formatted output
// shape rather than the (empty) trim result.

#[test]
fn format_backtrace_full_returns_display_verbatim() {
    let bt = std::backtrace::Backtrace::disabled();
    let out = format_backtrace(&bt, true);
    assert_eq!(out, format!("{}", bt));
}

#[test]
fn format_backtrace_short_strips_through_filter() {
    let bt = std::backtrace::Backtrace::disabled();
    // `filter_short_backtrace` on the disabled-Display string is a
    // no-op (no markers, no digit lines), so we get back what we put in.
    let out = format_backtrace(&bt, false);
    assert_eq!(out, filter_short_backtrace(&format!("{}", bt)));
}

// ── run_lifecycle: reproducer_line ───────────────────────────────────────
//
// `reproducer_line` decides the copy-pasteable
// `#[hegel::reproduce_failure("…")]` line printed after a failure's
// diagnostic. It is `Some` only when `print_blob` is enabled *and* the
// failure carries a blob; `None` (suppressed / no blob) prints nothing. The formatting embeds the blob verbatim.

fn failure_with_blob(blob: Option<&str>) -> crate::backend::Failure {
    crate::backend::Failure {
        panic_message: "boom".to_string(),
        diagnostic: "boom\n".to_string(),
        origin: "Panic at x".to_string(),
        reproduce_blob: blob.map(str::to_string),
    }
}

#[test]
fn reproducer_line_none_when_print_blob_disabled() {
    let settings = crate::runner::Settings::new();
    assert!(!settings.print_blob);
    assert!(reproducer_line(&settings, &failure_with_blob(Some("AAEC"))).is_none());
}

#[test]
fn reproducer_line_none_when_no_blob_attached() {
    // The health-check / server-backend case: print_blob on, but no blob.
    let settings = crate::runner::Settings::new().print_blob(true);
    assert!(reproducer_line(&settings, &failure_with_blob(None)).is_none());
}

#[test]
fn reproducer_line_emits_attribute_when_enabled_and_present() {
    let settings = crate::runner::Settings::new().print_blob(true);
    let line = reproducer_line(&settings, &failure_with_blob(Some("AAEC"))).unwrap();
    assert!(
        line.contains("#[hegel::reproduce_failure(\"AAEC\")]"),
        "expected the reproducer attribute, got: {line}"
    );
}

// ── run_lifecycle: drive prints reproducer lines on failure ──────────────
//
// A stub runner returns a pre-built failing `TestRunResult` (without touching
// the `run_case` callback), so `drive`'s single- and multi-failure output
// paths can be exercised — including the per-failure reproducer line — and
// the distinct panic messages asserted.

struct StubRunner {
    failures: Vec<crate::backend::Failure>,
}

impl crate::backend::TestRunner for StubRunner {
    fn run(
        &self,
        _settings: &crate::runner::Settings,
        _database_key: Option<&str>,
        _run_case: &mut dyn FnMut(Box<dyn crate::backend::DataSource + Send + Sync>, bool),
    ) -> crate::backend::TestRunResult {
        crate::backend::TestRunResult {
            passed: false,
            failures: self.failures.clone(),
        }
    }
}

fn drive_panic_message(failures: Vec<crate::backend::Failure>) -> String {
    let settings = crate::runner::Settings::new().print_blob(true);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drive(
            StubRunner { failures },
            |_tc: TestCase| {},
            &settings,
            None,
            None,
        );
    }));
    let payload = result.expect_err("drive should panic on a failing run");
    payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("")
        .to_string()
}

#[test]
fn drive_single_failure_prints_reproducer_and_panics() {
    let msg = drive_panic_message(vec![failure_with_blob(Some("AAEC"))]);
    assert!(
        msg.contains("Property test failed: boom"),
        "unexpected panic message: {msg}"
    );
}

#[test]
fn drive_multiple_failures_prints_each_reproducer_and_panics() {
    let msg = drive_panic_message(vec![
        failure_with_blob(Some("AAEC")),
        failure_with_blob(Some("BBBB")),
    ]);
    assert!(
        msg.contains("2 distinct failures"),
        "unexpected panic message: {msg}"
    );
}

// ── run_lifecycle: backtrace capture is gated to where it is shown ───────
//
// The panic hook captures a backtrace for every panic raised in a test
// context, but the only backtraces that are ever shown belong to failures
// whose diagnostic is emitted — the final replay (`is_final`) or, in
// verbose mode, every interesting case. Capturing (and, under
// `RUST_BACKTRACE`, symbolizing) one for each discarded shrink probe is the
// dominant cost of failing-heavy property runs. These tests pin the gating:
// they are meaningful when the process has backtraces enabled (e.g. CI runs
// with `RUST_BACKTRACE=1`) and harmless otherwise.

/// A no-op `DataSource` for driving `run_test_case` with a body that panics
/// before it draws. Only `mark_complete` is reached.
struct BtStubDataSource;

impl crate::backend::DataSource for BtStubDataSource {
    fn generate(&self, _schema: &Value) -> Result<Value, DataSourceError> {
        unimplemented!()
    }
    fn start_span(&self, _label: u64) -> Result<(), DataSourceError> {
        unimplemented!()
    }
    fn stop_span(&self, _discard: bool) -> Result<(), DataSourceError> {
        unimplemented!()
    }
    fn new_collection(&self, _min: u64, _max: Option<u64>) -> Result<i64, DataSourceError> {
        unimplemented!()
    }
    fn collection_more(&self, _id: i64) -> Result<bool, DataSourceError> {
        unimplemented!()
    }
    fn collection_reject(&self, _id: i64, _why: Option<&str>) -> Result<(), DataSourceError> {
        unimplemented!()
    }
    fn primitive_boolean(&self, _p: f64, _forced: Option<bool>) -> Result<bool, DataSourceError> {
        unimplemented!()
    }
    fn new_pool(&self) -> Result<i64, DataSourceError> {
        unimplemented!()
    }
    fn pool_add(&self, _pool_id: i64) -> Result<i64, DataSourceError> {
        unimplemented!()
    }
    fn pool_generate(&self, _pool_id: i64, _consume: bool) -> Result<i64, DataSourceError> {
        unimplemented!()
    }
    fn target_observation(&self, _score: f64, _label: &str) {
        unimplemented!()
    }
    fn mark_complete(&self, _result: &TestCaseResult) {}
}

fn run_case_capturing(
    is_final: bool,
    verbosity: crate::runner::Verbosity,
    body: &mut dyn FnMut(TestCase),
) -> TestCaseResult {
    init_panic_hook();
    run_test_case(
        Box::new(BtStubDataSource),
        body,
        is_final,
        crate::runner::Mode::TestRun,
        verbosity,
    )
}

fn interesting_diagnostic(result: &TestCaseResult) -> String {
    match result {
        TestCaseResult::Interesting(failure) => failure.diagnostic.clone(),
        other => panic!("expected an Interesting result, got {other:?}"),
    }
}

fn backtraces_enabled() -> bool {
    matches!(
        Backtrace::capture().status(),
        std::backtrace::BacktraceStatus::Captured
    )
}

#[test]
fn discarded_failures_skip_backtrace_capture() {
    // Non-final, non-verbose: `should_emit` is false, so the diagnostic is
    // thrown away — no backtrace should be captured, even with backtraces
    // enabled. This is the shrinker hot path.
    let result = run_case_capturing(false, crate::runner::Verbosity::Normal, &mut |_tc| {
        panic!("{}", "boom")
    });
    let diagnostic = interesting_diagnostic(&result);
    assert!(
        !diagnostic.contains("stack backtrace"),
        "a discarded (non-final) failure must not capture a backtrace; got:\n{diagnostic}"
    );
}

#[test]
fn shown_failures_capture_backtrace_when_enabled() {
    // Final replay: `should_emit` is true, so the diagnostic is shown and
    // should carry a backtrace exactly when the process has them enabled.
    let result = run_case_capturing(true, crate::runner::Verbosity::Normal, &mut |_tc| {
        panic!("{}", "boom")
    });
    let diagnostic = interesting_diagnostic(&result);
    assert_eq!(
        diagnostic.contains("stack backtrace"),
        backtraces_enabled(),
        "a shown (final) failure should carry a backtrace exactly when enabled; got:\n{diagnostic}"
    );
}

#[test]
fn verbose_mode_captures_backtrace_for_non_final_failures() {
    // Verbose mode emits every interesting case's diagnostic live, so
    // `should_emit` is true even when not final — the backtrace must be
    // captured (when enabled) so the live output matches a real failure.
    let result = run_case_capturing(false, crate::runner::Verbosity::Verbose, &mut |_tc| {
        panic!("{}", "boom")
    });
    let diagnostic = interesting_diagnostic(&result);
    assert_eq!(
        diagnostic.contains("stack backtrace"),
        backtraces_enabled(),
        "verbose mode should capture a backtrace for non-final failures; got:\n{diagnostic}"
    );
}

#[test]
fn control_flow_panics_never_capture_a_backtrace() {
    use std::backtrace::BacktraceStatus;
    // An assume-style control panic is classified as `Invalid` and its
    // captured info is discarded — so it must never pay to capture a
    // backtrace, even on the final replay where `should_emit` is true.
    init_panic_hook();
    let result = run_test_case(
        Box::new(BtStubDataSource),
        &mut |_tc| std::panic::panic_any(crate::test_case::ASSUME_FAIL_STRING),
        true,
        crate::runner::Mode::TestRun,
        crate::runner::Verbosity::Normal,
    );
    assert!(matches!(result, TestCaseResult::Invalid));
    // The hook recorded the control panic but the `Invalid` branch left the
    // info unconsumed; confirm no backtrace was captured for it.
    let (_, _, _, backtrace) = take_panic_info().expect("hook recorded the control panic");
    assert_eq!(
        backtrace.status(),
        BacktraceStatus::Disabled,
        "control-flow panics must not capture a backtrace"
    );
}

// `drive` runs the runner unconditionally — phase selection (skipping a
// run that lacks `Phase::Generate`) is the caller's concern: `Hegel::run`
// performs that check before calling `drive`, so phase-agnostic callers
// like the blob-replay path are not gated here.

#[test]
fn drive_runs_the_runner_regardless_of_phases() {
    use std::cell::Cell;
    use std::rc::Rc;

    struct RecordingRunner(Rc<Cell<bool>>);
    impl crate::backend::TestRunner for RecordingRunner {
        fn run(
            &self,
            _settings: &crate::runner::Settings,
            _database_key: Option<&str>,
            _run_case: &mut dyn FnMut(Box<dyn crate::backend::DataSource + Send + Sync>, bool),
        ) -> crate::backend::TestRunResult {
            self.0.set(true);
            crate::backend::TestRunResult {
                passed: true,
                failures: vec![],
            }
        }
    }

    let ran = Rc::new(Cell::new(false));
    let settings = crate::runner::Settings::new().phases([]); // no Phase::Generate
    drive(
        RecordingRunner(ran.clone()),
        |_tc: TestCase| {},
        &settings,
        None,
        None,
    );
    assert!(
        ran.get(),
        "drive must run the runner regardless of the phase selection"
    );
}

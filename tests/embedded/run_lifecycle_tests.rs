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
    // A blobless failure (e.g. `Mode::SingleTestCase`): print_blob on, but
    // nothing to print.
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
// A stub runner whose exploration yields one pre-built `Failure` per
// counterexample and whose `replay_final` hands it straight back, so
// `drive`'s single- and multi-failure replay-and-report loop can be
// exercised — including the per-failure reproducer line — and the distinct
// panic messages asserted, without a live engine.

struct StubRunner {
    failures: Vec<crate::backend::Failure>,
}

impl crate::backend::TestRunner for StubRunner {
    type Counterexample = crate::backend::Failure;

    fn explore(
        &self,
        _settings: &crate::runner::Settings,
        _database_key: Option<&str>,
        _run_case: &mut dyn FnMut(Box<dyn crate::backend::DataSource + Send + Sync>),
    ) -> Result<crate::backend::Exploration<crate::backend::Failure>, crate::backend::RunError>
    {
        Ok(crate::backend::Exploration::Counterexamples(
            self.failures.clone(),
        ))
    }

    fn replay_final(
        &self,
        counterexample: crate::backend::Failure,
        _run_case: &mut dyn FnMut(Box<dyn crate::backend::DataSource + Send + Sync>),
    ) -> Result<crate::backend::Failure, crate::backend::RunError> {
        Ok(counterexample)
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

// ── run_lifecycle: drive's defensive unknown panic ───────────────────────
//
// An exploration that reports counterexamples but lists none of them (no
// real runner produces this) must still fail the run, with the legacy
// generic message, rather than report a count of zero or pass.

#[test]
fn drive_panics_with_unknown_for_an_empty_counterexample_list() {
    init_panic_hook();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drive(
            StubRunner { failures: vec![] },
            |_tc: TestCase| {},
            &crate::runner::Settings::new(),
            None,
            None,
        );
    }));
    let payload = result.expect_err("drive should panic on an empty counterexample list");
    let msg = payload.downcast_ref::<&str>().copied().unwrap_or("");
    assert_eq!(msg, "Property test failed: unknown");
}

// ── run_lifecycle: drive surfaces a RunError from the final replay ───────
//
// When `replay_final` errors — the bug fired during exploration but not on
// its final replay — `drive` must panic with the error's own message (no
// `Property test failed:` framing: it's a failure of the run, not of a
// test case). The engine equivalents are a flaky test (native) and a stale
// blob (reproduce); the stub keeps the path deterministic.

struct VanishingRunner;

impl crate::backend::TestRunner for VanishingRunner {
    type Counterexample = ();

    fn explore(
        &self,
        _settings: &crate::runner::Settings,
        _database_key: Option<&str>,
        _run_case: &mut dyn FnMut(Box<dyn crate::backend::DataSource + Send + Sync>),
    ) -> Result<crate::backend::Exploration<()>, crate::backend::RunError> {
        Ok(crate::backend::Exploration::Counterexamples(vec![()]))
    }

    fn replay_final(
        &self,
        _counterexample: (),
        _run_case: &mut dyn FnMut(Box<dyn crate::backend::DataSource + Send + Sync>),
    ) -> Result<crate::backend::Failure, crate::backend::RunError> {
        Err(crate::backend::RunError::Flaky(
            "the bug went away".to_string(),
        ))
    }
}

#[test]
fn drive_panics_with_the_run_error_when_a_replay_stops_failing() {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drive(
            VanishingRunner,
            |_tc: TestCase| {},
            &crate::runner::Settings::new(),
            None,
            None,
        );
    }));
    let payload = result.expect_err("drive should panic on a vanished counterexample");
    let msg = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or("");
    assert_eq!(msg, "the bug went away");
}

// ── run_lifecycle: drive surfaces a RunError from exploration ────────────
//
// A run error during exploration (health check, nondeterminism) must panic
// with the error's own message, before any replay or report machinery runs.

struct ErroringRunner;

impl crate::backend::TestRunner for ErroringRunner {
    type Counterexample = ();

    fn explore(
        &self,
        _settings: &crate::runner::Settings,
        _database_key: Option<&str>,
        _run_case: &mut dyn FnMut(Box<dyn crate::backend::DataSource + Send + Sync>),
    ) -> Result<crate::backend::Exploration<()>, crate::backend::RunError> {
        Err(crate::backend::RunError::HealthCheck(
            "FailedHealthCheck: the run went wrong".to_string(),
        ))
    }

    fn replay_final(
        &self,
        _counterexample: (),
        _run_case: &mut dyn FnMut(Box<dyn crate::backend::DataSource + Send + Sync>),
    ) -> Result<crate::backend::Failure, crate::backend::RunError> {
        unreachable!("an erroring exploration has nothing to replay")
    }
}

#[test]
fn drive_panics_with_the_run_error_when_exploration_errors() {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drive(
            ErroringRunner,
            |_tc: TestCase| {},
            &crate::runner::Settings::new(),
            None,
            None,
        );
    }));
    let payload = result.expect_err("drive should panic on an exploration error");
    let msg = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or("");
    assert_eq!(msg, "FailedHealthCheck: the run went wrong");
}

// ── run_lifecycle: backtrace capture is gated to where it is shown ───────
//
// The only backtraces ever shown belong to failures whose diagnostic is
// emitted — a non-quiet final replay or, in verbose mode, every interesting
// case. Capturing (and, under `RUST_BACKTRACE`, symbolizing) one for each
// discarded shrink probe is the dominant cost of failing-heavy property
// runs. The gate has two halves, each pinned separately below:
// `run_test_case` sets `CAPTURE_BACKTRACE` to `should_emit`, and the panic
// hook captures a backtrace only when the flag is set.

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
    let (result, _payload) = run_test_case(
        Box::new(BtStubDataSource),
        body,
        is_final,
        crate::runner::Mode::TestRun,
        verbosity,
    );
    result
}

fn backtraces_enabled() -> bool {
    matches!(
        Backtrace::capture().status(),
        std::backtrace::BacktraceStatus::Captured
    )
}

/// The `CAPTURE_BACKTRACE` flag left behind by the last `run_test_case` on
/// this thread — i.e. what the gate decided for that test case.
fn capture_flag() -> bool {
    CAPTURE_BACKTRACE.get()
}

#[test]
fn discarded_failures_skip_backtrace_capture() {
    // Non-final, non-verbose: `should_emit` is false, the diagnostic will
    // never be shown — the gate must tell the hook not to pay for a
    // backtrace. This is the shrinker hot path.
    run_case_capturing(false, crate::runner::Verbosity::Normal, &mut |_tc| {
        panic!("{}", "boom")
    });
    assert!(
        !capture_flag(),
        "a discarded (non-final) failure must not pay for a backtrace"
    );
}

#[test]
fn shown_failures_enable_backtrace_capture() {
    // Final replay: `should_emit` is true, the diagnostic is shown, so the
    // gate must enable capture.
    run_case_capturing(true, crate::runner::Verbosity::Normal, &mut |_tc| {
        panic!("{}", "boom")
    });
    assert!(capture_flag());
}

#[test]
fn quiet_final_replay_skips_backtrace_capture() {
    // Quiet suppresses the final replay's diagnostic, so there is nothing
    // to capture a backtrace for.
    run_case_capturing(true, crate::runner::Verbosity::Quiet, &mut |_tc| {
        panic!("{}", "boom")
    });
    assert!(!capture_flag());
}

#[test]
fn verbose_mode_enables_backtrace_capture_for_non_final_failures() {
    // Verbose mode emits every interesting case's diagnostic live, so
    // `should_emit` is true even when not final.
    run_case_capturing(false, crate::runner::Verbosity::Verbose, &mut |_tc| {
        panic!("{}", "boom")
    });
    assert!(capture_flag());
}

#[test]
fn hook_captures_backtrace_only_when_flagged() {
    use std::backtrace::BacktraceStatus;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    init_panic_hook();
    take_panic_info();

    CAPTURE_BACKTRACE.set(false);
    let _ = crate::control::with_test_context(|| {
        catch_unwind(AssertUnwindSafe(|| panic!("{}", "unflagged")))
    });
    let (_, _, _, backtrace) = take_panic_info().unwrap();
    assert_eq!(
        backtrace.status(),
        BacktraceStatus::Disabled,
        "the hook must not capture a backtrace when the flag is clear"
    );

    CAPTURE_BACKTRACE.set(true);
    let _ = crate::control::with_test_context(|| {
        catch_unwind(AssertUnwindSafe(|| panic!("{}", "flagged")))
    });
    let (_, _, _, backtrace) = take_panic_info().unwrap();
    assert_eq!(
        backtrace.status() == BacktraceStatus::Captured,
        backtraces_enabled(),
        "the hook should capture a backtrace exactly when flagged and enabled"
    );
}

#[test]
fn control_flow_unwinds_never_reach_the_panic_hook() {
    // A rejected assumption unwinds via `resume_unwind`, which skips panic
    // hooks entirely — so even on the final replay (where the hook would
    // pay for a backtrace) the hook must record nothing at all.
    init_panic_hook();
    take_panic_info();
    let (result, payload) = run_test_case(
        Box::new(BtStubDataSource),
        &mut |_tc| crate::control::raise_control(crate::control::AssumeFailed),
        true,
        crate::runner::Mode::TestRun,
        crate::runner::Verbosity::Normal,
    );
    assert!(matches!(result, TestCaseResult::Invalid));
    assert!(payload.is_none(), "control flow carries no failure payload");
    assert!(
        take_panic_info().is_none(),
        "a control-flow unwind must not touch the panic hook's state"
    );
}

// ── run_lifecycle: where diagnostics and replay output land ──────────────
//
// The final replay's draw/note lines are emitted live (through the
// installed sink, when there is one) as the test body runs; the diagnostic
// prints to stderr at the catch site, never into the sink — snapshot tests
// capture draw/note lines without nondeterministic panic locations. In
// verbose mode, a non-final case's diagnostic goes through the sink so
// in-process tests can observe it.

#[test]
fn verbose_non_final_diagnostic_flows_through_the_sink_without_duplicating_notes() {
    use std::sync::{Arc, Mutex};

    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = lines.clone();
    let sink: crate::test_case::OutputSink =
        Arc::new(move |s: &str| writer.lock().unwrap().push(s.to_string()));

    crate::test_case::with_output_override(sink, || {
        run_case_capturing(false, crate::runner::Verbosity::Verbose, &mut |tc| {
            tc.note("the noted line");
            panic!("{}", "boom");
        })
    });

    let lines = lines.lock().unwrap();
    assert_eq!(
        lines.iter().filter(|l| *l == "the noted line").count(),
        1,
        "the live note must appear exactly once (not re-embedded in the \
         diagnostic), got {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("panicked at")),
        "the non-final diagnostic must flow through the sink in verbose \
         mode, got {lines:?}"
    );
    assert_eq!(
        lines.iter().any(|l| l.contains("stack backtrace")),
        backtraces_enabled(),
        "the verbose diagnostic should carry a backtrace exactly when \
         enabled, got {lines:?}"
    );
}

#[test]
fn final_replay_sink_receives_output_but_not_the_diagnostic() {
    use std::sync::{Arc, Mutex};

    // Snapshot tests capture the final replay's draw/note lines through
    // `with_output_override`; they must keep flowing to that sink as the
    // body runs — and the diagnostic must stay out of the sink (it prints
    // to stderr), or snapshots would gain nondeterministic panic locations.
    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = lines.clone();
    let sink: crate::test_case::OutputSink =
        Arc::new(move |s: &str| writer.lock().unwrap().push(s.to_string()));

    crate::test_case::with_output_override(sink, || {
        run_case_capturing(true, crate::runner::Verbosity::Normal, &mut |tc| {
            tc.note("the noted line");
            panic!("{}", "boom");
        })
    });

    assert_eq!(lines.lock().unwrap().as_slice(), ["the noted line"]);
}

#[test]
fn quiet_final_replay_emits_no_draw_or_note_output() {
    use std::sync::{Arc, Mutex};

    // `Verbosity::Quiet` suppresses even the final replay's draw/note
    // lines — the sink stays empty.
    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = lines.clone();
    let sink: crate::test_case::OutputSink =
        Arc::new(move |s: &str| writer.lock().unwrap().push(s.to_string()));

    crate::test_case::with_output_override(sink, || {
        run_case_capturing(true, crate::runner::Verbosity::Quiet, &mut |tc| {
            tc.note("the noted line");
            panic!("{}", "boom");
        })
    });

    assert!(
        lines.lock().unwrap().is_empty(),
        "quiet mode must suppress final-replay output, got {:?}",
        lines.lock().unwrap()
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
        type Counterexample = ();

        fn explore(
            &self,
            _settings: &crate::runner::Settings,
            _database_key: Option<&str>,
            _run_case: &mut dyn FnMut(Box<dyn crate::backend::DataSource + Send + Sync>),
        ) -> Result<crate::backend::Exploration<()>, crate::backend::RunError> {
            self.0.set(true);
            Ok(crate::backend::Exploration::Passed)
        }

        fn replay_final(
            &self,
            _counterexample: (),
            _run_case: &mut dyn FnMut(Box<dyn crate::backend::DataSource + Send + Sync>),
        ) -> Result<crate::backend::Failure, crate::backend::RunError> {
            unreachable!("a passing exploration has nothing to replay")
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

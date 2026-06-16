//! Embedded tests for `src/run_lifecycle.rs`.

use super::*;
use crate::generators::{booleans, integers};

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
// diagnostic. It is `Some` only when `print_blob` is enabled *and* a blob is
// present; `None` (suppressed / no blob) prints nothing.

#[test]
fn reproducer_line_none_when_print_blob_disabled() {
    let settings = Settings::new();
    assert!(!settings.print_blob);
    assert!(reproducer_line(&settings, Some("AAEC")).is_none());
}

#[test]
fn reproducer_line_none_when_no_blob_attached() {
    // A blobless failure (e.g. `Mode::SingleTestCase`): print_blob on, but
    // nothing to print.
    let settings = Settings::new().print_blob(true);
    assert!(reproducer_line(&settings, None).is_none());
}

#[test]
fn reproducer_line_emits_attribute_when_enabled_and_present() {
    let settings = Settings::new().print_blob(true);
    let line = reproducer_line(&settings, Some("AAEC")).unwrap();
    assert!(
        line.contains("#[hegel::reproduce_failure(\"AAEC\")]"),
        "expected the reproducer attribute, got: {line}"
    );
}

// ── real-engine helpers ──────────────────────────────────────────────────
//
// There is no `TestRunner` / `DataSource` trait to mock any more — `drive`
// and `run_test_case` go through libhegel's C ABI — so these tests drive a
// real engine and shape the outcome from the test body.

fn test_settings() -> Settings {
    Settings::new()
        .database(None)
        .derandomize(true)
        .seed(Some(1))
}

/// Run one real test case from a fresh engine run through `run_test_case`,
/// passing `is_final` / `verbosity` straight through to exercise its emit and
/// backtrace gating. The body decides the outcome; the engine supplies a real,
/// working handle (so `mark_complete` and any draws are real).
fn run_one_case(
    is_final: bool,
    verbosity: Verbosity,
    body: &mut dyn FnMut(TestCase),
) -> (
    TestCaseResult,
    Option<Box<dyn std::any::Any + Send>>,
    Option<String>,
) {
    init_panic_hook();
    let c_settings = SettingsHandle::build(&test_settings(), None);
    let run = RunHandle::start(&c_settings).expect("the engine starts");
    let c_tc = run
        .next_test_case()
        .expect("the engine schedules at least one case");
    run_test_case(c_tc, body, is_final, Mode::TestRun, verbosity)
}

fn run_case_capturing(
    is_final: bool,
    verbosity: Verbosity,
    body: &mut dyn FnMut(TestCase),
) -> TestCaseResult {
    run_one_case(is_final, verbosity, body).0
}

/// Drive a real run of `body` and return the message it panics with.
fn drive_panic_message(body: impl FnMut(TestCase), settings: &Settings) -> String {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drive(body, settings, None, None)
    }));
    let payload = result.expect_err("drive should panic on a failing run");
    payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("")
        .to_string()
}

// ── run_lifecycle: drive re-raises and reports ───────────────────────────

#[test]
fn drive_single_failure_reraises_the_tests_own_panic() {
    // A single distinct failure re-raises the test body's *own* panic, payload
    // intact, so `#[should_panic(expected = ...)]` / `catch_unwind` consumers
    // see exactly what the test raised — not synthetic framing.
    let msg = drive_panic_message(
        |tc| {
            let x: i32 = tc.draw(integers());
            assert!(x == 0, "boom x={x}");
        },
        &test_settings(),
    );
    assert!(
        msg.contains("boom x="),
        "expected the test's own panic, got: {msg}"
    );
}

#[test]
fn drive_multiple_failures_panics_with_the_count() {
    // Two distinct panic sites are two distinct bugs; with
    // report_multiple_failures (the default) the run ends with the count.
    let msg = drive_panic_message(
        |tc| {
            let b: bool = tc.draw(booleans());
            if b {
                panic!("boom A");
            }
            panic!("boom B");
        },
        &test_settings(),
    );
    assert!(
        msg.contains("2 distinct failures"),
        "expected the multi-failure count, got: {msg}"
    );
}

#[test]
fn drive_run_error_panics_with_the_engines_message() {
    // A run-level failure (here a FilterTooMuch health check from rejecting
    // every case) is not a test failure: drive surfaces the engine's own
    // message, with no `Property test failed:` framing.
    let msg = drive_panic_message(
        |tc| {
            let _: i32 = tc.draw(integers());
            tc.assume(false);
        },
        &test_settings(),
    );
    assert!(
        msg.to_lowercase().contains("filter"),
        "expected a health-check run error, got: {msg}"
    );
    assert!(
        !msg.contains("Property test failed"),
        "a run error must not use the test-failure framing, got: {msg}"
    );
}

#[test]
fn drive_passing_run_does_not_panic() {
    drive(
        |tc| {
            let _: i32 = tc.draw(integers());
        },
        &test_settings(),
        None,
        None,
    );
}

// ── run_lifecycle: backtrace capture is gated to where it is shown ───────
//
// The only backtraces ever shown belong to failures whose diagnostic is
// emitted — a non-quiet final replay or, in verbose mode, every interesting
// case. `run_test_case` sets `CAPTURE_BACKTRACE` to `should_emit`, and the
// panic hook captures a backtrace only when the flag is set.

fn backtraces_enabled() -> bool {
    matches!(
        std::backtrace::Backtrace::capture().status(),
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
    // Non-final, non-verbose: `should_emit` is false — the shrinker hot path
    // must not pay for a backtrace.
    run_case_capturing(false, Verbosity::Normal, &mut |_tc| panic!("{}", "boom"));
    assert!(
        !capture_flag(),
        "a discarded (non-final) failure must not pay for a backtrace"
    );
}

#[test]
fn shown_failures_enable_backtrace_capture() {
    run_case_capturing(true, Verbosity::Normal, &mut |_tc| panic!("{}", "boom"));
    assert!(capture_flag());
}

#[test]
fn quiet_final_replay_skips_backtrace_capture() {
    run_case_capturing(true, Verbosity::Quiet, &mut |_tc| panic!("{}", "boom"));
    assert!(!capture_flag());
}

#[test]
fn verbose_mode_enables_backtrace_capture_for_non_final_failures() {
    run_case_capturing(false, Verbosity::Verbose, &mut |_tc| panic!("{}", "boom"));
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
    // hooks — so even on a final replay the hook records nothing.
    init_panic_hook();
    take_panic_info();
    let (result, payload, _) = run_one_case(true, Verbosity::Normal, &mut |tc| {
        tc.assume(false);
    });
    assert!(matches!(result, TestCaseResult::Invalid));
    assert!(payload.is_none(), "control flow carries no failure payload");
    assert!(
        take_panic_info().is_none(),
        "a control-flow unwind must not touch the panic hook's state"
    );
}

// ── run_lifecycle: where diagnostics and replay output land ──────────────
//
// Draw/note lines and — so `drive` can buffer the per-failure block and print
// the multi-failure headline before it — the final replay's diagnostic flow
// through the installed output sink. With no sink they go to stderr.

#[test]
fn verbose_non_final_diagnostic_flows_through_the_sink_without_duplicating_notes() {
    use std::sync::{Arc, Mutex};

    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = lines.clone();
    let sink: crate::test_case::OutputSink =
        Arc::new(move |s: &str| writer.lock().unwrap().push(s.to_string()));

    crate::test_case::with_output_override(sink, || {
        run_case_capturing(false, Verbosity::Verbose, &mut |tc| {
            tc.note("the noted line");
            panic!("{}", "boom");
        })
    });

    let lines = lines.lock().unwrap();
    assert_eq!(
        lines.iter().filter(|l| *l == "the noted line").count(),
        1,
        "the live note must appear exactly once, got {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("panicked at")),
        "the diagnostic must flow through the sink, got {lines:?}"
    );
}

#[test]
fn final_replay_sink_receives_output_but_not_the_diagnostic() {
    use std::sync::{Arc, Mutex};

    // A final replay's draw/note lines flow through the installed sink (so
    // snapshot tests capture them), but its diagnostic is *returned* by
    // `run_test_case` rather than sinked — keeping nondeterministic panic
    // locations out of captured snapshots. (`drive` appends that returned
    // diagnostic to its own buffer; it never reaches the sink.)
    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = lines.clone();
    let sink: crate::test_case::OutputSink =
        Arc::new(move |s: &str| writer.lock().unwrap().push(s.to_string()));

    crate::test_case::with_output_override(sink, || {
        run_case_capturing(true, Verbosity::Normal, &mut |tc| {
            tc.note("the noted line");
            panic!("{}", "boom");
        })
    });

    assert_eq!(lines.lock().unwrap().as_slice(), ["the noted line"]);
}

#[test]
fn quiet_final_replay_emits_no_draw_or_note_output() {
    use std::sync::{Arc, Mutex};

    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = lines.clone();
    let sink: crate::test_case::OutputSink =
        Arc::new(move |s: &str| writer.lock().unwrap().push(s.to_string()));

    crate::test_case::with_output_override(sink, || {
        run_case_capturing(true, Verbosity::Quiet, &mut |tc| {
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

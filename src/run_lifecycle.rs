//! Shared per-test-case execution lifecycle.
//!
//! Provides the panic hook, the `run_test_case` wrapper that catches a single
//! test body's panic and converts it into a [`TestCaseResult`], and the
//! `drive` function that takes a [`TestRunner`] implementation, hands it a
//! `run_case` callback, and surfaces the run-level result.
//!
//! The native engine backend (`crate::native::test_runner::NativeTestRunner`)
//! plugs into this lifecycle. The runner is free to do whatever it likes
//! inside its `TestRunner::explore` to decide which test cases to run; the
//! lifecycle owns everything that surrounds it — installing the panic hook,
//! wrapping each test body with `catch_unwind` plus `mark_complete`, the
//! antithesis integration, the final replay of each discovered
//! counterexample (with its report printed around it), and the closing
//! re-raise of the failing test's own panic.

use std::backtrace::{Backtrace, BacktraceStatus};
use std::cell::{Cell, RefCell};
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, Once};

use crate::antithesis::TestLocation;
use crate::backend::{Failure, TestCaseResult};
use crate::control::{
    AssumeFailed, InternalError, InvalidArgument, LoopDone, StopTest, currently_in_test_context,
    hegel_internal_error, with_test_context,
};
use crate::ffi::{CTestCase, RunHandle, SettingsHandle};
use crate::runner::{Mode, Settings, Verbosity};
use crate::test_case::{OutputSink, TestCase, current_output_sink, with_output_override};

static PANIC_HOOK_INIT: Once = Once::new();

thread_local! {
    /// `(thread_name, thread_id, location, backtrace)` captured by the panic
    /// hook for the most recent panic raised inside a test context. The
    /// hook overwrites this on every panic; callers consume it with
    /// [`take_panic_info`] right after `catch_unwind` returns.
    static LAST_PANIC_INFO: RefCell<Option<(String, String, String, Backtrace)>> =
        const { RefCell::new(None) };

    /// Whether the panic hook should pay to capture a backtrace for the next
    /// panic on this thread. Set by [`run_test_case`] to `should_emit`
    /// (`(is_final && !quiet) || verbose`) — i.e. only when the resulting
    /// diagnostic will actually be shown. Capturing (and, under
    /// `RUST_BACKTRACE`, symbolizing) a backtrace for every discarded shrink
    /// probe is the dominant cost of failing-heavy property runs, and is far
    /// worse on Windows.
    static CAPTURE_BACKTRACE: Cell<bool> = const { Cell::new(false) };
}

fn take_panic_info() -> Option<(String, String, String, Backtrace)> {
    LAST_PANIC_INFO.with(|info| info.borrow_mut().take())
}

/// Install the cross-backend panic hook on first call.
///
/// Idempotent across all backends: the hook captures the location for any
/// panic raised inside a test context (so [`run_test_case`] can read it after
/// `catch_unwind`), and forwards everything else to the previous hook
/// unchanged. A backtrace is captured only when [`CAPTURE_BACKTRACE`] is
/// set. Control-flow unwinds (a rejected assumption, out-of-data, ...)
/// never reach any hook at all — they are raised via
/// [`crate::control::raise_control`]'s `resume_unwind`, which skips hooks
/// by construction.
pub(crate) fn init_panic_hook() {
    PANIC_HOOK_INIT.call_once(|| {
        let prev_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !currently_in_test_context() {
                prev_hook(info);
                return;
            }

            let thread = std::thread::current();
            let thread_name = thread.name().unwrap_or("<unnamed>").to_string();
            // `ThreadId` only exposes its integer via the unstable
            // `as_u64`, so scrape the `Debug` form ("ThreadId(N)"). That
            // format is not guaranteed; if it changes, the diagnostic
            // header degrades cosmetically and the report-layout tests'
            // `(\d+)` patterns will flag it on the toolchain bump.
            let thread_id = format!("{:?}", thread.id())
                .trim_start_matches("ThreadId(")
                .trim_end_matches(')')
                .to_string();
            let location = info
                .location()
                .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
                .unwrap_or_else(|| "<unknown>".to_string());
            // Only capture (and symbolize) a backtrace when the diagnostic
            // will actually be shown.
            let backtrace = if CAPTURE_BACKTRACE.get() {
                Backtrace::capture()
            } else {
                Backtrace::disabled()
            };

            LAST_PANIC_INFO
                .with(|l| *l.borrow_mut() = Some((thread_name, thread_id, location, backtrace)));
        }));
    });
}

/// Format a backtrace, optionally filtering to the "short" format that the
/// default Rust panic handler uses.
fn format_backtrace(bt: &Backtrace, full: bool) -> String {
    let backtrace_str = format!("{}", bt);
    if full {
        return backtrace_str;
    }
    filter_short_backtrace(&backtrace_str)
}

/// Trim a `Backtrace`-as-string down to the "short" view (between
/// `__rust_end_short_backtrace` and `__rust_begin_short_backtrace`) and
/// renumber the surviving frames. Split out from [`format_backtrace`] so
/// the string-shape branches (frames with a `digit:` prefix, frames with
/// a digit but no colon, frames that don't start with a digit) can be
/// covered directly.
fn filter_short_backtrace(backtrace_str: &str) -> String {
    let lines: Vec<&str> = backtrace_str.lines().collect();
    let mut start_idx = 0;
    let mut end_idx = lines.len();

    for (i, line) in lines.iter().enumerate() {
        if line.contains("__rust_end_short_backtrace") {
            for (j, next_line) in lines.iter().enumerate().skip(i + 1) {
                if next_line
                    .trim_start()
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
                {
                    start_idx = j;
                    break;
                }
            }
        }
        if line.contains("__rust_begin_short_backtrace") {
            for (j, prev_line) in lines
                .iter()
                .enumerate()
                .take(i + 1)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                if prev_line
                    .trim_start()
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
                {
                    end_idx = j;
                    break;
                }
            }
            break;
        }
    }

    let filtered: Vec<&str> = lines[start_idx..end_idx].to_vec();
    let mut new_frame_num = 0usize;
    let mut result = Vec::new();
    for line in filtered {
        let trimmed = line.trim_start();
        if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            if let Some(colon_pos) = trimmed.find(':') {
                let rest = &trimmed[colon_pos..];
                result.push(format!("{:>4}{}", new_frame_num, rest));
                new_frame_num += 1;
            } else {
                result.push(line.to_string());
            }
        } else {
            result.push(line.to_string());
        }
    }
    result.join("\n")
}

/// Placeholder thread/location/backtrace tuple used when the panic hook
/// captured nothing for a caught panic. This *is* reached in production:
/// a genuine panic on a spawned thread lands its capture in that thread's
/// `LAST_PANIC_INFO`, and the `join().unwrap()` that propagates it uses
/// `resume_unwind`, which skips the hook on the joining thread — so the
/// lifecycle finds nothing here. One consequence is that every such
/// failure shares the origin `"Panic at <unknown>"`, merging distinct
/// threaded bugs into one counterexample; fixing that needs cross-thread
/// capture, which is deferred until there is structured concurrency
/// support to hang it on.
pub(crate) fn unknown_panic_info() -> (String, String, String, Backtrace) {
    (
        "<unknown>".to_string(),
        "?".to_string(),
        "<unknown>".to_string(),
        Backtrace::disabled(),
    )
}

/// Extract a string message from a panic payload. Pre-N1 this was duplicated
/// in `src/native/runner.rs:11`; that copy now re-exports this one.
///
/// `pub` so that the libhegel C bindings (`hegel-c`) can use it from their
/// own `catch_unwind` wrapper around `run_native`. Not part of the
/// supported public surface — `#[doc(hidden)]`.
#[doc(hidden)]
pub fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}

/// Run the user's test body once for the supplied [`DataSource`], catching
/// any panic and translating it to a [`TestCaseResult`].
///
/// Reports the outcome back through the [`DataSource`] interface via
/// [`TestCase::mark_complete`]: that is the channel for per-test-case
/// results.  The native engine reads it back off the data-source handle.
/// On the `Interesting` path the panic site is captured as a
/// `file:line:col` string and stored on the [`Failure`] so per-origin
/// shrinking can key on it, and the rendered diagnostic block (panic
/// location, message, backtrace) is printed here, at the moment the panic
/// is caught — to stderr on a non-quiet final replay (right after the live
/// draw/note lines, which is what keeps each failure one block), or
/// through the verbose output sink for a non-final case in verbose mode.
///
/// Also returns the caught panic payload for an `Interesting` result, so a
/// final replay's caller can re-raise the test's *own* panic as the run's
/// closing unwind instead of synthesizing one.
pub(crate) fn run_test_case(
    c_tc: CTestCase,
    test_fn: &mut dyn FnMut(TestCase),
    is_final: bool,
    mode: Mode,
    verbosity: Verbosity,
) -> (
    TestCaseResult,
    Option<Box<dyn std::any::Any + Send>>,
    Option<String>,
) {
    let verbose = matches!(verbosity, Verbosity::Verbose | Verbosity::Debug);
    let quiet = verbosity == Verbosity::Quiet;
    // Surface draw/note output — and pay for a backtrace — only when the
    // diagnostic will be shown: a non-quiet final replay, or every test
    // case in verbose mode.
    let should_emit = (is_final && !quiet) || verbose;
    CAPTURE_BACKTRACE.with(|c| c.set(should_emit));

    let tc = TestCase::new(c_tc, should_emit, mode);
    let result = with_test_context(|| catch_unwind(AssertUnwindSafe(|| test_fn(tc.clone()))));

    let (tc_result, payload, diagnostic) = match result {
        Ok(()) => (TestCaseResult::Valid, None, None),
        Err(e) if e.downcast_ref::<AssumeFailed>().is_some() => {
            (TestCaseResult::Invalid, None, None)
        }
        Err(e) if e.downcast_ref::<StopTest>().is_some() => (TestCaseResult::Overrun, None, None),
        Err(e) if e.downcast_ref::<LoopDone>().is_some() => (TestCaseResult::Valid, None, None),
        Err(e) => {
            // An invalid-argument (usage) error is a mistake in how the
            // test is configured, not a discovered counterexample: abort
            // the run with the message instead of recording it as
            // `Interesting` and shrinking it.
            let e = match e.downcast::<InvalidArgument>() {
                Ok(invalid) => std::panic::resume_unwind(Box::new(invalid.0)),
                Err(e) => e,
            };
            // A violated internal invariant is a bug in Hegel: abort the
            // run with the bug-report message rather than spending the
            // shrink budget "minimizing" a framework bug.
            let e = match e.downcast::<InternalError>() {
                Ok(internal) => std::panic::resume_unwind(Box::new(internal.0)),
                Err(e) => e,
            };
            let msg = panic_message(&e);
            let (thread_name, thread_id, location, backtrace) =
                take_panic_info().unwrap_or_else(unknown_panic_info);

            let diagnostic =
                render_diagnostic(&thread_name, &thread_id, &location, &msg, &backtrace);
            // A final replay's diagnostic is *returned* to the caller (`drive`)
            // rather than emitted here: drive buffers it into the per-failure
            // block so the multi-failure headline can precede it, and keeping
            // it out of the output sink preserves the snapshot invariant that
            // sinks capture only draws/notes (no nondeterministic panic
            // location). A verbose *non-final* case has no such buffering, so
            // its diagnostic flows live through the sink (or stderr) here.
            let captured = if is_final && !quiet {
                Some(diagnostic)
            } else {
                if verbose {
                    // The sink is line-oriented and `diagnostic` ends in a
                    // newline, so drop the trailing empty piece.
                    for line in diagnostic.trim_end_matches('\n').split('\n') {
                        crate::test_case::emit_verbose_line(line);
                    }
                }
                None
            };
            let failure = TestCaseResult::Interesting(Failure {
                panic_message: msg,
                origin: format!("Panic at {}", location),
                // The blob is attached from the run result on a failing run.
                reproduce_blob: None,
            });
            (failure, Some(e), captured)
        }
    };

    if verbose {
        emit_verbose_stop_reason(&tc_result);
    }

    tc.mark_complete(&tc_result);

    (tc_result, payload, diagnostic)
}

/// Print a per-test-case line describing why this test case stopped.
fn emit_verbose_stop_reason(result: &TestCaseResult) {
    match result {
        TestCaseResult::Invalid => {
            crate::test_case::emit_verbose_line("Test case stopped: failed assumption");
        }
        TestCaseResult::Overrun => {
            crate::test_case::emit_verbose_line("Test case stopped: out of data");
        }
        TestCaseResult::Valid | TestCaseResult::Interesting(_) => {}
    }
}

/// Render a failure's diagnostic block. Mirrors the default Rust
/// panic-handler output so each failure in the report looks like a
/// stand-alone test failure.
fn render_diagnostic(
    thread_name: &str,
    thread_id: &str,
    location: &str,
    msg: &str,
    backtrace: &Backtrace,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "thread '{}' ({}) panicked at {}:\n",
        thread_name, thread_id, location
    ));
    out.push_str(msg);
    out.push('\n');
    // nocov start
    if backtrace.status() == BacktraceStatus::Captured {
        let is_full = std::env::var("RUST_BACKTRACE")
            .map(|v| v == "full")
            .unwrap_or(false);
        let formatted = format_backtrace(backtrace, is_full);
        out.push_str(&format!("stack backtrace:\n{}\n", formatted));
        if !is_full {
            out.push_str(
                "note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.\n",
            );
        }
    }
    // nocov end
    out
}

/// The copy-pasteable reproduce failure text to append after a failure's diagnostic,
/// or `None` when nothing should be printed.
///
/// `Some` only when [`Settings::print_blob`](crate::Settings::print_blob) is
/// enabled *and* the failure carries a reproduce blob. A replayed
/// counterexample always has one; a blobless failure (e.g.
/// `Mode::SingleTestCase`, whose one random case has no shrunk choice
/// sequence to encode) prints nothing.
fn reproducer_line(settings: &Settings, reproduce_blob: Option<&str>) -> Option<String> {
    if !settings.print_blob {
        return None;
    }
    let blob = reproduce_blob?;
    Some(format!(
        "\nTo reproduce this failure, add the attribute below \
         #[hegel::test]:\n    #[hegel::reproduce_failure(\"{blob}\")]"
    ))
}

/// Drive a libhegel run to completion against the user's test function.
///
/// Installs the cross-backend panic hook, starts the engine through the C ABI
/// (`hegel_run_start`), and pulls each test case the engine schedules
/// (`hegel_next_test_case`), wrapping every one in [`run_test_case`]. The
/// engine owns the whole loop — generation, shrinking, and the final replays —
/// and flags the final replays via `hegel_test_case_is_final_replay`.
///
/// Because the failure count is only known once the loop drains
/// (`hegel_run_result`), each final replay's output is buffered so the
/// "N distinct failures" headline can still be printed before the per-failure
/// blocks, preserving the previous report ordering. The run ends by re-raising
/// the failing test's own panic (or, for several distinct bugs, a panic
/// carrying the count). A run-level error — a failed health check,
/// nondeterminism, an engine panic — surfaces with the engine's own message
/// instead of the `Property test failed:` framing.
///
/// `Mode::SingleTestCase` needs no special handling here: it is carried in the
/// settings, and the engine simply emits one final case and stops.
pub(crate) fn drive<F>(
    test_fn: F,
    settings: &Settings,
    database_key: Option<&str>,
    test_location: Option<&TestLocation>,
) where
    F: FnMut(TestCase),
{
    init_panic_hook();
    require_antithesis_feature();
    let mut test_fn = test_fn;
    let mode = settings.mode;
    let verbosity = settings.verbosity;
    let quiet = verbosity == Verbosity::Quiet;

    let c_settings = SettingsHandle::build(settings, database_key);
    let run = match RunHandle::start(&c_settings) {
        Ok(run) => run,
        // The engine could not even start. With a builder-produced settings
        // handle this only happens on OS worker-thread spawn failure (see
        // ffi::RunHandle::start), an unprovokable resource-exhaustion path;
        // surfaced as the run's own panic message.
        Err(message) => panic!("{message}"), // nocov
    };

    // Pull and run each scheduled case. Final replays (emitted after
    // exploration) get their output buffered so the headline can precede them;
    // the last final replay's panic payload is kept for the closing re-raise.
    let mut final_blocks: Vec<Vec<String>> = Vec::new();
    let mut last_payload: Option<Box<dyn std::any::Any + Send>> = None;
    while let Some(c_tc) = run.next_test_case() {
        if !c_tc.is_final_replay() {
            run_test_case(c_tc, &mut test_fn, false, mode, verbosity);
            continue;
        }
        // A final replay: always capture the payload (so the run can re-raise
        // the test's own panic), and buffer its block (draws/notes + the
        // returned diagnostic) unless quiet so the headline can be printed
        // first. The buffering sink *tees* to any sink already installed (e.g.
        // a test capturing draws) so it still sees the draw/note lines; the
        // diagnostic is returned separately and goes only into the block, never
        // through the sink.
        if quiet {
            let (_, payload, _) = run_test_case(c_tc, &mut test_fn, true, mode, verbosity);
            last_payload = payload;
        } else {
            let buffer: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
            let outer = current_output_sink();
            let sink: OutputSink = {
                let buffer = Arc::clone(&buffer);
                Arc::new(move |line: &str| {
                    buffer.lock().unwrap().push(line.to_string());
                    if let Some(outer) = &outer {
                        outer(line);
                    }
                })
            };
            let (_, payload, diagnostic) = with_output_override(sink, || {
                run_test_case(c_tc, &mut test_fn, true, mode, verbosity)
            });
            last_payload = payload;
            let mut block = std::mem::take(&mut *buffer.lock().unwrap());
            if let Some(diagnostic) = diagnostic {
                block.extend(
                    diagnostic
                        .trim_end_matches('\n')
                        .split('\n')
                        .map(str::to_string),
                );
            }
            final_blocks.push(block);
        }
    }

    let result = run.result();
    use hegel_c::hegel_run_status_t as RunStatus;
    let status = result.status();
    emit_antithesis_assertion(status != RunStatus::HEGEL_RUN_STATUS_PASSED, test_location);

    match status {
        RunStatus::HEGEL_RUN_STATUS_PASSED => {}
        RunStatus::HEGEL_RUN_STATUS_ERROR => {
            // A failure of the run itself (health check, nondeterminism, engine
            // panic) — not a test failure, so it gets the engine's own message.
            let message = result
                .error()
                .unwrap_or_else(|| "the run failed with an unknown error".to_string());
            panic!("{message}");
        }
        RunStatus::HEGEL_RUN_STATUS_FAILED => {
            let count = result.failure_count();
            let multiple = count > 1;
            // The headline precedes the per-failure blocks (the closing
            // `panic!`'s message is only rendered later, by the panic hook).
            if multiple && !quiet {
                eprintln!("Property-based test failed with {count} distinct failures.");
            }
            let mut reported: Vec<String> = Vec::new();
            for index in 0..count {
                if multiple && !quiet {
                    eprintln!();
                }
                // Flush the buffered draws + diagnostic for this failure, in
                // the same order the engine replayed them.
                if let Some(block) = final_blocks.get(index) {
                    for line in block {
                        eprintln!("{line}");
                    }
                }
                let failure = result
                    .failure(index)
                    .unwrap_or_else(|| hegel_internal_error!("failure index {index} out of range"));
                if let Some(line) = reproducer_line(settings, failure.reproduce_blob.as_deref()) {
                    eprintln!("{line}");
                }
                reported.push(failure.panic_message);
            }

            // End the run by re-raising. `resume_unwind` skips the panic hook,
            // so nothing prints twice — the unwind is purely the run's result.
            match reported.as_slice() {
                // Defensive: a FAILED status with no failures shouldn't happen.
                [] => panic!("Property test failed: unknown"), // nocov
                // Single-failure path: re-raise the test's *own* panic, payload
                // intact, so `should_panic(expected = ...)` and `catch_unwind`
                // consumers see exactly what the test raised. A quiet run that
                // somehow produced no payload falls back to a synthetic panic.
                [message] => match last_payload.take() {
                    Some(payload) => std::panic::resume_unwind(payload),
                    None => panic!("Property test failed: {}", message), // nocov
                },
                // Multi-failure path: no single panic to re-raise, so fail with
                // the count (already eprinted as the headline).
                many => std::panic::resume_unwind(Box::new(format!(
                    "Property-based test failed with {} distinct failures.",
                    many.len()
                ))),
            }
        }
    }
}

/// Replay a single base64 failure blob through the C ABI
/// (`hegel_test_case_from_blob`), bypassing generation and shrinking.
///
/// Decoding failures (corrupt or incompatible blobs) panic with the engine's
/// diagnostic. A blob that decodes but no longer fails is a stale reproducer,
/// reported as such. A reproduced failure re-raises the test's own panic; a
/// replayed example has no fresh blob to print.
pub(crate) fn drive_blob_replay<F>(
    test_fn: F,
    settings: &Settings,
    database_key: Option<&str>,
    blob: &str,
    test_location: Option<&TestLocation>,
) where
    F: FnMut(TestCase),
{
    init_panic_hook();
    require_antithesis_feature();
    let mut test_fn = test_fn;
    let c_settings = SettingsHandle::build(settings, database_key);
    let c_tc = match CTestCase::from_blob(&c_settings, blob) {
        Ok(c_tc) => c_tc,
        // Undecodable / incompatible blob — there is nothing to replay.
        Err(message) => panic!("{message}"),
    };
    // The replayed example *is* the counterexample, so it is final.
    let (result, payload, diagnostic) =
        run_test_case(c_tc, &mut test_fn, true, settings.mode, settings.verbosity);
    // run_test_case returns a final replay's diagnostic rather than emitting
    // it; a single replay has no headline to print first, so emit it now.
    if let Some(diagnostic) = diagnostic {
        eprint!("{diagnostic}");
    }
    match result {
        TestCaseResult::Interesting(_) => {
            emit_antithesis_assertion(true, test_location);
            // No reproducer line: a replay has no fresh blob to print.
            match payload {
                Some(payload) => std::panic::resume_unwind(payload),
                None => unreachable!(), // nocov
            }
        }
        _ => {
            // The blob decoded but no longer fails: a stale reproducer.
            emit_antithesis_assertion(false, test_location);
            panic!(
                "reproduce_failure: the supplied failure blob no longer reproduces a \
                 failure. The failure may have been fixed, or the blob is stale."
            );
        }
    }
}

/// Fail fast — before any test case runs — when running under Antithesis
/// without the `antithesis` feature compiled in.
fn require_antithesis_feature() {
    crate::antithesis::require_antithesis_feature(
        crate::antithesis::is_running_in_antithesis(),
        cfg!(feature = "antithesis"),
    );
}

/// Report the run's verdict to Antithesis (when running under it).
fn emit_antithesis_assertion(test_failed: bool, test_location: Option<&TestLocation>) {
    #[cfg(feature = "antithesis")]
    // nocov start
    if crate::antithesis::is_running_in_antithesis() {
        if let Some(loc) = test_location {
            crate::antithesis::emit_assertion(loc, !test_failed);
        }
    }
    // nocov end
    let _ = (test_failed, test_location);
}

#[cfg(test)]
#[path = "../tests/embedded/run_lifecycle_tests.rs"]
mod tests;

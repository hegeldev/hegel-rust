//! Shared per-test-case execution lifecycle.
//!
//! Provides the panic hook, the `run_test_case` wrapper that catches a single
//! test body's panic and converts it into a [`TestCaseResult`], and the
//! `drive` function that starts a libhegel run, pulls each test case the
//! engine schedules, and surfaces the run-level result.
//!
//! The engine lives behind libhegel's C ABI; `drive` owns everything around
//! it — installing the panic hook, wrapping each test body with
//! `catch_unwind` plus `mark_complete`, the antithesis integration, the final
//! replay of each discovered counterexample (with its report printed around
//! it), and the closing re-raise of the failing test's own panic.

use std::backtrace::{Backtrace, BacktraceStatus};
use std::cell::{Cell, RefCell};
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Once};

use crate::antithesis::TestLocation;
use crate::backend::{Failure, TestCaseResult};
use crate::control::{
    AssumeFailed, InternalError, InvalidArgument, LoopDone, StopTest, currently_in_test_context,
    hegel_internal_error, with_test_context,
};
use crate::ffi::{CTestCase, RunHandle, SettingsHandle};
use crate::runner::{Mode, Settings, Verbosity};
use crate::test_case::{RunOutput, TestCase};

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

/// The `(thread_name, thread_id, location, backtrace)` tuple the panic hook
/// captures.
pub(crate) type PanicInfo = (String, String, String, Backtrace);

pub(crate) fn take_panic_info() -> Option<PanicInfo> {
    LAST_PANIC_INFO.with(|info| info.borrow_mut().take())
}

/// Install `info` into this thread's panic-info slot, as if the panic hook
/// had captured it here. Used by `stateful::run_concurrent` to re-install a
/// worker thread's capture on the main thread before `resume_unwind`ing the
/// ferried payload — the re-raise skips the panic hook, so without this the
/// lifecycle would fall back to [`unknown_panic_info`] and every concurrent
/// failure would share the origin `"Panic at <unknown>"`.
pub(crate) fn install_panic_info(info: PanicInfo) {
    LAST_PANIC_INFO.with(|slot| *slot.borrow_mut() = Some(info));
}

/// Whether the panic hook captures backtraces on this thread right now.
/// `run_test_case` decides this per case; `stateful::run_concurrent` reads
/// it on the main thread to mirror the setting onto its worker threads.
pub(crate) fn backtrace_capture_enabled() -> bool {
    CAPTURE_BACKTRACE.get()
}

/// Set whether the panic hook captures backtraces on this thread. Called by
/// `stateful::run_concurrent` on each worker thread (see
/// [`backtrace_capture_enabled`]).
pub(crate) fn set_backtrace_capture(enabled: bool) {
    CAPTURE_BACKTRACE.set(enabled);
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
            let thread_id = format!("{:?}", thread.id())
                .trim_start_matches("ThreadId(")
                .trim_end_matches(')')
                .to_string();
            let location = info
                .location()
                .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
                .unwrap_or_else(|| "<unknown>".to_string());
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

/// Run the user's test body once against the supplied libhegel test case,
/// catching any panic and translating it to a [`TestCaseResult`].
///
/// The body's `TestCase` shares `c_tc` through an `Arc`, and the outcome is
/// reported on the same shared handle after the body finishes — so no
/// per-case libhegel clone is needed (a real clone is created only when the
/// body itself calls `tc.clone()`), while a `TestCase` that escapes the body
/// (moved to a thread that is never joined) keeps the handle alive rather
/// than dangling; its later draws fail cleanly because the case has finished.
///
/// Reports the outcome back to the engine via [`report_outcome`] (which calls
/// `hegel_mark_complete`): that is the channel for per-test-case results, and
/// the engine reads it back over the C ABI.
/// On the `Interesting` path the panic site is captured as a
/// `file:line:col` string and stored on the [`Failure`] so per-origin
/// shrinking can key on it, and the rendered diagnostic block (panic
/// location, message, backtrace) is printed here, at the moment the panic
/// is caught — on a non-quiet final replay it is returned to the caller to
/// print (right after the live draw/note lines, which is what keeps each
/// failure one block), and for a non-final case in verbose mode it goes to
/// `output`, the run's resolved destination.
///
/// Also returns the caught panic payload for an `Interesting` result, so a
/// final replay's caller can re-raise the test's *own* panic as the run's
/// closing unwind instead of synthesizing one.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_test_case(
    c_tc: CTestCase,
    test_fn: &mut dyn FnMut(TestCase),
    is_final: bool,
    mode: Mode,
    verbosity: Verbosity,
    output: &RunOutput,
    nondeterministic: bool,
    case_sink: Option<crate::test_case::OutputSink>,
) -> (
    TestCaseResult,
    Option<Box<dyn std::any::Any + Send>>,
    Option<String>,
) {
    let verbose = matches!(verbosity, Verbosity::Verbose | Verbosity::Debug);
    let quiet = verbosity == Verbosity::Quiet;
    let capture_at_discovery = nondeterministic && !is_final;
    let should_emit = ((is_final || capture_at_discovery) && !quiet) || verbose;
    CAPTURE_BACKTRACE.with(|c| c.set(should_emit));
    // Drop any capture left over from a previous test case on this thread
    // (e.g. a body that caught its own panic and then passed): a later panic
    // that skips the hook via resume_unwind must not inherit it.
    take_panic_info();

    let c_tc = Arc::new(c_tc);
    let tc = TestCase::new(
        Arc::clone(&c_tc),
        should_emit,
        mode,
        nondeterministic,
        case_sink.or_else(|| output.sink().cloned()),
    );
    let result = with_test_context(|| catch_unwind(AssertUnwindSafe(|| test_fn(tc))));

    let (tc_result, payload, diagnostic) = match result {
        Ok(()) => (TestCaseResult::Valid, None, None),
        Err(e) if e.downcast_ref::<AssumeFailed>().is_some() => {
            (TestCaseResult::Invalid, None, None)
        }
        Err(e) if e.downcast_ref::<StopTest>().is_some() => (TestCaseResult::Overrun, None, None),
        Err(e) if e.downcast_ref::<LoopDone>().is_some() => (TestCaseResult::Valid, None, None),
        Err(e) => {
            let e = match e.downcast::<InvalidArgument>() {
                Ok(invalid) => std::panic::resume_unwind(Box::new(invalid.0)),
                Err(e) => e,
            };
            let e = match e.downcast::<InternalError>() {
                Ok(internal) => std::panic::resume_unwind(Box::new(internal.0)),
                Err(e) => e,
            };
            let (thread_name, thread_id, location, backtrace) =
                take_panic_info().unwrap_or_else(unknown_panic_info);

            let captured = if (is_final || capture_at_discovery) && !quiet {
                let msg = panic_message(&e);
                Some(render_diagnostic(
                    &thread_name,
                    &thread_id,
                    &location,
                    &msg,
                    &backtrace,
                ))
            } else {
                if verbose {
                    let msg = panic_message(&e);
                    let diagnostic =
                        render_diagnostic(&thread_name, &thread_id, &location, &msg, &backtrace);
                    for line in diagnostic.trim_end_matches('\n').split('\n') {
                        output.line(line);
                    }
                }
                None
            };
            let failure = TestCaseResult::Interesting(Failure {
                origin: format!("Panic at {}", location),
            });
            (failure, Some(e), captured)
        }
    };

    if verbose {
        emit_verbose_stop_reason(&tc_result, output);
    }

    report_outcome(&c_tc, &tc_result);

    (tc_result, payload, diagnostic)
}

/// Report a test case's outcome to the engine over the C ABI.
///
/// Only the status — and, for an interesting (failing) case, the bug origin —
/// crosses the boundary; the panic message and reproduce blob are recovered
/// from the run result afterward, not pushed through here. This reports on
/// the handle the body drew on (shared with it through the `Arc`);
/// `hegel_mark_complete` waits for any in-flight operation on the handle, so
/// a still-running leaked thread cannot make this report fail.
fn report_outcome(handle: &CTestCase, result: &TestCaseResult) {
    use hegel_c::hegel_status_t as Status;
    let (status, origin) = match result {
        TestCaseResult::Valid => (Status::HEGEL_STATUS_VALID, None),
        TestCaseResult::Invalid => (Status::HEGEL_STATUS_INVALID, None),
        TestCaseResult::Overrun => (Status::HEGEL_STATUS_OVERRUN, None),
        TestCaseResult::Interesting(failure) => (
            Status::HEGEL_STATUS_INTERESTING,
            Some(failure.origin.as_str()),
        ),
    };
    handle
        .mark_complete(status, origin)
        .unwrap_or_else(|rc| crate::test_case::raise_for_rc(rc));
}

/// Print a per-test-case line describing why this test case stopped.
fn emit_verbose_stop_reason(result: &TestCaseResult, output: &RunOutput) {
    match result {
        TestCaseResult::Invalid => {
            output.line("Test case stopped: failed assumption");
        }
        TestCaseResult::Overrun => {
            output.line("Test case stopped: out of data");
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

/// The run's failure candidate retained by a declared-nondeterministic run:
/// everything captured at discovery time from the last test case that
/// classified interesting frontend-side. There is no replay for such a run,
/// so discovery is the only chance to capture — but the *print* decision is
/// deferred to the run verdict, because a frontend-interesting report can
/// lose silently to an engine-side family conclusion (an overrunning or
/// invalidating draw concluded the family first, and `mark_complete` after
/// a conclusion is a no-op). If the run comes back failed the stash is the
/// accepted bug and is printed then; if the run passes the stash lost, and
/// is discarded — a genuine racy bug resurfaces in a later case.
struct NondetStash {
    /// The buffered draw/note lines of the case (empty under
    /// [`Verbosity::Quiet`], where nothing would be printed).
    lines: Vec<String>,
    /// The rendered panic diagnostic (thread, location, message, backtrace);
    /// `None` under quiet.
    diagnostic: Option<String>,
    /// The caught panic payload, re-raised as the run's closing unwind.
    payload: Box<dyn std::any::Any + Send>,
}

/// Message for a flaky test — one whose outcome changed when re-run with the
/// same generated data. After the engine shrinks and verifies a counterexample,
/// the client replays its blob one final time; if that replay does not fail,
/// the test is non-deterministic.
const FLAKY_DIAGNOSTIC: &str = "Flaky test detected: Your test produced different outcomes \
     when run with the same generated data — it failed when it \
     previously succeeded, or succeeded when it previously failed. \
     This usually means your test depends on external state such as \
     global variables, system time, or external random number generators.";

/// Drive a libhegel run to completion against the user's test function.
///
/// Installs the cross-backend panic hook, starts the engine through the C ABI
/// (`hegel_run_start`), and pulls each test case the engine schedules
/// (`hegel_next_test_case`), wrapping every one in [`run_test_case`]. The
/// engine only *explores* — generation and shrinking — so every pumped case is
/// non-final. The client owns the final replays: once the loop drains, it reads
/// each discovered counterexample's reproduce blob from `hegel_run_result` and
/// replays it via [`drive`]'s own `from_blob` path, marking it final itself.
///
/// Because the failures (and their count) are known up front once the loop
/// drains, the "N distinct failures" headline is printed before replaying, and
/// each replay's draws/notes flow live (to the active sink or stderr) followed
/// by its diagnostic and reproducer line, so each failure prints as one grouped
/// block. The run ends by re-raising the failing test's own panic (or, for
/// several distinct bugs, a panic carrying the count). A run-level error — a
/// failed health check, nondeterminism, an engine panic — surfaces with the
/// engine's own message instead of the `Property test failed:` framing.
///
/// `Mode::SingleTestCase` has no exploration, shrinking, or blob: the engine
/// emits one case, and if it fails the run re-raises that case's own panic
/// straight away (see [`drive_single_case`]).
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
    let output = RunOutput::resolve();

    let c_settings = SettingsHandle::build(settings, database_key);
    let run = match RunHandle::start(&c_settings, output.sink()) {
        Ok(run) => run,
        Err(message) => panic!("{message}"), // nocov
    };

    if mode == Mode::SingleTestCase {
        drive_single_case(
            &run,
            &mut test_fn,
            verbosity,
            settings.nondeterministic,
            test_location,
            &output,
        );
        return;
    }

    let nondeterministic = settings.nondeterministic;
    let mut stash: Option<NondetStash> = None;
    while let Some(c_tc) = run.next_test_case() {
        if nondeterministic {
            let buffer: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
            let case_sink: Option<crate::test_case::OutputSink> = if quiet {
                None
            } else {
                let buffer = Arc::clone(&buffer);
                Some(Arc::new(move |line: &str| {
                    buffer
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(line.to_string());
                }))
            };
            let (tc_result, payload, diagnostic) = run_test_case(
                c_tc,
                &mut test_fn,
                false,
                mode,
                verbosity,
                &output,
                true,
                case_sink,
            );
            if matches!(tc_result, TestCaseResult::Interesting(_)) {
                let lines = std::mem::take(&mut *buffer.lock().unwrap_or_else(|e| e.into_inner()));
                stash = Some(NondetStash {
                    lines,
                    diagnostic,
                    payload: payload.expect("an interesting case carries the caught panic payload"),
                });
            }
        } else {
            run_test_case(
                c_tc,
                &mut test_fn,
                false,
                mode,
                verbosity,
                &output,
                false,
                None,
            );
        }
    }

    let result = run.result();
    use hegel_c::hegel_run_status_t as RunStatus;
    let status = result.status();
    emit_antithesis_assertion(status != RunStatus::HEGEL_RUN_STATUS_PASSED, test_location);

    match status {
        RunStatus::HEGEL_RUN_STATUS_PASSED => {}
        RunStatus::HEGEL_RUN_STATUS_ERROR => {
            let message = result
                .error()
                .unwrap_or_else(|| "the run failed with an unknown error".to_string());
            panic!("{message}");
        }
        RunStatus::HEGEL_RUN_STATUS_FAILED if nondeterministic => {
            // The stashed candidate is necessarily the accepted bug: the
            // engine concludes a case interesting only via `mark_complete`,
            // generation stops at the first accepted bug (shrinking is off
            // run-wide), and the run ends immediately after it — so the last
            // stash written is the failure the engine is reporting. A stash
            // whose report lost to an engine-side family conclusion
            // (overrun, invalid) can only be *followed* by more cases, never
            // be the last one of a failed run.
            let stash = stash.expect("a failed nondeterministic run has a stashed failure");
            for line in &stash.lines {
                output.line(line);
            }
            if let Some(diagnostic) = stash.diagnostic {
                output.block(&diagnostic);
            }
            std::panic::resume_unwind(stash.payload);
        }
        RunStatus::HEGEL_RUN_STATUS_FAILED => {
            let count = result.failure_count();
            let multiple = count > 1;
            if multiple && !quiet {
                output.line(&format!(
                    "Property-based test failed with {count} distinct failures."
                ));
            }
            let mut last_payload: Option<Box<dyn std::any::Any + Send>> = None;
            for index in 0..count {
                if multiple && !quiet {
                    output.line("");
                }
                let blob = result
                    .failure(index)
                    .reproduce_blob
                    .unwrap_or_else(|| hegel_internal_error!("failure {index} has no blob"));
                let c_tc = match CTestCase::from_blob(&c_settings, &blob, output.sink()) {
                    Ok(c_tc) => c_tc,
                    Err(message) => panic!("{message}"), // nocov
                };
                let (tc_result, payload, diagnostic) = run_test_case(
                    c_tc,
                    &mut test_fn,
                    true,
                    mode,
                    verbosity,
                    &output,
                    false,
                    None,
                );
                if !matches!(tc_result, TestCaseResult::Interesting(_)) {
                    panic!("{FLAKY_DIAGNOSTIC}");
                }
                if let Some(diagnostic) = diagnostic {
                    output.block(&diagnostic);
                }
                if let Some(line) = reproducer_line(settings, Some(blob.as_str())) {
                    output.block(&format!("{line}\n"));
                }
                last_payload = payload;
            }

            if multiple {
                std::panic::resume_unwind(Box::new(format!(
                    "Property-based test failed with {count} distinct failures."
                )));
            } else {
                std::panic::resume_unwind(
                    last_payload.expect("a re-failing replay carries a panic payload"),
                );
            }
        }
    }
}

/// Drive a `Mode::SingleTestCase` run: the engine emits exactly one case and
/// the run's verdict is that case's outcome. The case is still run through
/// [`run_test_case`] — the engine needs its `mark_complete`, and `assume()` /
/// out-of-data must be classified rather than escape — but a real failure is
/// re-raised straight away. There is no shrinking or replay, so no report to
/// build and no run-level error to consult.
fn drive_single_case(
    run: &RunHandle,
    test_fn: &mut dyn FnMut(TestCase),
    verbosity: Verbosity,
    nondeterministic: bool,
    test_location: Option<&TestLocation>,
    output: &RunOutput,
) {
    let c_tc = run
        .next_test_case()
        .expect("a SingleTestCase run produces exactly one test case");
    let (result, payload, diagnostic) = run_test_case(
        c_tc,
        test_fn,
        true,
        Mode::SingleTestCase,
        verbosity,
        output,
        nondeterministic,
        None,
    );
    if matches!(result, TestCaseResult::Interesting(_)) {
        emit_antithesis_assertion(true, test_location);
        if let Some(diagnostic) = diagnostic {
            output.block(&diagnostic);
        }
        std::panic::resume_unwind(
            payload.expect("an interesting case carries the caught panic payload"),
        );
    }
    emit_antithesis_assertion(false, test_location);
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
    let output = RunOutput::resolve();
    let c_settings = SettingsHandle::build(settings, database_key);
    let c_tc = match CTestCase::from_blob(&c_settings, blob, output.sink()) {
        Ok(c_tc) => c_tc,
        Err(message) => panic!("{message}"),
    };
    let (result, payload, diagnostic) = run_test_case(
        c_tc,
        &mut test_fn,
        true,
        settings.mode,
        settings.verbosity,
        &output,
        settings.nondeterministic,
        None,
    );
    if let Some(diagnostic) = diagnostic {
        output.block(&diagnostic);
    }
    match result {
        TestCaseResult::Interesting(_) => {
            emit_antithesis_assertion(true, test_location);
            match payload {
                Some(payload) => std::panic::resume_unwind(payload),
                None => unreachable!(), // nocov
            }
        }
        _ => {
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

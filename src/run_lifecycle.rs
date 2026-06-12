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
//! `panic!("Property test failed: ...")` re-raise.

use std::backtrace::{Backtrace, BacktraceStatus};
use std::cell::{Cell, RefCell};
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::antithesis::TestLocation;
use crate::backend::{DataSource, Exploration, Failure, TestCaseResult, TestRunner};
use crate::control::{
    AssumeFailed, InvalidArgument, LoopDone, StopTest, currently_in_test_context, with_test_context,
};
use crate::runner::{Mode, Settings};
use crate::test_case::TestCase;

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

/// Placeholder thread/location/backtrace tuple for the (defensive) case
/// where the cross-backend panic hook didn't capture info for a panic —
/// e.g. if [`init_panic_hook`] wasn't called before the test ran, or
/// if the panic originated outside the `with_test_context` window.
/// `drive` always installs the hook before calling [`run_test_case`],
/// so this fallback isn't reached from the production path; isolating
/// it lets us cover the placeholder construction without a contrived
/// hook-bypass setup.
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
pub(crate) fn run_test_case(
    data_source: Box<dyn DataSource + Send + Sync>,
    test_fn: &mut dyn FnMut(TestCase),
    is_final: bool,
    mode: Mode,
    verbosity: crate::runner::Verbosity,
) -> TestCaseResult {
    let verbose = matches!(
        verbosity,
        crate::runner::Verbosity::Verbose | crate::runner::Verbosity::Debug
    );
    let quiet = verbosity == crate::runner::Verbosity::Quiet;
    // Surface draw/note output — and pay for a backtrace — only when the
    // diagnostic will be shown: a non-quiet final replay, or every test
    // case in verbose mode.
    let should_emit = (is_final && !quiet) || verbose;
    CAPTURE_BACKTRACE.with(|c| c.set(should_emit));

    let tc = TestCase::new(data_source, should_emit, mode);
    let result = with_test_context(|| catch_unwind(AssertUnwindSafe(|| test_fn(tc.clone()))));

    let tc_result = match &result {
        Ok(()) => TestCaseResult::Valid,
        Err(e) if e.downcast_ref::<AssumeFailed>().is_some() => TestCaseResult::Invalid,
        Err(e) if e.downcast_ref::<StopTest>().is_some() => TestCaseResult::Overrun,
        Err(e) if e.downcast_ref::<LoopDone>().is_some() => TestCaseResult::Valid,
        Err(e) => {
            if let Some(InvalidArgument(message)) = e.downcast_ref::<InvalidArgument>() {
                // An invalid-argument (usage) error is a mistake in how the
                // test is configured, not a discovered counterexample: abort
                // the run with the message instead of recording it as
                // `Interesting` and shrinking it.
                std::panic::resume_unwind(Box::new(message.clone()));
            }
            let msg = panic_message(e);
            let (thread_name, thread_id, location, backtrace) =
                take_panic_info().unwrap_or_else(unknown_panic_info);

            let diagnostic =
                render_diagnostic(&thread_name, &thread_id, &location, &msg, &backtrace);
            if is_final && !quiet {
                // The final replay's draws printed live just above; printing
                // the diagnostic immediately after keeps the failure one
                // contiguous block on stderr. The reporter only adds the
                // reproducer line.
                eprint!("{diagnostic}");
            } else if verbose {
                // The sink interface takes whole lines; `diagnostic` ends
                // with a newline, so drop the trailing empty piece.
                for line in diagnostic.trim_end_matches('\n').split('\n') {
                    crate::test_case::emit_verbose_line(line);
                }
            }
            TestCaseResult::Interesting(Failure {
                panic_message: msg,
                origin: format!("Panic at {}", location),
                // `replay_final` attaches the blob on a final replay.
                reproduce_blob: None,
            })
        }
    };

    if verbose {
        emit_verbose_stop_reason(&tc_result);
    }

    tc.mark_complete(&tc_result);

    tc_result
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
/// enabled *and* the failure carries a reproduce blob. A genuine property
/// failure on the native backend always has a blob; `None` is reached for
/// health-check failures (`FilterTooMuch` / `TooSlow` / flaky — no
/// counterexample to encode) and for the backend (no blob at all), in
/// which case there is simply nothing to print.
/// todo: indicate a way to signal a backend failure so we unconditionally print
/// failure blobs when that happens.
fn reproducer_line(settings: &Settings, failure: &crate::backend::Failure) -> Option<String> {
    if !settings.print_blob {
        return None;
    }
    let blob = failure.reproduce_blob.as_ref()?;
    Some(format!(
        "\nTo reproduce this failure, add the attribute below \
         #[hegel::test]:\n    #[hegel::reproduce_failure(\"{blob}\")]"
    ))
}

/// Drive a [`TestRunner`] to completion against the user's test function.
///
/// Installs the cross-backend panic hook, hands the runner a `run_case`
/// callback that wraps each test invocation in [`run_test_case`], and lets
/// it explore (generate + shrink). On a failing run it then replays each
/// counterexample, reporting each failure as its replay completes, and
/// re-raises the closing `panic!`. A [`crate::backend::RunError`] — a
/// failure of the run itself rather than of a test case — panics with the
/// error's message, without the `Property test failed:` framing.
pub(crate) fn drive<R, F>(
    runner: R,
    test_fn: F,
    settings: &Settings,
    database_key: Option<&str>,
    test_location: Option<&TestLocation>,
) where
    R: TestRunner,
    F: FnMut(TestCase),
{
    init_panic_hook();
    let mut test_fn = test_fn;
    let got_interesting = AtomicBool::new(false);
    let mode = settings.mode;
    let verbosity = settings.verbosity;
    let mut run_case = |backend: Box<dyn DataSource + Send + Sync>, is_final: bool| {
        let tc_result = run_test_case(backend, &mut test_fn, is_final, mode, verbosity);
        if matches!(&tc_result, TestCaseResult::Interesting(_)) {
            got_interesting.store(true, Ordering::SeqCst);
        }
    };

    let exploration = runner.explore(settings, database_key, &mut run_case);

    let test_failed =
        !matches!(exploration, Ok(Exploration::Passed)) || got_interesting.load(Ordering::SeqCst);

    crate::antithesis::require_antithesis_feature(
        crate::antithesis::is_running_in_antithesis(),
        cfg!(feature = "antithesis"),
    );

    #[cfg(feature = "antithesis")]
    // nocov start
    if crate::antithesis::is_running_in_antithesis() {
        if let Some(loc) = test_location {
            crate::antithesis::emit_assertion(loc, !test_failed);
        }
    }
    // nocov end
    let _ = test_location;

    if !test_failed {
        return;
    }

    let quiet = verbosity == crate::runner::Verbosity::Quiet;

    let counterexamples = match exploration {
        // The run itself failed — health check, nondeterminism. Not a test
        // failure, so it gets the error's own message, not the
        // `Property test failed:` framing.
        Err(error) => panic!("{error}"),
        // `got_interesting` was set but the runner reported a clean pass —
        // e.g. an aborted mid-draw test case.  Preserve the legacy generic
        // panic.
        Ok(Exploration::Passed) => panic!("Property test failed: unknown"),
        // A failure with no counterexample to replay (`Mode::SingleTestCase`
        // ran its one test case as its own final, printing its diagnostic
        // at the catch site): report it directly.
        Ok(Exploration::Failed(failure)) => {
            if let Some(line) = reproducer_line(settings, &failure) {
                eprintln!("{line}");
            }
            panic!("Property test failed: {}", failure.panic_message);
        }
        Ok(Exploration::Counterexamples(counterexamples)) => counterexamples,
    };

    // Replay each counterexample with `is_final = true`. Each replay prints
    // its draws live and its diagnostic at the catch site, so each failure
    // reads as one block; only the reproducer line is added here. The count
    // headline has to be eprinted before the replays — the closing
    // `panic!`'s message is only rendered by the panic hook, after
    // everything else.
    let multiple = counterexamples.len() > 1;
    if multiple && !quiet {
        eprintln!(
            "Property-based test failed with {} distinct failures.",
            counterexamples.len()
        );
    }
    let mut reported: Vec<Failure> = Vec::new();
    for counterexample in counterexamples {
        if multiple && !quiet {
            eprintln!();
        }
        // A counterexample that stopped failing between discovery and
        // replay is a run error (flaky test / stale blob), which ends the
        // run on the spot.
        let failure = match runner.replay_final(counterexample, &mut run_case) {
            Ok(failure) => failure,
            Err(error) => panic!("{error}"),
        };
        if let Some(line) = reproducer_line(settings, &failure) {
            eprintln!("{line}");
        }
        reported.push(failure);
    }

    match reported.as_slice() {
        // Defensive: an empty counterexample list (no runner produces one
        // today) falls through to the legacy generic panic.
        [] => panic!("Property test failed: unknown"),
        // Single-failure path: keep the original panic shape so test
        // harnesses that pattern-match on `"Property test failed: <msg>"`
        // (e.g. `Minimal::run` in `tests/common/utils.rs`) keep working.
        [failure] => panic!("Property test failed: {}", failure.panic_message),
        many => panic!(
            "Property-based test failed with {} distinct failures.",
            many.len()
        ),
    }
}

#[cfg(test)]
#[path = "../tests/embedded/run_lifecycle_tests.rs"]
mod tests;

//! Shared per-test-case execution lifecycle.
//!
//! Provides the panic hook, the `run_test_case` wrapper that catches a single
//! test body's panic and converts it into a [`TestCaseResult`] (or re-raises
//! it immediately when the panic originates inside hegel's own source), and
//! the `drive` function that takes a [`TestRunner`] implementation, hands it
//! a `run_case` callback, and surfaces the run-level result.
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
use std::sync::Once;

use crate::antithesis::TestLocation;
use crate::backend::{DataSource, Exploration, Failure, TestCaseResult, TestRunner};
use crate::control::{
    AssumeFailed, InternalError, InvalidArgument, LoopDone, StopTest, currently_in_test_context,
    with_test_context,
};
use crate::settings::{Mode, Settings, Verbosity};
use crate::test_case::TestCase;

static PANIC_HOOK_INIT: Once = Once::new();

/// Information about a panic captured during test execution.
#[doc(hidden)]
#[derive(Debug)]
pub struct PanicInfo {
    pub thread_name: String,
    pub thread_id: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub backtrace: Backtrace,
}

impl PanicInfo {
    pub(crate) fn location(&self) -> String {
        format!("{}:{}:{}", self.file, self.line, self.column)
    }
}

thread_local! {
    /// `PanicInfo` captured by the panic hook for the most recent panic
    /// raised inside a test context. The hook overwrites this on every
    /// panic; callers consume it with [`take_panic_info`] right after
    /// `catch_unwind` returns.
    static LAST_PANIC_INFO: RefCell<Option<PanicInfo>> = const { RefCell::new(None) };

    /// Whether the panic hook should pay to capture a backtrace for the next
    /// panic on this thread. Set by [`run_test_case`] to `should_emit`
    /// (`(is_final && !quiet) || verbose`) — i.e. only when the resulting
    /// diagnostic will actually be shown. Capturing (and, under
    /// `RUST_BACKTRACE`, symbolizing) a backtrace for every discarded shrink
    /// probe is the dominant cost of failing-heavy property runs, and is far
    /// worse on Windows. Panics that originate from hegel's own source
    /// bypass this gate: they are fatal internal errors whose backtrace
    /// must survive to the re-raise.
    static CAPTURE_BACKTRACE: Cell<bool> = const { Cell::new(false) };
}

fn take_panic_info() -> Option<PanicInfo> {
    LAST_PANIC_INFO.with(|info| info.borrow_mut().take())
}

/// Install the cross-backend panic hook on first call.
///
/// Idempotent across all backends: the hook captures the location for any
/// panic raised inside a test context (so [`run_test_case`] can read it after
/// `catch_unwind`), and forwards everything else to the previous hook
/// unchanged. A backtrace is captured only when [`CAPTURE_BACKTRACE`] is
/// set or the panic originates from hegel's own source (a fatal internal
/// error whose backtrace must survive to the re-raise — see
/// [`is_hegel_file`]). Control-flow unwinds (a rejected assumption,
/// out-of-data, ...) never reach any hook at all — they are raised via
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
            let loc = info.location().expect(
                "PanicHookInfo.location() returned None. This should never happen - please open an issue!"
            );
            let file = loc.file().to_string();
            let line = loc.line();
            let column = loc.column();
            // Only capture (and symbolize) a backtrace when the diagnostic
            // will actually be shown. Hegel-internal panics always capture:
            // they re-raise as fatal internal errors whose original
            // backtrace must be preserved.
            let backtrace = if CAPTURE_BACKTRACE.get() || is_hegel_file(&file) {
                Backtrace::capture()
            } else {
                Backtrace::disabled()
            };

            LAST_PANIC_INFO.with(|l| {
                *l.borrow_mut() = Some(PanicInfo {
                    thread_name,
                    thread_id,
                    file,
                    line,
                    column,
                    backtrace,
                })
            });
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

/// Placeholder `PanicInfo` used when the panic hook captured nothing for a
/// caught panic. This *is* reached in production: a genuine panic on a
/// spawned thread lands its capture in that thread's `LAST_PANIC_INFO`,
/// and the `join().unwrap()` that propagates it uses `resume_unwind`,
/// which skips the hook on the joining thread — so the lifecycle finds
/// nothing here. One consequence is that every such failure shares the
/// origin `"Panic at <unknown>"`, merging distinct threaded bugs into one
/// counterexample; fixing that needs cross-thread capture, which is
/// deferred until there is structured concurrency support to hang it on.
pub(crate) fn unknown_panic_info() -> PanicInfo {
    PanicInfo {
        thread_name: "<unknown>".to_string(),
        thread_id: "?".to_string(),
        file: "<unknown>".to_string(),
        line: 0,
        column: 0,
        backtrace: Backtrace::disabled(),
    }
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

const HEGEL_CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// Source directories within the hegel crate that count as "internal".
/// Only panics from these directories are treated as hegel errors; panics
/// from `tests/` or other paths are treated as user test failures.
const HEGEL_SRC_DIRS: &[&str] = &["src", "hegel-macros/src"];

/// Returns true if `file_path` (as captured from a panic location) points
/// at hegel's own source code (i.e. a panic from inside the library
/// itself, not the user's test body).
pub(crate) fn is_hegel_file(file_path: &str) -> bool {
    let path = std::path::Path::new(file_path);

    // Get the path relative to the hegel crate root.
    let relative = if path.is_absolute() {
        match path.strip_prefix(HEGEL_CRATE_DIR) {
            Ok(rel) => rel.to_path_buf(),
            Err(_) => return false,
        }
    } else {
        // When running inside hegel's own test binary, panic locations use
        // paths relative to the crate root. Verify the file exists there.
        if !std::path::Path::new(HEGEL_CRATE_DIR).join(path).is_file() {
            return false;
        }
        path.to_path_buf()
    };

    // Normalize the relative path (resolve ".." components) and check it
    // lives under a hegel source directory, not under tests/ or elsewhere.
    let mut normalized = std::path::PathBuf::new();
    for component in relative.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            c => normalized.push(c),
        }
    }

    HEGEL_SRC_DIRS.iter().any(|dir| normalized.starts_with(dir))
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
/// A panic whose location lies inside hegel's own source (see
/// [`is_hegel_file`]) is a bug in hegel, not a property failure: it is
/// re-raised immediately with its original location and backtrace — no
/// shrinking, no `Property test failed` wrapping, and no
/// [`TestCase::mark_complete`].
///
/// Also returns the caught panic payload for an `Interesting` result, so a
/// final replay's caller can re-raise the test's *own* panic as the run's
/// closing unwind instead of synthesizing one.
pub(crate) fn run_test_case(
    data_source: Box<dyn DataSource + Send + Sync>,
    test_fn: &mut dyn FnMut(TestCase),
    is_final: bool,
    mode: Mode,
    verbosity: Verbosity,
) -> (TestCaseResult, Option<Box<dyn std::any::Any + Send>>) {
    let verbose = matches!(verbosity, Verbosity::Verbose | Verbosity::Debug);
    let quiet = verbosity == Verbosity::Quiet;
    // Surface draw/note output — and pay for a backtrace — only when the
    // diagnostic will be shown: a non-quiet final replay, or every test
    // case in verbose mode.
    let should_emit = (is_final && !quiet) || verbose;
    CAPTURE_BACKTRACE.with(|c| c.set(should_emit));

    let tc = TestCase::new(data_source, should_emit, mode);
    let result = with_test_context(|| catch_unwind(AssertUnwindSafe(|| test_fn(tc.clone()))));

    let (tc_result, payload) = match result {
        Ok(()) => (TestCaseResult::Valid, None),
        Err(e) if e.downcast_ref::<AssumeFailed>().is_some() => (TestCaseResult::Invalid, None),
        Err(e) if e.downcast_ref::<StopTest>().is_some() => (TestCaseResult::Overrun, None),
        Err(e) if e.downcast_ref::<LoopDone>().is_some() => (TestCaseResult::Valid, None),
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
            let panic_info = take_panic_info().unwrap_or_else(unknown_panic_info);

            // Immediately re-raise panics originating from inside hegel
            // itself as internal errors rather than user test failures —
            // they should never be shrunk or swallowed.
            if is_hegel_file(&panic_info.file) {
                panic!("{}", render_internal_error(&msg, &panic_info));
            }

            let location = panic_info.location();
            let diagnostic = render_diagnostic(
                &panic_info.thread_name,
                &panic_info.thread_id,
                &location,
                &msg,
                &panic_info.backtrace,
            );
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
            let failure = TestCaseResult::Interesting(Failure {
                panic_message: msg,
                origin: format!("Panic at {}", location),
                // `replay_final` attaches the blob on a final replay.
                reproduce_blob: None,
            });
            (failure, Some(e))
        }
    };

    if verbose {
        emit_verbose_stop_reason(&tc_result);
    }

    tc.mark_complete(&tc_result);

    (tc_result, payload)
}

/// Append a captured backtrace (and an optional short-form note) to `out`.
///
/// Shared between [`render_diagnostic`] (panic diagnostics) and
/// [`render_internal_error`] (hegel-internal panic propagation). Both
/// callers need the same "if captured: format with current RUST_BACKTRACE
/// setting" logic; factoring it out keeps a single nocov region rather
/// than duplicating the env-var-conditional block at every call site.
fn append_captured_backtrace(
    out: &mut String,
    backtrace: &Backtrace,
    header: &str,
    include_short_note: bool,
) {
    // nocov start
    if backtrace.status() == BacktraceStatus::Captured {
        let is_full = std::env::var("RUST_BACKTRACE").is_ok_and(|v| v == "full");
        let formatted = format_backtrace(backtrace, is_full);
        out.push_str(&format!("{header}{formatted}\n"));
        if include_short_note && !is_full {
            out.push_str("note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.\n");
        }
    }
    // nocov end
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
    append_captured_backtrace(&mut out, backtrace, "stack backtrace:\n", true);
    out
}

/// Render an internal-error message + backtrace block for the `panic!` we
/// emit when a hegel-internal panic short-circuits a test run.
fn render_internal_error(panic_message: &str, panic_info: &PanicInfo) -> String {
    let mut msg = format!(
        "hegel internal error at {}:\n{}\n",
        panic_info.location(),
        panic_message,
    );
    append_captured_backtrace(
        &mut msg,
        &panic_info.backtrace,
        "\noriginal backtrace:\n",
        false,
    );
    msg
}

/// The copy-pasteable reproduce failure text to append after a failure's diagnostic,
/// or `None` when nothing should be printed.
///
/// `Some` only when [`Settings::print_blob`](crate::Settings::print_blob) is
/// enabled *and* the failure carries a reproduce blob. A replayed
/// counterexample always has one; a blobless failure (e.g.
/// `Mode::SingleTestCase`, whose one random case has no shrunk choice
/// sequence to encode) prints nothing.
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
/// ends the run by re-raising the failing test's own panic (or, for
/// several distinct bugs, a panic carrying the failure count). A
/// [`crate::backend::RunError`] — a failure of the run itself rather than
/// of a test case — panics with the error's message instead.
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
    require_antithesis_feature();
    let mut test_fn = test_fn;
    let mode = settings.mode;
    let verbosity = settings.verbosity;

    let exploration = {
        let mut explore_case = |backend: Box<dyn DataSource + Send + Sync>| {
            run_test_case(backend, &mut test_fn, false, mode, verbosity);
        };
        runner.explore(settings, database_key, &mut explore_case)
    };

    let test_failed = !matches!(exploration, Ok(Exploration::Passed));
    emit_antithesis_assertion(test_failed, test_location);

    if !test_failed {
        return;
    }

    let quiet = verbosity == Verbosity::Quiet;

    let counterexamples = match exploration {
        // The run itself failed — health check, nondeterminism. Not a test
        // failure, so it gets the error's own message, not the
        // `Property test failed:` framing.
        Err(error) => panic!("{error}"),
        // `test_failed` is exactly `!Passed`, and a passing run returned
        // above.
        Ok(Exploration::Passed) => unreachable!(),
        Ok(Exploration::Counterexamples(counterexamples)) => counterexamples,
    };

    // Replay each counterexample as a final test case. Each replay prints
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
    let last_payload: RefCell<Option<Box<dyn std::any::Any + Send>>> = RefCell::new(None);
    let mut final_case = |backend: Box<dyn DataSource + Send + Sync>| {
        let (_, payload) = run_test_case(backend, &mut test_fn, true, mode, verbosity);
        *last_payload.borrow_mut() = payload;
    };
    let mut reported: Vec<String> = Vec::new();
    for counterexample in counterexamples {
        if multiple && !quiet {
            eprintln!();
        }
        // A counterexample that stopped failing between discovery and
        // replay is a run error (flaky test / stale blob), which ends the
        // run on the spot.
        let failure = match runner.replay_final(counterexample, &mut final_case) {
            Ok(failure) => failure,
            Err(error) => panic!("{error}"),
        };
        if let Some(line) = reproducer_line(settings, &failure) {
            eprintln!("{line}");
        }
        reported.push(failure.panic_message);
    }

    // The report is complete; end the run by re-raising. `resume_unwind`
    // skips the panic hook, so nothing prints twice — the unwind is purely
    // the run's programmatic result.
    match reported.as_slice() {
        // Defensive: an empty counterexample list (no runner produces one
        // today) falls through to the legacy generic panic.
        [] => panic!("Property test failed: unknown"),
        // Single-failure path: the run fails with the test's *own* panic,
        // payload intact — `should_panic(expected = ...)` and `catch_unwind`
        // consumers see exactly what the test raised. A runner whose final
        // replay produced no payload (it didn't execute the test body) falls
        // back to a synthetic panic carrying the recorded message.
        [message] => match last_payload.borrow_mut().take() {
            Some(payload) => std::panic::resume_unwind(payload),
            None => panic!("Property test failed: {}", message),
        },
        // Multi-failure path: there is no single panic to re-raise, so the
        // run fails with the count (already eprinted as the headline).
        many => std::panic::resume_unwind(Box::new(format!(
            "Property-based test failed with {} distinct failures.",
            many.len()
        ))),
    }
}

/// Run `Mode::SingleTestCase`: one test case, final from the start, with
/// its diagnostic printed at the catch site. A single test case is not a
/// property-test run — there is no exploration, shrinking, or replay — so
/// it bypasses the [`TestRunner`] machinery entirely.
pub(crate) fn drive_single<F>(
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
    let last_payload: RefCell<Option<Box<dyn std::any::Any + Send>>> = RefCell::new(None);
    let failure =
        crate::native::test_runner::run_single_case(settings, database_key, &mut |backend| {
            let (_, payload) = run_test_case(
                backend,
                &mut test_fn,
                true,
                settings.mode,
                settings.verbosity,
            );
            *last_payload.borrow_mut() = payload;
        });

    emit_antithesis_assertion(failure.is_some(), test_location);

    // No reproducer line: a single random test case has no shrunk choice
    // sequence to encode, so its failure never carries a blob. The run ends
    // by re-raising the test's own panic.
    if failure.is_none() {
        return;
    }
    match last_payload.borrow_mut().take() {
        Some(payload) => std::panic::resume_unwind(payload),
        // A single-case failure always comes from a panic this run caught.
        None => unreachable!(),
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

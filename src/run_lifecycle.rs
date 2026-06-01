//! Shared per-test-case execution lifecycle.
//!
//! Provides the panic hook, the `run_test_case` wrapper that catches a single
//! test body's panic and converts it into a [`TestCaseResult`] (or an
//! [`InternalError`] when the panic originates inside hegel's own source),
//! and the `drive` function that takes a [`TestRunner`] implementation,
//! hands it a `run_case` callback, and surfaces the run-level result.
//!
//! Both the server-protocol backend (`crate::server::session::ServerTestRunner`)
//! and the native engine backend (`crate::native::test_runner::NativeTestRunner`)
//! plug into this lifecycle. Each backend is free to do whatever it likes
//! inside its `TestRunner::run` to decide which test cases to run; the
//! lifecycle owns everything that's identical across backends — installing
//! the panic hook, wrapping each test body with `catch_unwind` plus
//! `mark_complete`, the antithesis integration, and the final
//! `panic!("Property test failed: ...")` re-raise.

use std::backtrace::{Backtrace, BacktraceStatus};
use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::antithesis::TestLocation;
use crate::backend::{DataSource, Failure, TestCaseResult, TestRunner};
use crate::control::{currently_in_test_context, with_test_context};
use crate::settings::{Mode, Phase, Settings, Verbosity};
use crate::test_case::{
    ASSUME_FAIL_STRING, INVALID_ARGUMENT_PREFIX, LOOP_DONE_STRING, STOP_TEST_STRING, TestCase,
};

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

/// A hegel-internal panic. When a panic captured during a test originates
/// from hegel's own source files (rather than the user's test body), the
/// lifecycle short-circuits the per-test-case flow and bubbles this up so
/// the top-level driver can re-raise it as a hegel error rather than a
/// user-test failure.
#[doc(hidden)]
#[derive(Debug)]
pub struct InternalError {
    pub panic_message: String,
    pub panic_info: PanicInfo,
}

thread_local! {
    /// `PanicInfo` captured by the panic hook for the most recent panic
    /// raised inside a test context. The hook overwrites this on every
    /// panic; callers consume it with [`take_panic_info`] right after
    /// `catch_unwind` returns.
    static LAST_PANIC_INFO: RefCell<Option<PanicInfo>> = const { RefCell::new(None) };
}

fn take_panic_info() -> Option<PanicInfo> {
    LAST_PANIC_INFO.with(|info| info.borrow_mut().take())
}

/// Install the cross-backend panic hook on first call.
///
/// Idempotent across all backends: the hook captures location + backtrace
/// for any panic raised inside a test context (so [`run_test_case`] can read
/// the location after `catch_unwind`), and forwards everything else to the
/// previous hook unchanged. Without the suppression, every shrinker probe
/// would print a `thread 'main' panicked` line to stderr and the user-visible
/// output would be unreadable.
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
            let loc = info.location().expect(
                "PanicHookInfo.location() returned None. This should never happen - please open an issue!"
            );
            let file = loc.file().to_string();
            let line = loc.line();
            let column = loc.column();
            let backtrace = Backtrace::capture();

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

/// Placeholder `PanicInfo` used when the cross-backend panic hook didn't
/// capture info for a panic — e.g. if [`init_panic_hook`] wasn't called
/// before the test ran, or if the panic originated outside the
/// `with_test_context` window. `drive` always installs the hook before
/// calling [`run_test_case`], so this fallback isn't reached from the
/// production path; isolating it lets us cover the placeholder
/// construction without a contrived hook-bypass setup.
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

/// Extract a string message from a panic payload.
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
/// any panic and translating it to a [`TestCaseResult`] (or
/// [`InternalError`] when the panic originates from hegel itself).
///
/// On the user-panic path the panic site is captured as a `file:line:col`
/// string and stored on the [`Failure`] so per-origin shrinking can key on
/// it. On the hegel-internal panic path no [`TestCase::mark_complete`] is
/// sent — the caller is expected to re-raise the error immediately.
pub(crate) fn run_test_case(
    data_source: Box<dyn DataSource + Send + Sync>,
    test_fn: &mut dyn FnMut(TestCase),
    is_final: bool,
    mode: Mode,
    verbosity: Verbosity,
) -> Result<TestCaseResult, Box<InternalError>> {
    let verbose = matches!(verbosity, Verbosity::Verbose | Verbosity::Debug);
    let tc = TestCase::new(data_source, is_final, mode, verbose);
    let result = with_test_context(|| catch_unwind(AssertUnwindSafe(|| test_fn(tc.clone()))));

    let tc_result = match &result {
        Ok(()) => TestCaseResult::Valid,
        Err(e) => {
            let msg = panic_message(e);
            if msg == ASSUME_FAIL_STRING {
                TestCaseResult::Invalid
            } else if msg == STOP_TEST_STRING {
                TestCaseResult::Overrun
            } else if msg == LOOP_DONE_STRING {
                TestCaseResult::Valid
            } else if let Some(stripped) = msg.strip_prefix(INVALID_ARGUMENT_PREFIX) {
                // An invalid-argument (usage) error is a mistake in how the
                // test is configured, not a discovered counterexample: abort
                // the run with the message instead of recording it as
                // `Interesting` and shrinking it.
                std::panic::resume_unwind(Box::new(stripped.to_string()));
            } else {
                let panic_info = take_panic_info().unwrap_or_else(unknown_panic_info);

                // Immediately propagate panics originating from inside
                // hegel itself as internal errors rather than user test
                // failures — they should never be shrunk or swallowed.
                if is_hegel_file(&panic_info.file) {
                    return Err(Box::new(InternalError {
                        panic_message: msg,
                        panic_info,
                    }));
                }

                let location = panic_info.location();
                let diagnostic = render_diagnostic(
                    &panic_info.thread_name,
                    &panic_info.thread_id,
                    &location,
                    &msg,
                    &panic_info.backtrace,
                );
                TestCaseResult::Interesting(Failure {
                    panic_message: msg,
                    diagnostic,
                    origin: format!("Panic at {}", location),
                })
            }
        }
    };

    if verbose {
        emit_verbose_test_case_outcome(&tc_result, is_final);
    }

    tc.mark_complete(&tc_result);

    let _ = is_final;
    Ok(tc_result)
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

/// Print a per-test-case line describing why this test case stopped, and
/// — for genuine panics on non-final test cases — the full panic
/// diagnostic. Final-replay panics are already covered by the run-level
/// summary in [`drive`]; printing them here too would just duplicate the
/// block.
fn emit_verbose_test_case_outcome(result: &TestCaseResult, is_final: bool) {
    match result {
        TestCaseResult::Invalid => {
            crate::test_case::emit_verbose_line("Test case stopped: failed assumption");
        }
        TestCaseResult::Overrun => {
            crate::test_case::emit_verbose_line("Test case stopped: out of data");
        }
        TestCaseResult::Interesting(failure) if !is_final => {
            // `diagnostic` already ends with a newline, so use `eprint!`-
            // style emission (no extra newline). The sink interface only
            // takes whole lines, so split on '\n' and drop the trailing
            // empty piece if any.
            for line in failure.diagnostic.trim_end_matches('\n').split('\n') {
                crate::test_case::emit_verbose_line(line);
            }
        }
        TestCaseResult::Valid | TestCaseResult::Interesting(_) => {}
    }
}

/// Render the per-failure diagnostic block previously emitted inline.
/// Mirrors the default Rust panic-handler output so each row in the
/// multi-failure report looks like a stand-alone test failure.
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

/// Render an internal-error message + backtrace block for the
/// `panic!` we emit when a hegel-internal panic short-circuits a test run.
fn render_internal_error(err: &InternalError) -> String {
    let mut msg = format!(
        "hegel internal error at {}:\n{}\n",
        err.panic_info.location(),
        err.panic_message,
    );
    append_captured_backtrace(
        &mut msg,
        &err.panic_info.backtrace,
        "\noriginal backtrace:\n",
        false,
    );
    msg
}

/// Drive a [`TestRunner`] to completion against the user's test function.
///
/// Installs the cross-backend panic hook, hands the runner a `run_case`
/// callback that wraps each test invocation in [`run_test_case`], handles
/// the antithesis integration, and re-raises a final `panic!` if any test
/// case failed.
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
    if !settings.phases.contains(&Phase::Generate) {
        return;
    }

    let mut test_fn = test_fn;
    let got_interesting = AtomicBool::new(false);
    let mode = settings.mode;
    let verbosity = settings.verbosity;
    let result = runner.run(settings, database_key, &mut |backend, is_final| {
        match run_test_case(backend, &mut test_fn, is_final, mode, verbosity) {
            Err(internal_err) => {
                // Re-raise hegel-internal panics immediately as their own
                // dedicated panic — no shrinking, no "Property test
                // failed" wrapping.
                panic!("{}", render_internal_error(&internal_err));
            }
            Ok(tc_result) => {
                if matches!(&tc_result, TestCaseResult::Interesting(_)) {
                    got_interesting.store(true, Ordering::SeqCst);
                }
            }
        }
    });

    let test_failed = !result.passed || got_interesting.load(Ordering::SeqCst);

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

    let quiet = settings.verbosity == Verbosity::Quiet;

    match result.failures.as_slice() {
        // `test_failed` was set but no Failure surfaced — e.g. an aborted
        // mid-draw test case or a backend that reported failure without
        // attaching a Failure.  Preserve the legacy generic panic.
        [] => panic!("Property test failed: unknown"),
        // Single-failure path: keep the original output shape so test
        // harnesses that pattern-match on `"Property test failed: <msg>"`
        // (e.g. `Minimal::run` in `tests/common/utils.rs`) keep working.
        [failure] => {
            if !quiet {
                eprint!("{}", failure.diagnostic);
            }
            panic!("Property test failed: {}", failure.panic_message);
        }
        // Multi-failure path: emit a header, print each replay's
        // diagnostic block in order, then panic with the count so callers
        // see the headline figure rather than just one of the messages.
        failures => {
            let n = failures.len();
            if !quiet {
                eprintln!("Hegel found {} failing test cases:", n);
                for failure in failures {
                    eprint!("{}", failure.diagnostic);
                }
            }
            panic!("Property-based test failed with {} distinct failures.", n);
        }
    }
}

#[cfg(test)]
#[path = "../tests/embedded/run_lifecycle_tests.rs"]
mod tests;

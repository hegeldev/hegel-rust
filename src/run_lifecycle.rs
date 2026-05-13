//! Shared per-test-case execution lifecycle.
//!
//! Provides the panic hook, the `run_test_case` wrapper that catches a single
//! test body's panic and converts it into a [`TestCaseResult`], and the
//! `drive` function that takes a [`TestRunner`] implementation, hands it a
//! `run_case` callback, and surfaces the run-level result.
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
use crate::runner::{Mode, Phase, Settings};
use crate::test_case::{ASSUME_FAIL_STRING, LOOP_DONE_STRING, STOP_TEST_STRING, TestCase};

static PANIC_HOOK_INIT: Once = Once::new();

thread_local! {
    /// `(thread_name, thread_id, location, backtrace)` captured by the panic
    /// hook for the most recent panic raised inside a test context. The
    /// hook overwrites this on every panic; callers consume it with
    /// [`take_panic_info`] right after `catch_unwind` returns.
    static LAST_PANIC_INFO: RefCell<Option<(String, String, String, Backtrace)>> =
        const { RefCell::new(None) };
}

fn take_panic_info() -> Option<(String, String, String, Backtrace)> {
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
            let location = info
                .location()
                .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
                .unwrap_or_else(|| "<unknown>".to_string());
            let backtrace = Backtrace::capture();

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
pub(crate) fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
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
/// [`TestCase::mark_complete`]: that is the single cross-backend channel
/// for per-test-case results.  Both backends consume it the same way (the
/// server forwards it to Hypothesis; the native engine reads it back off
/// the data-source handle); neither backend looks at the return value of
/// this function for the outcome.  On the `Interesting` path the panic
/// site is captured as a `file:line:col` string and stored on the
/// [`Failure`] so per-origin shrinking can key on it.
pub(crate) fn run_test_case(
    data_source: Box<dyn DataSource>,
    test_fn: &mut dyn FnMut(TestCase),
    is_final: bool,
    mode: Mode,
    verbosity: crate::runner::Verbosity,
) -> TestCaseResult {
    let tc = TestCase::new(data_source, is_final, mode);
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
            } else {
                let (thread_name, thread_id, location, backtrace) =
                    take_panic_info().unwrap_or_else(unknown_panic_info);

                let diagnostic =
                    render_diagnostic(&thread_name, &thread_id, &location, &msg, &backtrace);
                TestCaseResult::Interesting(Failure {
                    panic_message: msg,
                    diagnostic,
                    origin: format!("Panic at {}", location),
                })
            }
        }
    };

    tc.mark_complete(&tc_result);

    let _ = (is_final, verbosity);
    tc_result
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
        let tc_result = run_test_case(backend, &mut test_fn, is_final, mode, verbosity);
        if matches!(&tc_result, TestCaseResult::Interesting(_)) {
            got_interesting.store(true, Ordering::SeqCst);
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

    let quiet = verbosity == crate::runner::Verbosity::Quiet;

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

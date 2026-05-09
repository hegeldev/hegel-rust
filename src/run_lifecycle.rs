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
use crate::backend::{DataSource, TestCaseResult, TestRunner};
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

    /// One-shot flag: when set, the panic hook silently swallows the next
    /// panic's stderr output. Used by [`drive`] to re-raise a `"Property
    /// test failed: <msg>"` panic for `catch_unwind` callers and `cargo
    /// test`'s benefit, without duplicating the user-facing diagnostic
    /// that the final replay already printed. Cleared by the hook on read.
    static SUPPRESS_NEXT_PANIC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn take_panic_info() -> Option<(String, String, String, Backtrace)> {
    LAST_PANIC_INFO.with(|info| info.borrow_mut().take())
}

/// Take just the captured `file:line:col` location for the most recent
/// panic raised inside a test context, leaving the rest of
/// `LAST_PANIC_INFO` (thread name, id, backtrace) intact for downstream
/// consumers. Used by `NativeConjectureRunner::run_test_fn` to key
/// `InterestingOrigin` on the panic site so two distinct `assert!` sites
/// with the same payload string don't collapse into a single origin.
pub(crate) fn take_panic_location() -> Option<String> {
    LAST_PANIC_INFO.with(|info| {
        let mut slot = info.borrow_mut();
        slot.as_mut().map(|(_, _, location, _)| std::mem::take(location))
    })
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
            // `drive` sets this when re-raising the bookkeeping
            // `"Property test failed: ..."` panic. Swallow stderr for
            // exactly that one panic; the diagnostic was already printed.
            if SUPPRESS_NEXT_PANIC.with(|c| c.replace(false)) {
                return;
            }

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
        if trimmed
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
        {
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

/// Extract a string message from a panic payload.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
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
/// Also reports the outcome back to the data source via
/// [`TestCase::mark_complete`] so backends that need to forward outcome
/// information (the server protocol speaks to Hypothesis) can. On the
/// `Interesting` path, formats the panic's captured `file:line:col` into
/// the cross-backend `origin` string that `mark_complete` carries; this is
/// also surfaced inside [`TestCaseResult::Interesting`] so a [`TestRunner`]
/// implementation that wants to key per-origin shrinking (the native
/// engine's multi-origin tracking) can read it directly without poking at
/// thread-locals.
pub(crate) fn run_test_case(
    data_source: Box<dyn DataSource>,
    test_fn: &mut dyn FnMut(TestCase),
    is_final: bool,
    mode: Mode,
) -> TestCaseResult {
    let tc = TestCase::new(data_source, is_final, mode);
    let result = with_test_context(|| catch_unwind(AssertUnwindSafe(|| test_fn(tc.clone()))));

    let (tc_result, origin) = match &result {
        Ok(()) => (TestCaseResult::Valid, None),
        Err(e) => {
            let msg = panic_message(e);
            if msg == ASSUME_FAIL_STRING {
                (TestCaseResult::Invalid, None)
            } else if msg == STOP_TEST_STRING {
                (TestCaseResult::Overrun, None)
            } else if msg == LOOP_DONE_STRING {
                (TestCaseResult::Valid, None)
            } else {
                let (thread_name, thread_id, location, backtrace) =
                    take_panic_info().unwrap_or_else(|| {
                        (
                            "<unknown>".to_string(),
                            "?".to_string(),
                            "<unknown>".to_string(),
                            Backtrace::disabled(),
                        )
                    });

                if is_final {
                    eprintln!(
                        "thread '{}' ({}) panicked at {}:",
                        thread_name, thread_id, location
                    );
                    eprintln!("{}", msg);
                    if backtrace.status() == BacktraceStatus::Captured {
                        let is_full = std::env::var("RUST_BACKTRACE")
                            .map(|v| v == "full")
                            .unwrap_or(false);
                        let formatted = format_backtrace(&backtrace, is_full);
                        eprintln!("stack backtrace:\n{}", formatted);
                        if !is_full {
                            eprintln!(
                                "note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace."
                            );
                        }
                    }
                }

                let origin = format!("Panic at {}", location);
                (
                    TestCaseResult::Interesting {
                        panic_message: msg,
                        origin: Some(origin.clone()),
                    },
                    Some(origin),
                )
            }
        }
    };

    if !tc.test_aborted() {
        let status = match &tc_result {
            TestCaseResult::Valid => "VALID",
            TestCaseResult::Invalid | TestCaseResult::Overrun => "INVALID",
            TestCaseResult::Interesting { .. } => "INTERESTING",
        };
        tc.mark_complete(status, origin.as_deref());
    }

    tc_result
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
    let result = runner.run(settings, database_key, &mut |backend, is_final| {
        let tc_result = run_test_case(backend, &mut test_fn, is_final, mode);
        if matches!(&tc_result, TestCaseResult::Interesting { .. }) {
            got_interesting.store(true, Ordering::SeqCst);
        }
        tc_result
    });

    let test_failed = !result.passed || got_interesting.load(Ordering::SeqCst);

    crate::antithesis::require_antithesis_feature(
        crate::antithesis::is_running_in_antithesis(),
        cfg!(feature = "antithesis"),
    );

    #[cfg(feature = "antithesis")]
    if crate::antithesis::is_running_in_antithesis() {
        if let Some(loc) = test_location {
            crate::antithesis::emit_assertion(loc, !test_failed);
        }
    }
    let _ = test_location;

    if test_failed {
        let msg = result.failure_message.as_deref().unwrap_or("unknown");
        // The user-facing diagnostic (location, falsifying example, original
        // panic message, backtrace) was already printed by `run_test_case`
        // during the final replay above. We need three things from this
        // re-raise:
        //   - stderr to say something so a human sees the run failed —
        //     printed manually as a one-line footer below;
        //   - the panic payload to be exactly `"Property test failed: <msg>"`
        //     so `catch_unwind` callers like `Minimal::run` in
        //     `tests/common/utils.rs` can pattern-match expected failures;
        //   - `cargo test` to see the test panic so it marks it failed.
        // Suppressing the next panic's stderr print keeps the original
        // message from being duplicated (Rust's default hook would otherwise
        // print `"Property test failed: <msg>"` to stderr verbatim).
        eprintln!("Property test failed.");
        SUPPRESS_NEXT_PANIC.with(|c| c.set(true));
        panic!("Property test failed: {}", msg);
    }
}

use crate::antithesis::TestLocation;
use crate::antithesis::is_running_in_antithesis;
use crate::backend::{DataSource, TestCaseResult, TestRunner};
use crate::control::{currently_in_test_context, with_test_context};
use crate::runner::Settings;
use crate::test_case::TestCase;
use crate::test_case::{ASSUME_FAIL_STRING, LOOP_DONE_STRING, STOP_TEST_STRING};

use std::backtrace::{Backtrace, BacktraceStatus};
use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

static PANIC_HOOK_INIT: Once = Once::new();

thread_local! {
    /// (thread_name, thread_id, location, backtrace)
    static LAST_PANIC_INFO: RefCell<Option<(String, String, String, Backtrace)>> = const { RefCell::new(None) };
}

/// (thread_name, thread_id, location, backtrace).
fn take_panic_info() -> Option<(String, String, String, Backtrace)> {
    LAST_PANIC_INFO.with(|info| info.borrow_mut().take())
}

/// Format a backtrace, optionally filtering to "short" format.
///
/// Short format shows only frames between `__rust_end_short_backtrace` and
/// `__rust_begin_short_backtrace` markers, matching the default Rust panic handler.
/// Frame numbers are renumbered to start at 0.
// nocov start
fn format_backtrace(bt: &Backtrace, full: bool) -> String {
    let backtrace_str = format!("{}", bt);

    if full {
        return backtrace_str;
    }

    // Filter to short backtrace: keep lines between the markers
    // Frame groups look like:
    //    N: function::name
    //              at /path/to/file.rs:123:45
    let lines: Vec<&str> = backtrace_str.lines().collect();
    let mut start_idx = 0;
    let mut end_idx = lines.len();

    for (i, line) in lines.iter().enumerate() {
        if line.contains("__rust_end_short_backtrace") {
            // Skip past this frame (find the next frame number)
            for (j, next_line) in lines.iter().enumerate().skip(i + 1) {
                if next_line
                    .trim_start()
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                {
                    start_idx = j;
                    break;
                }
            }
        }
        if line.contains("__rust_begin_short_backtrace") {
            // Find the start of this frame (the line with the frame number)
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
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                {
                    end_idx = j;
                    break;
                }
            }
            break;
        }
    }

    // Renumber frames starting at 0
    let filtered: Vec<&str> = lines[start_idx..end_idx].to_vec();
    let mut new_frame_num = 0usize;
    let mut result = Vec::new();

    for line in filtered {
        let trimmed = line.trim_start();
        if trimmed
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            // This is a frame number line like "   8: function_name"
            // Find where the number ends (at the colon)
            if let Some(colon_pos) = trimmed.find(':') {
                let rest = &trimmed[colon_pos..];
                // Preserve original indentation style (right-aligned numbers)
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
// nocov end

// Panic unconditionally prints to stderr, even if it's caught later. This results in
// messy output during shrinking. To avoid this, we replace the panic hook with our
// own that suppresses the printing except for the final replay.
//
// This is called once per process, the first time any hegel test runs.
pub(super) fn init_panic_hook() {
    PANIC_HOOK_INIT.call_once(|| {
        let prev_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !currently_in_test_context() {
                // use actual panic hook outside of tests
                prev_hook(info);
                return;
            }

            let thread = std::thread::current();
            let thread_name = thread.name().unwrap_or("<unnamed>").to_string();
            // ThreadId's debug output is ThreadId(N)
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

fn run_test_case(
    data_source: Box<dyn DataSource>,
    test_fn: &mut dyn FnMut(TestCase),
    is_final: bool,
) -> TestCaseResult {
    let tc = TestCase::new(data_source, is_final);

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
                // `TestCase::repeat` returns `!`, so it exits via this
                // sentinel panic when its loop completes normally. Treat it
                // the same as a no-panic return.
                (TestCaseResult::Valid, None)
            } else {
                // Take panic info - we need location for origin, and print details on final
                let (thread_name, thread_id, location, backtrace) = take_panic_info()
                    .unwrap_or_else(|| {
                        // nocov start
                        (
                            "<unknown>".to_string(),
                            "?".to_string(),
                            "<unknown>".to_string(),
                            Backtrace::disabled(),
                        )
                        // nocov end
                    });

                if is_final {
                    eprintln!(
                        "thread '{}' ({}) panicked at {}:",
                        thread_name, thread_id, location
                    );
                    eprintln!("{}", msg);

                    // nocov start
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
                    // nocov end
                }

                let origin = format!("Panic at {}", location);
                (
                    TestCaseResult::Interesting { panic_message: msg },
                    Some(origin),
                )
            }
        }
    };

    // Send mark_complete via the data source.
    // Skip if test was aborted (StopTest) - the data source already closed.
    if !tc.data_source().test_aborted() {
        let status = match &tc_result {
            TestCaseResult::Valid => "VALID",
            TestCaseResult::Invalid | TestCaseResult::Overrun => "INVALID",
            TestCaseResult::Interesting { .. } => "INTERESTING",
        };
        tc.data_source().mark_complete(status, origin.as_deref());
    }

    tc_result
}

/// Extract a message from a panic payload.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string() // nocov
    }
}

/// Encode a ciborium::Value to CBOR bytes.
pub(super) fn cbor_encode(value: &ciborium::Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).expect("CBOR encoding failed");
    bytes
}

/// Decode CBOR bytes to a ciborium::Value.
pub(super) fn cbor_decode(bytes: &[u8]) -> ciborium::Value {
    ciborium::from_reader(bytes).expect("CBOR decoding failed")
}

pub fn server_run<F>(
    test_fn: F,
    settings: &Settings,
    database_key: Option<&str>,
    test_location: Option<&TestLocation>,
) where
    F: FnMut(TestCase),
{
    init_panic_hook();

    let runner = super::session::ServerTestRunner;
    let mut test_fn = test_fn;
    let got_interesting = AtomicBool::new(false);

    let result = runner.run(settings, database_key, &mut |backend, is_final| {
        let tc_result = run_test_case(backend, &mut test_fn, is_final);
        if matches!(&tc_result, TestCaseResult::Interesting { .. }) {
            got_interesting.store(true, Ordering::SeqCst);
        }
        tc_result
    });

    let test_failed = !result.passed || got_interesting.load(Ordering::SeqCst);

    crate::antithesis::require_antithesis_feature(
        is_running_in_antithesis(),
        cfg!(feature = "antithesis"),
    );

    #[cfg(feature = "antithesis")]
    // nocov start
    if is_running_in_antithesis() {
        if let Some(ref loc) = test_location {
            crate::antithesis::emit_assertion(loc, !test_failed);
        }
    }
    // nocov end
    // Suppress unused-variable warning for the non-antithesis-feature build:
    // test_location is only consumed inside the cfg(feature = "antithesis") block above.
    let _ = test_location;

    if test_failed {
        let msg = result.failure_message.as_deref().unwrap_or("unknown");
        panic!("Property test failed: {}", msg);
    }
}

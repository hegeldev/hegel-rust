//! Embedded tests for `src/run_lifecycle.rs`.

use super::*;
use crate::generators as gs;

#[test]
fn panic_message_returns_unknown_panic_for_non_string_payload() {
    let result = std::panic::catch_unwind(|| {
        std::panic::panic_any(42i32);
    });
    let payload = result.unwrap_err();
    assert_eq!(panic_message(&payload), "Unknown panic");
}

#[test]
fn filter_short_backtrace_preserves_digit_lines_without_colons() {
    let input = "  5: first_frame at /tmp/a.rs:1\n\
                 10 anomalous_no_colon line\n\
                 6: third_frame at /tmp/b.rs:2";
    let out = filter_short_backtrace(input);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(
        lines[0].starts_with("   0: first_frame"),
        "expected renumbered first frame, got {:?}",
        lines[0]
    );
    assert_eq!(lines[1], "10 anomalous_no_colon line");
    assert!(
        lines[2].starts_with("   1: third_frame"),
        "expected renumbered third frame, got {:?}",
        lines[2]
    );
}

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
    let input = "stack backtrace:\n  0: real_frame at /tmp/x.rs:5";
    let out = filter_short_backtrace(input);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "stack backtrace:");
    assert!(lines[1].starts_with("   0: real_frame"));
}

#[test]
fn filter_short_backtrace_trims_at_end_marker() {
    let input = "  0: noisy_frame at /tmp/a.rs:1\n\
                 stuff: __rust_end_short_backtrace\n  \
                 1: real_frame at /tmp/b.rs:5\n  \
                 2: another_frame at /tmp/c.rs:7";
    let out = filter_short_backtrace(input);
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

#[test]
fn format_backtrace_full_returns_display_verbatim() {
    let bt = std::backtrace::Backtrace::disabled();
    let out = format_backtrace(&bt, true);
    assert_eq!(out, format!("{}", bt));
}

#[test]
fn format_backtrace_short_strips_through_filter() {
    let bt = std::backtrace::Backtrace::disabled();
    let out = format_backtrace(&bt, false);
    assert_eq!(out, filter_short_backtrace(&format!("{}", bt)));
}

#[test]
fn reproducer_line_none_when_print_blob_disabled() {
    let settings = Settings::new();
    assert!(!settings.print_blob);
    assert!(reproducer_line(&settings, Some("AAEC")).is_none());
}

#[test]
fn reproducer_line_none_when_no_blob_attached() {
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

#[test]
fn drive_single_failure_reraises_the_tests_own_panic() {
    let msg = drive_panic_message(
        |tc| {
            let x: i32 = tc.draw(gs::integers());
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
    let msg = drive_panic_message(
        |tc| {
            let b: bool = tc.draw(gs::booleans());
            if b {
                panic!("boom A");
            }
            panic!("boom B");
        },
        &test_settings().report_multiple_failures(true),
    );
    assert!(
        msg.contains("2 distinct failures"),
        "expected the multi-failure count, got: {msg}"
    );
}

#[test]
fn drive_run_error_panics_with_the_engines_message() {
    let msg = drive_panic_message(
        |tc| {
            let _: i32 = tc.draw(gs::integers());
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
            let _: i32 = tc.draw(gs::integers());
        },
        &test_settings(),
        None,
        None,
    );
}

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
fn stateful_overrun_mid_rule_is_reported_as_overrun() {
    use crate::stateful::{Rule, StateMachine};
    struct Hungry;
    impl StateMachine for Hungry {
        fn rules(&self) -> Vec<Rule<Self>> {
            vec![Rule::new("chomp", |_m, tc| {
                loop {
                    let _: i64 = tc.draw(gs::integers());
                }
            })]
        }
        fn invariants(&self) -> Vec<Rule<Self>> {
            vec![]
        }
    }
    let result = run_case_capturing(false, Verbosity::Normal, &mut |tc| {
        crate::stateful::run(Hungry, tc);
        panic!("unreachable: the endless rule must exhaust the choice budget");
    });
    assert!(
        matches!(result, TestCaseResult::Overrun),
        "an overrun inside a rule must unwind through run(), got {result:?}"
    );
}

#[test]
fn stale_panic_info_is_not_attributed_to_a_later_hook_skipping_panic() {
    let result = run_case_capturing(false, Verbosity::Normal, &mut |_tc| {
        let _ = std::panic::catch_unwind(|| panic!("handled internally"));
    });
    assert!(matches!(result, TestCaseResult::Valid));

    let result = run_case_capturing(false, Verbosity::Normal, &mut |_tc| {
        std::panic::resume_unwind(Box::new("hook-skipping panic".to_string()));
    });
    match result {
        TestCaseResult::Interesting(f) => {
            assert_eq!(
                f.origin, "Panic at <unknown>",
                "a hook-skipping panic must not inherit the previous case's capture"
            );
        }
        other => panic!("expected Interesting, got {other:?}"),
    }
}

#[test]
fn discarded_failures_skip_backtrace_capture() {
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

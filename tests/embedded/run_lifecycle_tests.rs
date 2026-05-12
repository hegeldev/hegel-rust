//! Embedded tests for `src/run_lifecycle.rs`.

use super::*;

// ── N18.run_lifecycle: panic_message fallback for non-string payloads ────
//
// `panic_message` downcasts the panic payload to `&str` or `String`; for
// any other type it falls through to the `"Unknown panic"` branch. Real
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

// ── N18.run_lifecycle: filter_short_backtrace digit-without-colon branch ──
//
// `filter_short_backtrace` renumbers frames whose trimmed form matches
// `digit … : …` (the standard `Backtrace` frame line shape). Anomalous
// frame lines that start with a digit but contain no colon — they exist
// in non-standard backtrace formats and as continuation lines from some
// runtimes — fall to a "preserve as-is" branch at run_lifecycle.rs:167.
// All other lines (non-digit-leading) go through the analogous
// preserve-as-is branch at line 170.

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

// ── N18.run_lifecycle: unknown_panic_info placeholder tuple ───────────────
//
// `unknown_panic_info` (run_lifecycle.rs:~190-200) returns the placeholder
// quadruple used when the cross-backend panic hook didn't capture info for
// a panic (e.g. init_panic_hook wasn't called yet). The production path
// always calls init_panic_hook before run_test_case, so this fallback is
// only reached via direct test entry — testing the placeholder shape here
// avoids needing a contrived hook-bypass setup at run_test_case.

#[test]
fn unknown_panic_info_returns_unknown_placeholders() {
    let (thread_name, thread_id, location, backtrace) = unknown_panic_info();
    assert_eq!(thread_name, "<unknown>");
    assert_eq!(thread_id, "?");
    assert_eq!(location, "<unknown>");
    assert_eq!(backtrace.status(), std::backtrace::BacktraceStatus::Disabled);
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

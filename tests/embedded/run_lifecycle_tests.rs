//! Embedded tests for `src/run_lifecycle.rs`.

use super::*;

// `panic_message` downcasts the panic payload to `&str` or `String`; for
// any other type it falls through to the `"Unknown panic"` branch.  Real
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

// `filter_short_backtrace` renumbers frames whose trimmed form matches
// `digit … : …` (the standard `Backtrace` frame line shape).  Anomalous
// frame lines that start with a digit but contain no colon — they exist
// in non-standard backtrace formats and as continuation lines from some
// runtimes — fall to a "preserve as-is" branch.  All other lines
// (non-digit-leading) go through the analogous preserve-as-is branch.

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

// `unknown_panic_info` returns the placeholder quadruple used when the
// cross-backend panic hook didn't capture info for a panic (e.g.
// `init_panic_hook` wasn't called yet).  The production path always
// calls `init_panic_hook` before `run_test_case`, so this fallback is
// only reached via direct test entry — testing the placeholder shape
// here avoids needing a contrived hook-bypass setup at `run_test_case`.

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
    // The non-digit-leading branch (line 170) for a header line.
    let input = "stack backtrace:\n  0: real_frame at /tmp/x.rs:5";
    let out = filter_short_backtrace(input);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "stack backtrace:");
    assert!(lines[1].starts_with("   0: real_frame"));
}

// ── run_lifecycle: filter_short_backtrace marker handling ────────────────
//
// `filter_short_backtrace` trims the captured backtrace down to the
// frames between `__rust_end_short_backtrace` and
// `__rust_begin_short_backtrace`, matching the default Rust panic
// handler's "short" output.  Real backtraces only contain those markers
// when captured during a real panic; the embedded tests below feed
// hand-crafted strings to exercise both ends of the trim.

#[test]
fn filter_short_backtrace_trims_at_end_marker() {
    let input = "  0: noisy_frame at /tmp/a.rs:1\n\
                 stuff: __rust_end_short_backtrace\n  \
                 1: real_frame at /tmp/b.rs:5\n  \
                 2: another_frame at /tmp/c.rs:7";
    let out = filter_short_backtrace(input);
    // The first "noisy" frame is gone; renumbering starts at 0 for the
    // first frame after the end marker.
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

// ── run_lifecycle: format_backtrace forwards to filter_short_backtrace ───
//
// `format_backtrace(bt, true)` returns the raw `Display` of the backtrace
// verbatim; `format_backtrace(bt, false)` runs it through the short-form
// trimmer.  `Backtrace::disabled()` Displays as `disabled backtrace`, so
// the test below distinguishes the branches by the formatted output
// shape rather than the (empty) trim result.

#[test]
fn format_backtrace_full_returns_display_verbatim() {
    let bt = std::backtrace::Backtrace::disabled();
    let out = format_backtrace(&bt, true);
    assert_eq!(out, format!("{}", bt));
}

#[test]
fn format_backtrace_short_strips_through_filter() {
    let bt = std::backtrace::Backtrace::disabled();
    // `filter_short_backtrace` on the disabled-Display string is a
    // no-op (no markers, no digit lines), so we get back what we put in.
    let out = format_backtrace(&bt, false);
    assert_eq!(out, filter_short_backtrace(&format!("{}", bt)));
}

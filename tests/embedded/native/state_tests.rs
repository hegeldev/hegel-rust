use super::*;

// ── NativeTestCase::draw_string ────────────────────────────────────────────
//
// Port of pbtkit/tests/test_text.py::test_draw_string_invalid_range. Pbtkit
// raises ValueError; the native engine's draw_string uses an assert! which
// panics with the same intent.

#[test]
#[should_panic(expected = "Invalid codepoint range")]
fn draw_string_invalid_codepoint_range_panics() {
    let mut tc = NativeTestCase::for_choices(&[], None);
    let _ = tc.draw_string(200, 100, 0, 5);
}

// ── NativeTestCase::start_span past MAX_DEPTH ──────────────────────────────
//
// Hypothesis's `ConjectureData.draw` checks `depth >= MAX_DEPTH` and calls
// `mark_invalid`, which freezes the test case and raises `StopTest`. The
// native engine's `start_span` sets the status to `Invalid` instead, and
// then the next draw must propagate `StopTest` so the test halts cleanly
// rather than panicking with "Frozen: attempted choice on completed test
// case". Recursive `gs::deferred` generators trip this regularly.
#[test]
fn draw_after_max_depth_returns_stop_test() {
    let mut tc = NativeTestCase::for_choices(&[], None);
    for _ in 0..=MAX_DEPTH {
        tc.start_span(0);
    }
    assert!(tc.frozen());
    assert!(tc.draw_integer(0, 100).is_err());
}

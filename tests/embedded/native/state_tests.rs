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

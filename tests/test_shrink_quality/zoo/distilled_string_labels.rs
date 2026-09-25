//! Control: matching labels as *strings* — an element's start and end tag — shorten together.
//!
//! `open = text().alphabet("ab").min_size(1).max_size(3)`, `payload ∈ [0, 100]`, `close` drawn
//! like `open`; the property fails iff `open == close` and the payload is non-zero. Shortlex
//! ideal `("a", 1, "a")`. A human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (String, i64, String);

fn tag() -> gs::TextGenerator {
    gs::text().alphabet("ab").min_size(1).max_size(3)
}

fn draw(tc: &TestCase) -> Draws {
    let open: String = tc.draw_silent(tag());
    let v = tc.draw_silent(gs::integers::<i64>().min_value(0).max_value(100));
    let close: String = tc.draw_silent(tag());
    (open, v, close)
}

fn tags_match_with_payload((open, v, close): &Draws) -> bool {
    open == close && *v != 0
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(tags_match_with_payload(&("a".into(), 1, "a".into())));
    assert!(!tags_match_with_payload(&("a".into(), 0, "a".into())));
    assert!(!tags_match_with_payload(&("a".into(), 1, "b".into())));
    assert!(tags_match_with_payload(&("ba".into(), 1, "ba".into())));
}

#[test]
fn control_matching_tags_are_shortened_together() {
    assert_shrinks_to(
        &("a".to_string(), 1, "a".to_string()),
        30,
        500,
        draw,
        tags_match_with_payload,
    );
}

//! Distilled: matching labels a few fields apart, plus an unrelated node that happens to hold the
//! same value.
//!
//! `tag ∈ [0, 2]`, `open ∈ [0, 3]`, three fields `payload, a, b ∈ [0, 100]`, `close ∈ [0, 3]`;
//! the property fails iff `tag == 2`, `open == close` and the payload is non-zero. Shortlex ideal
//! `(2, 0, 1, 0, 0, 0)`. The labels can only be lowered together, and `shrink_duplicates`
//! groups nodes by value: with the labels at `2` the tag joins the group (at `1`, the payload
//! does), so the whole-group replacement is rejected and the group has to be retried without
//! the decoy — split by constraints here, since the tag and the payload have other bounds.
//! `lower_integers_together` does not reach the pair: it pairs integer nodes at most three
//! entries apart, and these are four. `distilled_matching_labels` is the control without the
//! decoy. A human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (u8, u8, i64, i64, i64, u8);

fn field() -> gs::IntegerGenerator<i64> {
    gs::integers::<i64>().min_value(0).max_value(100)
}

fn draw(tc: &TestCase) -> Draws {
    let tag = tc.draw_silent(gs::integers::<u8>().max_value(2));
    let open = tc.draw_silent(gs::integers::<u8>().max_value(3));
    let v = tc.draw_silent(field());
    let a = tc.draw_silent(field());
    let b = tc.draw_silent(field());
    let close = tc.draw_silent(gs::integers::<u8>().max_value(3));
    (tag, open, v, a, b, close)
}

fn third_variant_with_matching_labels((tag, open, v, _a, _b, close): &Draws) -> bool {
    *tag == 2 && open == close && *v != 0
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(third_variant_with_matching_labels(&(2, 0, 1, 0, 0, 0)));
    assert!(!third_variant_with_matching_labels(&(1, 0, 1, 0, 0, 0)));
    assert!(!third_variant_with_matching_labels(&(2, 0, 0, 0, 0, 0)));
    assert!(!third_variant_with_matching_labels(&(2, 0, 1, 0, 0, 1)));
    assert!(third_variant_with_matching_labels(&(2, 2, 1, 0, 0, 2)));
}

#[test]
fn labels_are_lowered_past_the_tag_value() {
    assert_shrinks_to(
        &(2, 0, 1, 0, 0, 0),
        30,
        500,
        draw,
        third_variant_with_matching_labels,
    );
}

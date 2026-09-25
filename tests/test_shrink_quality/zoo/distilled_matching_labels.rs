//! Control: two draws that must stay equal — an opening and a closing label — are lowered
//! together.
//!
//! `open ∈ [0, 10]`, `payload ∈ [0, 100]`, `close ∈ [0, 10]`, the labels either separated by
//! the payload or adjacent; the property fails iff `open == close` and the payload is non-zero.
//! Shortlex ideal `(0, 1, 0)` as `(open, payload, close)`. Lowering either label alone breaks the
//! match; `shrink_duplicates` lowers every node holding one value as a group and no other node
//! shares the labels' value here. `distilled_labels_with_tag` adds the decoy that breaks it. A
//! human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (u8, i64, u8);

fn label() -> gs::IntegerGenerator<u8> {
    gs::integers::<u8>().max_value(10)
}

fn payload() -> gs::IntegerGenerator<i64> {
    gs::integers::<i64>().min_value(0).max_value(100)
}

fn draw_separated(tc: &TestCase) -> Draws {
    let open = tc.draw_silent(label());
    let v = tc.draw_silent(payload());
    let close = tc.draw_silent(label());
    (open, v, close)
}

fn draw_adjacent(tc: &TestCase) -> Draws {
    let open = tc.draw_silent(label());
    let close = tc.draw_silent(label());
    let v = tc.draw_silent(payload());
    (open, v, close)
}

fn labels_match_with_payload((open, v, close): &Draws) -> bool {
    open == close && *v != 0
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(labels_match_with_payload(&(0, 1, 0)));
    assert!(!labels_match_with_payload(&(0, 0, 0)));
    assert!(!labels_match_with_payload(&(0, 1, 1)));
    assert!(labels_match_with_payload(&(2, 1, 2)));
}

#[test]
fn control_separated_labels_are_lowered_together() {
    assert_shrinks_to(
        &(0, 1, 0),
        30,
        200,
        draw_separated,
        labels_match_with_payload,
    );
}

#[test]
fn control_adjacent_labels_are_lowered_together() {
    assert_shrinks_to(
        &(0, 1, 0),
        30,
        200,
        draw_adjacent,
        labels_match_with_payload,
    );
}

//! Distilled: the value behind the list is a *string* whose length must match.
//!
//! `v = vecs(integers 0..=1000).max_size(10)`, then `s = text().max_size(10)`; the property is
//! only checked when `s.chars().count() == v.len()` and fails iff some element is non-zero.
//! Shortlex ideal `[1]` with a one-character string. `distilled_declared_length_after` with the
//! length carried by a string: deleting an element needs one character deleted from `s` in the
//! same move, and the string sits behind the list, so `delete_spans`' nudge has to shorten a
//! string, not just step an integer. A human writes `[1]` and any one character.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (Vec<i64>, String);

fn draw(tc: &TestCase) -> Draws {
    let v: Vec<i64> =
        tc.draw_silent(gs::vecs(gs::integers::<i64>().min_value(0).max_value(1000)).max_size(10));
    let s: String = tc.draw_silent(gs::text().max_size(10));
    (v, s)
}

fn one_char_per_element_with_payload((v, s): &Draws) -> bool {
    s.chars().count() == v.len() && v.iter().any(|&x| x != 0)
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(one_char_per_element_with_payload(&(
        vec![1],
        "a".to_string()
    )));
    assert!(!one_char_per_element_with_payload(&(
        vec![1],
        String::new()
    )));
    assert!(!one_char_per_element_with_payload(&(
        vec![0],
        "a".to_string()
    )));
}

#[test]
fn elements_and_characters_are_deleted_together() {
    assert_shrinks_to(
        &(vec![1], "0".to_string()),
        30,
        200,
        draw,
        one_char_per_element_with_payload,
    );
}

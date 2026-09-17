//! Distilled: three pinned labels, and a boolean decoy between two.
//!
//! Triple: `a ∈ [0, 3]`, `x ∈ [0, 100]`, `b ∈ [0, 3]`, `y ∈ [0, 100]`, `c ∈ [0, 3]`; fails iff
//! `a == b == c` and `x != 0`; ideal `(0, 1, 0, 0, 0)`. `lower_integers_together` moves pairs,
//! so only `shrink_duplicates` can lower a triple, and the payload `1` joins its value-keyed
//! group when the labels sit at `1` until the group is retried split by constraints. Flagged:
//! `a ∈ [0, 3]`, `flag: bool`, `x, y, z ∈ [0, 100]`, `b ∈ [0, 3]`; fails iff `a == b`, `flag`
//! and `x >= 2`; ideal `(0, true, 2, 0, 0, 0)`. The `true` does not join an integer group
//! (kinds are compared), so only labels that start at `2` meet a decoy, the payload `2`. A human
//! writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

fn label() -> gs::IntegerGenerator<u8> {
    gs::integers::<u8>().max_value(3)
}

fn field() -> gs::IntegerGenerator<i64> {
    gs::integers::<i64>().min_value(0).max_value(100)
}

type Triple = (u8, i64, u8, i64, u8);

fn draw_triple(tc: &TestCase) -> Triple {
    let a = tc.draw_silent(label());
    let x = tc.draw_silent(field());
    let b = tc.draw_silent(label());
    let y = tc.draw_silent(field());
    let c = tc.draw_silent(label());
    (a, x, b, y, c)
}

fn three_labels_with_payload((a, x, b, _y, c): &Triple) -> bool {
    a == b && b == c && *x != 0
}

type Flagged = (u8, bool, i64, i64, i64, u8);

fn draw_flagged(tc: &TestCase) -> Flagged {
    let a = tc.draw_silent(label());
    let flag = tc.draw_silent(gs::booleans());
    let x = tc.draw_silent(field());
    let y = tc.draw_silent(field());
    let z = tc.draw_silent(field());
    let b = tc.draw_silent(label());
    (a, flag, x, y, z, b)
}

fn flagged_labels_with_payload((a, flag, x, _y, _z, b): &Flagged) -> bool {
    a == b && *flag && *x >= 2
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(three_labels_with_payload(&(0, 1, 0, 0, 0)));
    assert!(!three_labels_with_payload(&(0, 0, 0, 0, 0)));
    assert!(!three_labels_with_payload(&(0, 1, 0, 0, 1)));
    assert!(three_labels_with_payload(&(1, 1, 1, 0, 1)));

    assert!(flagged_labels_with_payload(&(0, true, 2, 0, 0, 0)));
    assert!(!flagged_labels_with_payload(&(0, false, 2, 0, 0, 0)));
    assert!(!flagged_labels_with_payload(&(0, true, 1, 0, 0, 0)));
    assert!(!flagged_labels_with_payload(&(0, true, 2, 0, 0, 1)));
    assert!(flagged_labels_with_payload(&(1, true, 2, 0, 0, 1)));
}

#[test]
fn three_labels_are_lowered_to_zero() {
    assert_shrinks_to(
        &(0, 1, 0, 0, 0),
        30,
        500,
        draw_triple,
        three_labels_with_payload,
    );
}

#[test]
fn labels_with_a_boolean_between_are_lowered_past_the_payload() {
    assert_shrinks_to(
        &(0, true, 2, 0, 0, 0),
        30,
        500,
        draw_flagged,
        flagged_labels_with_payload,
    );
}

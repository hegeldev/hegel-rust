//! Distilled: matching labels around a variable-length list of children.
//!
//! `open ∈ [0, 3]`, then `children = vecs((name ∈ [0, 3], value ∈ [0, 100])).max_size(6)`, then
//! `close ∈ [0, 3]`; the property fails iff `open == close` and at least two children carry a
//! non-zero value. Shortlex ideal `(0, [(0, 1), (0, 1)], 0)`. With the labels at `1` the two
//! payload `1`s join `shrink_duplicates`' value-keyed group, so the labels come down only when
//! the group is retried split by constraints; with two children between them the labels are five
//! integer entries apart, beyond `lower_integers_together`'s reach of three. The one-child
//! control (ideal `(0, [(0, 1)], 0)`) has the labels three apart. A human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (u8, Vec<(u8, u8)>, u8);

fn label() -> gs::IntegerGenerator<u8> {
    gs::integers::<u8>().max_value(3)
}

fn draw(tc: &TestCase) -> Draws {
    let open = tc.draw_silent(label());
    let children: Vec<(u8, u8)> = tc.draw_silent(
        gs::vecs(gs::tuples!(label(), gs::integers::<u8>().max_value(100))).max_size(6),
    );
    let close = tc.draw_silent(label());
    (open, children, close)
}

fn matching_labels_with_two_payloads((open, children, close): &Draws) -> bool {
    open == close && children.iter().filter(|&&(_, v)| v != 0).count() >= 2
}

fn matching_labels_with_payload((open, children, close): &Draws) -> bool {
    open == close && children.iter().any(|&(_, v)| v != 0)
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(matching_labels_with_two_payloads(&(
        0,
        vec![(0, 1), (0, 1)],
        0
    )));
    assert!(!matching_labels_with_two_payloads(&(0, vec![(0, 1)], 0)));
    assert!(!matching_labels_with_two_payloads(&(
        0,
        vec![(0, 1), (0, 0)],
        0
    )));
    assert!(!matching_labels_with_two_payloads(&(
        0,
        vec![(0, 1), (0, 1)],
        1
    )));
    assert!(matching_labels_with_two_payloads(&(
        1,
        vec![(0, 1), (0, 1)],
        1
    )));

    assert!(matching_labels_with_payload(&(0, vec![(0, 1)], 0)));
    assert!(!matching_labels_with_payload(&(0, vec![(0, 0)], 0)));
    assert!(!matching_labels_with_payload(&(0, vec![], 0)));
    assert!(!matching_labels_with_payload(&(0, vec![(0, 1)], 1)));
}

#[test]
fn labels_around_two_children_are_lowered_to_zero() {
    assert_shrinks_to(
        &(0, vec![(0, 1), (0, 1)], 0),
        30,
        300,
        draw,
        matching_labels_with_two_payloads,
    );
}

#[test]
fn control_labels_around_one_child_are_lowered_to_zero() {
    assert_shrinks_to(
        &(0, vec![(0, 1)], 0),
        30,
        300,
        draw,
        matching_labels_with_payload,
    );
}

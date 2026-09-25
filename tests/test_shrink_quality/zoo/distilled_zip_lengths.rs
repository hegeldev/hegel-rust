//! Control: two lists that must stay the same length, so deleting from one means deleting from
//! the other.
//!
//! `a, b = vecs(integers 0..=1000).max_size(10)`; the property is only checked when
//! `a.len() == b.len()` and fails iff some element of `a` is non-zero. Shortlex ideal
//! `([1], [0])`. A paired *deletion* — one element from each list, not contiguous — is found;
//! compare `distilled_index_after_list`, where the partner of the deletion is a *value* behind
//! the list. A human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (Vec<i64>, Vec<i64>);

fn list() -> gs::VecGenerator<gs::IntegerGenerator<i64>, i64> {
    gs::vecs(gs::integers::<i64>().min_value(0).max_value(1000)).max_size(10)
}

fn draw(tc: &TestCase) -> Draws {
    let a: Vec<i64> = tc.draw_silent(list());
    let b: Vec<i64> = tc.draw_silent(list());
    (a, b)
}

fn same_length_with_payload((a, b): &Draws) -> bool {
    a.len() == b.len() && a.iter().any(|&x| x != 0)
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(same_length_with_payload(&(vec![1], vec![0])));
    assert!(!same_length_with_payload(&(vec![1], vec![])));
    assert!(!same_length_with_payload(&(vec![0], vec![0])));
}

#[test]
fn control_paired_elements_are_deleted_together() {
    assert_shrinks_to(&(vec![1], vec![0]), 30, 100, draw, same_length_with_payload);
}

//! Control: a declared length drawn *before* the list, but not right before it.
//!
//! `n ∈ [0, 10]`, then an unrelated header field `h ∈ [0, 1000]`, then
//! `v = vecs(integers 0..=1000).max_size(10)`; the property is only checked when `n == v.len()`
//! and fails iff some element is non-zero. Shortlex ideal `(1, 0, [1])`. The node before the
//! first element is `h`, not `n`, and lowering `n` changes no draw, yet the deletions land: the
//! distance between the count and the list is not what matters, only that the count is before
//! the deletion (compare `distilled_declared_length_after`). A human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (usize, i64, Vec<i64>);

fn draw(tc: &TestCase) -> Draws {
    let n = tc.draw_silent(gs::integers::<usize>().max_value(10));
    let h = tc.draw_silent(gs::integers::<i64>().min_value(0).max_value(1000));
    let v: Vec<i64> =
        tc.draw_silent(gs::vecs(gs::integers::<i64>().min_value(0).max_value(1000)).max_size(10));
    (n, h, v)
}

fn declared_length_with_payload((n, _h, v): &Draws) -> bool {
    *n == v.len() && v.iter().any(|&x| x != 0)
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(declared_length_with_payload(&(1, 0, vec![1])));
    assert!(!declared_length_with_payload(&(1, 0, vec![0])));
    assert!(!declared_length_with_payload(&(0, 0, vec![])));
    assert!(!declared_length_with_payload(&(2, 0, vec![1])));
}

#[test]
fn control_elements_are_deleted_with_the_count_two_nodes_ahead() {
    assert_shrinks_to(
        &(1, 0, vec![1]),
        30,
        200,
        draw,
        declared_length_with_payload,
    );
}

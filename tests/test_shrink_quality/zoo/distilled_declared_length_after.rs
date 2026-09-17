//! Distilled: a list and its declared length, the length drawn *after* the list.
//!
//! `v = vecs(integers 0..=1000).max_size(10)`, then `n ∈ [0, 10]`; the property is only checked
//! when `n == v.len()` and fails iff some element is non-zero. Shortlex ideal `([1], 1)`. Deleting
//! an element is accepted only together with `n − 1`; `delete_chunks` pairs a deletion with a
//! decrement of the node just *before* the chunk, and `bind_deletion` lowers a count that comes
//! before what it counts, so the move has to come from `delete_spans` nudging the draw after
//! the list when the plain element deletion is rejected. A human writes `([1], 1)` too.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (Vec<i64>, usize);

fn draw(tc: &TestCase) -> Draws {
    let v: Vec<i64> =
        tc.draw_silent(gs::vecs(gs::integers::<i64>().min_value(0).max_value(1000)).max_size(10));
    let n = tc.draw_silent(gs::integers::<usize>().max_value(10));
    (v, n)
}

fn declared_length_with_payload((v, n): &Draws) -> bool {
    *n == v.len() && v.iter().any(|&x| x != 0)
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(declared_length_with_payload(&(vec![1], 1)));
    assert!(!declared_length_with_payload(&(vec![1], 0)));
    assert!(!declared_length_with_payload(&(vec![0], 1)));
}

#[test]
fn elements_are_deleted_and_the_length_lowered() {
    assert_shrinks_to(&(vec![1], 1), 30, 200, draw, declared_length_with_payload);
}

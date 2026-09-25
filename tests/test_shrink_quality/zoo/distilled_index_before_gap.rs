//! Control: a fixed-range position drawn *before* the list, adjacent to it or two fields ahead.
//!
//! `i ∈ [0, 19]`, then (in the gapped variant) two header fields `a, b ∈ [0, 1000]`, then
//! `v = vecs(integers 0..=1000).max_size(20)`; the property fails iff `i < v.len() && v[i] != 0`.
//! Shortlex ideal `(0, 0, 0, [1])`. The mirror image of `distilled_index_after_list`: a value
//! *before* the deletion is reached wherever it sits, so the family's boundary is exactly "value
//! behind". A human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (usize, i64, i64, Vec<i64>);

fn field() -> gs::IntegerGenerator<i64> {
    gs::integers::<i64>().min_value(0).max_value(1000)
}

fn list(tc: &TestCase) -> Vec<i64> {
    tc.draw_silent(gs::vecs(field()).max_size(20))
}

fn draw_adjacent(tc: &TestCase) -> Draws {
    let i = tc.draw_silent(gs::integers::<usize>().max_value(19));
    let v = list(tc);
    (i, 0, 0, v)
}

fn draw_gapped(tc: &TestCase) -> Draws {
    let i = tc.draw_silent(gs::integers::<usize>().max_value(19));
    let a = tc.draw_silent(field());
    let b = tc.draw_silent(field());
    let v = list(tc);
    (i, a, b, v)
}

fn indexed_element_nonzero((i, _a, _b, v): &Draws) -> bool {
    *i < v.len() && v[*i] != 0
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(indexed_element_nonzero(&(0, 0, 0, vec![1])));
    assert!(!indexed_element_nonzero(&(0, 0, 0, vec![0])));
    assert!(!indexed_element_nonzero(&(0, 0, 0, vec![])));
    assert!(!indexed_element_nonzero(&(1, 0, 0, vec![1])));
    assert!(indexed_element_nonzero(&(1, 0, 0, vec![0, 1])));
}

#[test]
fn dead_prefix_is_deleted_with_the_index_adjacent() {
    assert_shrinks_to(
        &(0, 0, 0, vec![1]),
        30,
        200,
        draw_adjacent,
        indexed_element_nonzero,
    );
}

#[test]
fn dead_prefix_is_deleted_with_the_index_two_fields_ahead() {
    assert_shrinks_to(
        &(0, 0, 0, vec![1]),
        30,
        200,
        draw_gapped,
        indexed_element_nonzero,
    );
}

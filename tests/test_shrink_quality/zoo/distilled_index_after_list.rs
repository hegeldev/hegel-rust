//! Distilled: a deletion blocked by a position drawn *after* the list.
//!
//! `v = vecs(integers 0..=1000).max_size(20)`, then `i ∈ [0, 19]` with a fixed range; the
//! property fails iff `i < v.len() && v[i] != 0`. Shortlex ideal `([1], 0)`. The shrinker deletes
//! everything after `i` and zeroes everything else, leaving `[0, …, 0, 1], i`: each zero in front
//! of the indexed element goes only together with `i − 1`, and `i` sits behind the list, where
//! `delete_chunks` (which decrements the node just *before* a rejected chunk) never looks;
//! `delete_spans` retries a rejected element deletion with the draw after the list nudged. Two
//! controls: the index drawn first with the list sized `min_size(i + 1)`, and the index drawn
//! after the list with a range that depends on it (the deletion pushes the recorded index out of
//! range and the engine redraws it). A human writes `([1], 0)` too.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (Vec<i64>, usize);

fn elements() -> gs::IntegerGenerator<i64> {
    gs::integers::<i64>().min_value(0).max_value(1000)
}

fn draw_fixed_index_after(tc: &TestCase) -> Draws {
    let v: Vec<i64> = tc.draw_silent(gs::vecs(elements()).max_size(20));
    let i = tc.draw_silent(gs::integers::<usize>().max_value(19));
    (v, i)
}

fn draw_index_before(tc: &TestCase) -> Draws {
    let i = tc.draw_silent(gs::integers::<usize>().max_value(19));
    let v: Vec<i64> = tc.draw_silent(gs::vecs(elements()).min_size(i + 1).max_size(20));
    (v, i)
}

fn draw_bounded_index_after(tc: &TestCase) -> Draws {
    let v: Vec<i64> = tc.draw_silent(gs::vecs(elements()).min_size(1).max_size(20));
    let i = tc.draw_silent(gs::integers::<usize>().max_value(v.len() - 1));
    (v, i)
}

fn indexed_is_nonzero((v, i): &Draws) -> bool {
    *i < v.len() && v[*i] != 0
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(indexed_is_nonzero(&(vec![1], 0)));
    assert!(!indexed_is_nonzero(&(vec![0], 0)));
    assert!(!indexed_is_nonzero(&(vec![], 0)));
}

#[test]
fn prefix_before_the_indexed_element_is_deleted() {
    assert_shrinks_to(
        &(vec![1], 0),
        30,
        100,
        draw_fixed_index_after,
        indexed_is_nonzero,
    );
}

#[test]
fn control_index_before_the_list_reaches_the_ideal() {
    assert_shrinks_to(
        &(vec![1], 0),
        30,
        100,
        draw_index_before,
        indexed_is_nonzero,
    );
}

#[test]
fn control_list_bounded_index_reaches_the_ideal() {
    assert_shrinks_to(
        &(vec![1], 0),
        30,
        100,
        draw_bounded_index_after,
        indexed_is_nonzero,
    );
}

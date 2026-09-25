//! Control: a list drawn as a count and then a `for` loop, with dead elements in the *middle*.
//!
//! `n ∈ [0, 10]`, then `n` draws of `integers 0..=1000`; the property fails iff `n ≥ 2` and the
//! first and last elements are both non-zero. Shortlex ideal `[1, 1]`. Lowering `n` alone drops
//! the *last* element and `delete_chunks` deletes the *first* with the decrement of `n` just
//! before it; a dead middle element needs `bind_deletion`'s lower-the-count-then-delete-a-window
//! move. A human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

fn draw(tc: &TestCase) -> Vec<i64> {
    let n: usize = tc.draw_silent(gs::integers::<usize>().max_value(10));
    (0..n)
        .map(|_| tc.draw_silent(gs::integers::<i64>().min_value(0).max_value(1000)))
        .collect()
}

fn first_and_last_nonzero(v: &[i64]) -> bool {
    v.len() >= 2 && v[0] != 0 && v[v.len() - 1] != 0
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(first_and_last_nonzero(&[1, 1]));
    assert!(!first_and_last_nonzero(&[1]));
    assert!(!first_and_last_nonzero(&[0, 1]));
    assert!(!first_and_last_nonzero(&[1, 0]));
    assert!(first_and_last_nonzero(&[1, 0, 0, 1]));
}

#[test]
fn control_dead_middle_elements_are_deleted_with_the_count() {
    assert_shrinks_to(&vec![1, 1], 30, 200, draw, |v| first_and_last_nonzero(v));
}

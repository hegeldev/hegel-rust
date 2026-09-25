//! Control: irrelevant elements that cannot be zeroed first — a `unique` list with one element
//! that matters.
//!
//! `v = vecs(integers 0..=1000).unique(true).max_size(10)`; the property fails iff some element
//! is `≥ 500`. Shortlex ideal `[500]`. The other elements are dead weight but, being distinct,
//! cannot all be lowered to `0` before deletion, so each has to be deleted as itself. A human
//! writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

fn draw(tc: &TestCase) -> Vec<i64> {
    tc.draw_silent(
        gs::vecs(gs::integers::<i64>().min_value(0).max_value(1000))
            .unique(true)
            .max_size(10),
    )
}

fn has_large_element(v: &[i64]) -> bool {
    v.iter().any(|&x| x >= 500)
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(has_large_element(&[500]));
    assert!(!has_large_element(&[499]));
    assert!(!has_large_element(&[]));
}

#[test]
fn control_distinct_dead_elements_are_deleted() {
    assert_shrinks_to(&vec![500], 30, 100, draw, |v| has_large_element(v));
}

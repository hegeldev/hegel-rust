//! Distilled: a deletion that must be paid for by flipping a *boolean* behind the list.
//!
//! `v = vecs(integers 0..=1000).max_size(10)`, then `even = booleans()`; the property is only
//! checked when `even == (v.len() % 2 == 0)` and fails iff some element is non-zero. Shortlex
//! ideal `([1], false)`. Deleting one element flips the parity, so it has to go with the flag
//! flipped — a change behind the list; deleting two keeps the parity, so dead zeros go in pairs
//! and an even list would be left one deletion short at `([0, 1], true)` without `delete_spans`
//! flipping the boolean after the list. The cheapest paired change there is. A human writes
//! `([1], false)` too.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (Vec<i64>, bool);

fn draw(tc: &TestCase) -> Draws {
    let v: Vec<i64> =
        tc.draw_silent(gs::vecs(gs::integers::<i64>().min_value(0).max_value(1000)).max_size(10));
    let even = tc.draw_silent(gs::booleans());
    (v, even)
}

fn parity_flag_with_payload((v, even): &Draws) -> bool {
    *even == (v.len() % 2 == 0) && v.iter().any(|&x| x != 0)
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(parity_flag_with_payload(&(vec![1], false)));
    assert!(!parity_flag_with_payload(&(vec![1], true)));
    assert!(!parity_flag_with_payload(&(vec![0], false)));
    assert!(!parity_flag_with_payload(&(vec![], true)));
    assert!(parity_flag_with_payload(&(vec![0, 1], true)));
}

#[test]
fn the_last_dead_element_is_deleted_with_the_flag_flipped() {
    assert_shrinks_to(&(vec![1], false), 30, 200, draw, parity_flag_with_payload);
}

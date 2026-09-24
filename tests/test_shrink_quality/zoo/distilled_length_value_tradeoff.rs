//! Distilled: a deletion that has to be paid for by *raising* a later draw.
//!
//! `v = vecs(booleans()).max_size(20)`, then `k ∈ [0, 100]`; the property fails iff
//! `v.len() + k >= 5`. Shortlex ideal `([], 5)`. Every element is deletable, but only if `k` goes
//! up by one in the same move. The draw to raise is behind the deletable span and is not a gate,
//! so `try_shortening_via_increment` (which raises a gating draw to drop what it guards) does not
//! reach it; `delete_spans`' nudge of the draw after the list tries one step each way, and this
//! is the case that needs the step up. A human writes `([], 5)` too.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (Vec<bool>, i64);

fn draw(tc: &TestCase) -> Draws {
    let v: Vec<bool> = tc.draw_silent(gs::vecs(gs::booleans()).max_size(20));
    let k = tc.draw_silent(gs::integers::<i64>().min_value(0).max_value(100));
    (v, k)
}

fn sum_at_least_five((v, k): &Draws) -> bool {
    v.len() as i64 + k >= 5
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(sum_at_least_five(&(vec![], 5)));
    assert!(!sum_at_least_five(&(vec![], 4)));
}

#[test]
fn elements_are_deleted_and_k_raised() {
    assert_shrinks_to(&(vec![], 5), 30, 100, draw, sum_at_least_five);
}

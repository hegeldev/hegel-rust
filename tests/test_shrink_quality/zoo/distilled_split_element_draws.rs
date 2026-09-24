//! Control, distilled from debian-changelog/11: dead elements of a count-driven list whose
//! draws are *split*, the values up front and a per-element separator much later.
//!
//! `n ∈ [0, 3]`, then `n` items `∈ [0, 9]`, a header field `h ∈ [0, 100]`, `n` separators
//! `∈ [0, 2]`, and the payload `x ∈ [0, 100]`; the property fails iff `x ≥ 50`. Shortlex ideal
//! `([], 0, [], 50)`. Deleting an item means `n − 1`, one node near `n` and one node behind `h`;
//! it passes because `h = 0` and the separator `0` are interchangeable, so `delete_chunks` drops
//! the window `[item, h]` and the separator stands in for `h`. Pin `h` non-zero and the deletion
//! stalls — `distilled_split_element_pinned_value`. The second control draws each separator next
//! to its item. A human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (Vec<u8>, i64, Vec<u8>, i64);

fn item() -> gs::IntegerGenerator<u8> {
    gs::integers::<u8>().max_value(9)
}

fn sep() -> gs::IntegerGenerator<u8> {
    gs::integers::<u8>().max_value(2)
}

fn field() -> gs::IntegerGenerator<i64> {
    gs::integers::<i64>().min_value(0).max_value(100)
}

fn draw_split(tc: &TestCase) -> Draws {
    let n: usize = tc.draw_silent(gs::integers::<usize>().max_value(3));
    let items: Vec<u8> = (0..n).map(|_| tc.draw_silent(item())).collect();
    let h = tc.draw_silent(field());
    let seps: Vec<u8> = (0..n).map(|_| tc.draw_silent(sep())).collect();
    let x = tc.draw_silent(field());
    (items, h, seps, x)
}

fn draw_adjacent(tc: &TestCase) -> Draws {
    let n: usize = tc.draw_silent(gs::integers::<usize>().max_value(3));
    let mut items = Vec::new();
    let mut seps = Vec::new();
    for _ in 0..n {
        items.push(tc.draw_silent(item()));
        seps.push(tc.draw_silent(sep()));
    }
    let h = tc.draw_silent(field());
    let x = tc.draw_silent(field());
    (items, h, seps, x)
}

fn payload_nonzero((_items, _h, _seps, x): &Draws) -> bool {
    *x >= 50
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(payload_nonzero(&(vec![], 0, vec![], 50)));
    assert!(!payload_nonzero(&(vec![], 0, vec![], 49)));
    assert!(payload_nonzero(&(vec![0], 0, vec![0], 50)));
}

#[test]
fn control_dead_items_with_split_draws_are_deleted() {
    assert_shrinks_to(
        &(vec![], 0, vec![], 50),
        30,
        100,
        draw_split,
        payload_nonzero,
    );
}

#[test]
fn control_dead_items_with_adjacent_draws_are_deleted() {
    assert_shrinks_to(
        &(vec![], 0, vec![], 50),
        30,
        100,
        draw_adjacent,
        payload_nonzero,
    );
}

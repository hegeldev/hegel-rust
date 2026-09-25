//! Control: elements refer to other elements by *position* (an arena).
//!
//! `next = vecs(integers 0..=7).max_size(8)`: element `i` names the position of the node after
//! it. Follow the links from position 0 until out of range or revisited; the property fails iff
//! the walk visits three nodes. Shortlex ideal `[1, 2, 0]`. A dead node in the middle is
//! deletable only if every link past it drops by one, in later elements too; the shrinker
//! instead *reuses* it, lowering links until the walk runs through the dead position, after which
//! the tail is ordinary deletion. `distilled_ops_push_get` lacks that cheap relinking. A human
//! writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

fn draw(tc: &TestCase) -> Vec<usize> {
    tc.draw_silent(gs::vecs(gs::integers::<usize>().max_value(7)).max_size(8))
}

fn walk_visits_three(next: &[usize]) -> bool {
    let mut seen = vec![false; next.len()];
    let mut at = 0;
    let mut visited = 0;
    while at < next.len() && !seen[at] {
        seen[at] = true;
        visited += 1;
        at = next[at];
    }
    visited >= 3
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(walk_visits_three(&[1, 2, 0]));
    assert!(!walk_visits_three(&[1, 0]));
    assert!(!walk_visits_three(&[1, 1, 0]));
    assert!(!walk_visits_three(&[1, 0, 0]));
    assert!(!walk_visits_three(&[0, 2, 0]));
    assert!(walk_visits_three(&[2, 7, 3, 0]));
}

#[test]
fn control_dead_arena_nodes_are_deleted() {
    assert_shrinks_to(&vec![1, 2, 0], 30, 200, draw, |next| {
        walk_visits_three(next)
    });
}

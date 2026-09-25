//! Distilled: elements refer to each other by *name*, and a too-long chain must be cut.
//!
//! `nodes = vecs((id ∈ [0, 9], next ∈ [0, 9])).max_size(8)`. Walk from the first element to the
//! first element whose `id` equals the current `next`, stopping when there is none or it was
//! seen; the property fails iff the walk visits three elements. Shortlex ideal
//! `[(0, 1), (1, 2), (2, 0)]`. The chain is cut to three from every seed; the stall is a
//! *rename*: some seeds end at `[(1, 0), (0, 2), (2, 0)]`, the same chain with names `0` and `1`
//! swapped, three nodes changed at once and none of them a lowering of a duplicate group. A
//! human writes the ideal.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

fn draw(tc: &TestCase) -> Vec<(u8, u8)> {
    tc.draw_silent(
        gs::vecs(gs::tuples!(
            gs::integers::<u8>().max_value(9),
            gs::integers::<u8>().max_value(9)
        ))
        .max_size(8),
    )
}

fn chain_visits_three(nodes: &[(u8, u8)]) -> bool {
    let mut seen = vec![false; nodes.len()];
    let mut at = if nodes.is_empty() { None } else { Some(0) };
    let mut visited = 0;
    while let Some(i) = at {
        if seen[i] {
            break;
        }
        seen[i] = true;
        visited += 1;
        let want = nodes[i].1;
        at = nodes.iter().position(|&(id, _)| id == want);
    }
    visited >= 3
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(chain_visits_three(&[(0, 1), (1, 2), (2, 0)]));
    assert!(!chain_visits_three(&[(0, 1), (1, 0)]));
    assert!(!chain_visits_three(&[(0, 1), (1, 1), (2, 0)]));
    assert!(!chain_visits_three(&[(0, 1), (1, 0), (2, 0)]));
    assert!(!chain_visits_three(&[(0, 0), (1, 2), (2, 0)]));
    assert!(chain_visits_three(&[(0, 1), (1, 2), (2, 3), (3, 0)]));
}

#[test]
#[ignore = "shrinker: no pass renames three nodes at once"]
fn chain_of_three_gets_the_smallest_names() {
    assert_shrinks_to(&vec![(0, 1), (1, 2), (2, 0)], 30, 200, draw, |nodes| {
        chain_visits_three(nodes)
    });
}

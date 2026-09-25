//! Distilled: the stateful shape as one list, where a later element refers to an earlier one by
//! position.
//!
//! `ops = vecs(op).max_size(12)`, each op `kind ∈ {0, 1}` then a value: `Push(v ∈ [0, 100])` for
//! kind 0, `Get(k)` for kind 1 (`k = v % 12`). The ops run against a growing vector, an
//! out-of-range `Get` is ignored, and the property fails iff some `Get` reads a non-zero value.
//! Shortlex ideal `[Push(1), Get(0)]`. Every `Push(0)` in front of the `Push(1)` is dead weight,
//! but deleting one shifts the position the `Get` reads, so it has to go together with `k − 1` —
//! a change to a *later list element*. Seeds that stall end on the ladder
//! `[Push(0) × m, Push(1), Get(m)]`. A human writes `[Push(1), Get(0)]` too.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

#[derive(Debug, PartialEq, Eq)]
enum Op {
    Push(i64),
    Get(usize),
}

fn draw(tc: &TestCase) -> Vec<Op> {
    let kinds: Vec<(u8, i64)> = tc.draw_silent(
        gs::vecs(gs::tuples!(
            gs::integers::<u8>().max_value(1),
            gs::integers::<i64>().min_value(0).max_value(100)
        ))
        .max_size(12),
    );
    kinds
        .into_iter()
        .map(|(kind, v)| {
            if kind == 0 {
                Op::Push(v)
            } else {
                Op::Get((v as usize) % 12)
            }
        })
        .collect()
}

fn a_get_reads_nonzero(ops: &[Op]) -> bool {
    let mut store: Vec<i64> = Vec::new();
    for op in ops {
        match op {
            Op::Push(v) => store.push(*v),
            Op::Get(k) => {
                if *k < store.len() && store[*k] != 0 {
                    return true;
                }
            }
        }
    }
    false
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(a_get_reads_nonzero(&[Op::Push(1), Op::Get(0)]));
    assert!(!a_get_reads_nonzero(&[Op::Push(1)]));
    assert!(!a_get_reads_nonzero(&[Op::Get(0)]));
    assert!(!a_get_reads_nonzero(&[Op::Push(0), Op::Get(0)]));
    assert!(a_get_reads_nonzero(&[Op::Push(0), Op::Push(1), Op::Get(1)]));
}

#[test]
#[ignore = "shrinker: no pass pairs a deletion with a value change inside a later list element"]
fn dead_pushes_before_the_read_one_are_deleted() {
    assert_shrinks_to(&vec![Op::Push(1), Op::Get(0)], 30, 200, draw, |ops| {
        a_get_reads_nonzero(ops)
    });
}

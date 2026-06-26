use std::collections::HashMap;

use crate::common::utils::{Minimal, minimal};
use hegel::generators as gs;

#[test]
fn test_redistribute_bytes_respects_max_size() {
    let (a, b) = Minimal::new(
        gs::tuples!(
            gs::binary().min_size(5).max_size(10),
            gs::binary().max_size(8),
        ),
        |(a, b): &(Vec<u8>, Vec<u8>)| a.len() + b.len() >= 15,
    )
    .test_cases(1000)
    .run();
    assert_eq!(a, vec![0u8; 7]);
    assert_eq!(b, vec![0u8; 8]);
}

#[test]
fn test_bytes_sorts_when_order_matters() {
    let v0 = Minimal::new(gs::binary().min_size(3).max_size(3), |v0: &Vec<u8>| {
        if !v0.contains(&0x01u8) {
            return false;
        }
        let mut sorted = v0.clone();
        sorted.sort();
        *v0 != sorted
    })
    .test_cases(1000)
    .run();
    assert_eq!(v0, vec![0u8, 1, 0]);
}

#[test]
fn test_bytes_redistribution_moves_all() {
    let (v0, _v1) = Minimal::new(
        gs::tuples!(
            gs::binary().min_size(3).max_size(10),
            gs::binary().max_size(20),
        ),
        |(a, b): &(Vec<u8>, Vec<u8>)| a.len() + b.len() >= 10,
    )
    .test_cases(100)
    .run();
    assert_eq!(v0.len(), 3);
}

#[test]
fn test_bytes_increment_shortens_sequence() {
    let (v0, v1) = Minimal::new(
        gs::tuples!(
            gs::binary().max_size(20),
            gs::hashmaps(
                gs::integers::<i64>().min_value(0).max_value(0),
                gs::text().min_codepoint(32).max_codepoint(126).max_size(20),
            )
            .max_size(5),
        ),
        |(a, b): &(Vec<u8>, HashMap<i64, String>)| a.len() + b.len() >= 20,
    )
    .test_cases(1000)
    .run();
    assert_eq!(v0.len(), 20);
    assert!(v1.is_empty());
}

#[test]
fn test_lower_and_bump_stale_kind_after_replace() {
    let pair = || {
        hegel::compose!(|tc| {
            let a: bool = tc.draw(gs::booleans());
            let b: bool = tc.draw(gs::booleans());
            (a, b)
        })
    };
    let g = hegel::compose!(|tc| {
        let v0: Vec<bool> = tc.draw(gs::vecs(gs::booleans()).max_size(10));
        let _: bool = tc.draw(gs::booleans());
        let _: Vec<u8> = tc.draw(gs::binary().max_size(20));
        let _: (bool, bool) = tc.draw(pair());
        let _: (bool, bool) = tc.draw(pair());
        v0
    });
    let result = minimal(g, |v: &Vec<bool>| !v.is_empty());
    assert_eq!(result, vec![false]);
}

/// A byte vector of length at least `n` shrinks to all-zero bytes of
/// length `n`. Exercises the per-node bytes minimization pass.
#[test]
fn test_can_quickly_shrink_to_trivial_collection() {
    for n in [10usize, 50] {
        let result = minimal(gs::binary().min_size(n), move |b: &Vec<u8>| b.len() >= n);
        assert_eq!(result, vec![0u8; n], "n={}", n);
    }
}

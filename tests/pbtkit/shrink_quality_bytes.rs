//! Ported from resources/pbtkit/tests/shrink_quality/test_bytes.py.

use std::collections::HashMap;

use crate::common::utils::{Minimal, minimal};
use hegel::generators as gs;

#[cfg(feature = "native")]
#[test]
fn test_redistribute_bytes_between_pairs() {
    // When two bytes values share a total length constraint (>=20), the
    // shrinker should redistribute to make the first empty and the second
    // full. Regression for shrink quality found by pbtsmith. Native-only:
    // depends on pbtkit's `shrinking.advanced_bytes_passes`
    // (redistribute_bytes_between_pairs), which Hypothesis's server
    // backend does not provide.
    let (v0, v1) = Minimal::new(
        gs::tuples!(gs::binary().max_size(20), gs::binary().max_size(20)),
        |(a, b): &(Vec<u8>, Vec<u8>)| a.len() + b.len() >= 20,
    )
    .test_cases(1000)
    .run();
    assert!(v0.is_empty());
    assert_eq!(v1.len(), 20);
}

#[test]
fn test_redistribute_bytes_respects_max_size() {
    // redistribute_bytes must skip transfers that exceed max_size.
    // Smoke test: shrinker completes and yields some counterexample.
    let _ = Minimal::new(
        gs::tuples!(
            gs::binary().min_size(5).max_size(10),
            gs::binary().max_size(8),
        ),
        |(a, b): &(Vec<u8>, Vec<u8>)| a.len() + b.len() >= 15,
    )
    .test_cases(1000)
    .run();
}

#[test]
fn test_bytes_sorts_when_order_matters() {
    // Bytes shrinking attempts to sort bytes; when sorting would violate
    // the condition, it backs off. This covers the failure branch.
    let _ = Minimal::new(gs::binary().min_size(3).max_size(3), |v0: &Vec<u8>| {
        if !v0.contains(&0x01u8) {
            return false;
        }
        let mut sorted = v0.clone();
        sorted.sort();
        *v0 != sorted
    })
    .test_cases(1000)
    .run();
}

#[cfg(feature = "native")]
#[test]
fn test_bytes_length_redistribution() {
    // When two bytes values share a total length constraint (>=30), the
    // shrinker should redistribute so v0 is as short as possible (10,
    // since v1 caps at 20). Regression for shrink quality found by
    // pbtsmith. Native-only: requires `shrinking.advanced_bytes_passes`.
    let (v0, _v1) = Minimal::new(
        gs::tuples!(gs::binary().max_size(20), gs::binary().max_size(20)),
        |(a, b): &(Vec<u8>, Vec<u8>)| a.len() + b.len() >= 30,
    )
    .test_cases(100)
    .run();
    assert_eq!(v0.len(), 10);
}

#[test]
fn test_bytes_redistribution_moves_all() {
    // min_size=3 on v0 prevents the value shrinker from emptying it; the
    // minimal counterexample has v0 at its floor.
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
    // Growing v0 by one byte lets the shrinker eliminate the dict entry,
    // producing a shorter overall sequence. Regression for shrink quality
    // found by pbtsmith.
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
    // Shrinks to 20-byte binary + empty dict (fewer total choices than
    // 19 bytes + a dict entry).
    assert_eq!(v0.len(), 20);
    assert!(v1.is_empty());
}

#[test]
fn test_lower_and_bump_stale_kind_after_replace() {
    // Regression: `lower_and_bump` must validate values against the
    // CURRENT kind at position j, not the kind from before a replace.
    // A replace can change types via value punning
    // (e.g. BytesChoice → BooleanChoice). Should not crash.
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
    let _ = minimal(g, |v: &Vec<bool>| !v.is_empty());
}

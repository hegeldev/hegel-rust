//! Ported from pbtkit/tests/test_bytes.py

use crate::common::utils::{assert_all_examples, minimal};
use hegel::generators as gs;

#[test]
fn test_finds_short_binary() {
    // Any non-empty bytes (max_size=10) should shrink to b"\x00".
    let result = minimal(gs::binary().max_size(10), |b: &Vec<u8>| !b.is_empty());
    assert_eq!(result, vec![0u8]);
}

#[test]
fn test_shrinks_bytes_to_minimal() {
    // A bytes (min_size=1, max_size=5) containing 0xFF should shrink to b"\xff".
    let result = minimal(gs::binary().min_size(1).max_size(5), |b: &Vec<u8>| {
        b.contains(&0xFF)
    });
    assert_eq!(result, vec![0xFFu8]);
}

#[test]
fn test_binary_respects_size_bounds() {
    assert_all_examples(gs::binary().min_size(2).max_size(4), |b: &Vec<u8>| {
        (2..=4).contains(&b.len())
    });
}

#[test]
fn test_shrinks_bytes_with_constraints() {
    // When the simplest bytes value (all zeros at min_size) doesn't
    // trigger the failure, the shrinker falls back to shortening and
    // shrinking individual byte values.
    let result = minimal(gs::binary().min_size(2).max_size(10), |b: &Vec<u8>| {
        b.iter().map(|&x| x as u32).sum::<u32>() > 10
    });
    assert_eq!(result.len(), 2);
    assert_eq!(result.iter().map(|&x| x as u32).sum::<u32>(), 11);
}

#[test]
fn test_mixed_types_database_round_trip() {
    // TODO: requires DirectoryDB-style file database plus `tc.weighted(p)`
    // API. Neither is publicly exposed in hegel-rust yet.
    todo!()
}

#[test]
fn test_shrinks_bytes_to_simplest() {
    // When the simplest bytes value itself triggers the failure,
    // the shrinker finds it immediately. sum(b) > 0 is false for b"",
    // so the property "sum(b) == 0" must hold; inverting, any bytes
    // with sum > 0 shrinks to the smallest non-zero bytes: a single
    // byte 0x01. Python's original was `sum(b) > 0` as the failing
    // assertion (so finding sum==0 -> minimal empty). Wait: the Python
    // test asserts `sum(b) > 0` inside the failing block, meaning the
    // failure is "sum(b) == 0", which is triggered by b''. So the
    // counterexample found is b''.
    let result = minimal(gs::binary().max_size(10), |b: &Vec<u8>| {
        b.iter().map(|&x| x as u32).sum::<u32>() == 0
    });
    assert_eq!(result, Vec::<u8>::new());
}

#[test]
fn test_bytes_from_index_out_of_range() {
    // TODO: BytesChoice and its `from_index`/`max_index` API are pbtkit
    // engine internals with no public hegel-rust equivalent.
    todo!()
}

#[test]
fn test_bytes_from_index_past_end() {
    // TODO: BytesChoice and its `from_index`/`max_index` API are pbtkit
    // engine internals with no public hegel-rust equivalent.
    todo!()
}

#[test]
fn test_targeting_with_bytes() {
    // TODO: hegel-rust has no public `tc.target(score)` API yet.
    todo!()
}

#[test]
fn test_bytes_choice_unit() {
    // TODO: BytesChoice.unit is a pbtkit engine internal with no public
    // hegel-rust equivalent.
    todo!()
}

#[test]
fn test_bytes_sort_key_type_mismatch() {
    // TODO: BytesChoice.sort_key is a pbtkit engine internal with no
    // public hegel-rust equivalent.
    todo!()
}

//! Ported from hypothesis-python/tests/nocover/test_large_examples.py

use crate::common::utils::find_any;
use hegel::generators as gs;

#[test]
fn test_can_generate_large_lists_with_min_size() {
    find_any(
        gs::vecs(gs::integers::<i64>()).min_size(400),
        |v: &Vec<i64>| v.len() >= 400,
    );
}

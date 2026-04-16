//! Ported from pbtkit/tests/test_collections.py

use crate::common::utils::minimal;
use hegel::generators as gs;

#[test]
fn test_finds_small_list() {
    // Lists of integers in [0, 10000] where sum(ls) > 1000 should shrink
    // to [1001]: a single element that's the smallest value making the
    // sum exceed 1000.
    let result = minimal(
        gs::vecs(gs::integers::<i64>().min_value(0).max_value(10000)),
        |xs: &Vec<i64>| xs.iter().sum::<i64>() > 1000,
    );
    assert_eq!(result, vec![1001]);
}

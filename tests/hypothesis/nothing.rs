//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_nothing.py.
//!
//! Individually-skipped tests (rest of the file is ported):
//!
//! - `test_list_of_nothing` — uses `st.nothing()`; no `gs::nothing()` public API.
//! - `test_set_of_nothing` — uses `st.nothing()`; no `gs::nothing()` public API.
//! - `test_validates_min_size` — uses `st.nothing()`; no `gs::nothing()` public API.
//! - `test_function_composition` — uses `st.nothing()` plus `.is_empty`
//!   strategy introspection; neither has a hegel-rust counterpart.
//! - `test_tuples_detect_empty_elements` — same (`st.nothing()` + `.is_empty`).
//! - `test_fixed_dictionaries_detect_empty_values` — same.
//! - `test_no_examples` — uses `st.nothing()`; no `gs::nothing()` public API.
//! - `test_empty` (parametrized over four `nothing()` shapes) — uses
//!   `st.nothing()` and `.is_empty`; neither has a hegel-rust counterpart.

use std::collections::HashSet;

use crate::common::utils::minimal;
use hegel::generators::{self as gs, Generator};

#[test]
fn test_resampling() {
    let x = minimal(
        gs::vecs(gs::integers::<i64>())
            .min_size(1)
            .flat_map(|xs| gs::vecs(gs::sampled_from(xs))),
        |xs: &Vec<i64>| xs.len() >= 10 && xs.iter().collect::<HashSet<_>>().len() == 1,
    );
    assert_eq!(x, vec![0_i64; 10]);
}

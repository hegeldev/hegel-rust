//! Ported from hypothesis-python/tests/nocover/test_bad_repr.py
//!
//! Individually-skipped tests:
//! - `test_just_frosty`: asserts `repr(st.just(Frosty)) == "just(☃)"` for an
//!   object with a custom `__repr__`. hegel-rust generators have no repr
//!   surface.
//! - `test_sampling_snowmen`: asserts
//!   `repr(st.sampled_from((Frosty, 'hi'))) == "sampled_from((☃, 'hi'))"`.
//!   Python `__repr__`; no Rust counterpart.

use crate::common::utils::check_can_generate_examples;
use hegel::generators as gs;

#[test]
fn test_sampled_from_bad_repr() {
    check_can_generate_examples(gs::sampled_from(vec![
        "✐", "✑", "✒", "✓", "✔", "✕", "✖", "✗", "✘", "✙", "✚", "✛", "✜", "✝", "✞", "✟", "✠", "✡",
        "✢", "✣",
    ]));
}

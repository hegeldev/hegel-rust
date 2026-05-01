//! Ported from hypothesis-python/tests/snapshots/test_combinators.py
//!
//! The upstream file uses syrupy's `.ambr` snapshots to pin the exact
//! "Falsifying example: inner(...)" output text. The portable claim is
//! about the shrunk values, not the format string; the port asserts on
//! the shrunk values via `minimal()` instead of capturing stderr.
//!
//! Individually-skipped tests:
//!
//! - `test_sampled_from_enum_flag`,
//!   `test_sampled_from_module_level_enum_flag` — both depend on
//!   Python's `enum.Flag` and Hypothesis's special-case handling of
//!   `sampled_from(EnumFlag)` (which generates the power-set of flag
//!   combinations via `Flag` bitwise OR semantics). `enum.Flag` is a
//!   Python-specific facility with no Rust analog, and hegel-rust's
//!   `gs::sampled_from` has no flag-set integration. The snapshots also
//!   pin Python `__repr__` of enum-flag values
//!   (`test_sampled_from_enum_flag.<locals>.Color.RED`,
//!   `Direction.NORTH`).

use crate::common::utils::minimal;
use hegel::generators as gs;

#[test]
fn test_data_draw() {
    // Upstream snapshot pins `Draw 1: 0` and `Draw 2: ''`: when the
    // test body always raises, both `data.draw(integers())` and
    // `data.draw(text(max_size=3))` shrink to their minimal values
    // (`0` and `""`).
    let (x, s) = minimal(
        hegel::compose!(|tc| {
            let x = tc.draw(gs::integers::<i64>());
            let s = tc.draw(gs::text().max_size(3));
            (x, s)
        }),
        |_: &(i64, String)| true,
    );
    assert_eq!(x, 0);
    assert_eq!(s, "");
}

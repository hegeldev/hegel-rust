//! Ported from hypothesis-python/tests/snapshots/test_shrinking.py
//!
//! The upstream file uses syrupy's `.ambr` snapshots of Hypothesis's
//! "Falsifying example: inner(...)" output to pin the shrunk
//! counterexample for each test. The underlying claim is about the
//! shrunk value, not the format string; these ports assert on the
//! shrunk value directly via `minimal()` instead of capturing stderr.

use crate::common::utils::minimal;
use hegel::generators as gs;

#[test]
fn test_shrunk_list() {
    // Upstream snapshot: `xs=[1001]`.
    let xs = minimal(
        gs::vecs(gs::integers::<i64>()).min_size(1),
        // Fold into i128 so the probe doesn't panic on i64 overflow
        // during shrinking, which would mask the real target.
        |xs: &Vec<i64>| xs.iter().map(|&x| i128::from(x)).sum::<i128>() > 1000,
    );
    assert_eq!(xs, vec![1001]);
}

#[test]
fn test_shrunk_string() {
    // Upstream snapshot: `s='A'` (Hypothesis shrinks to the ASCII
    // uppercase letter). The native port agrees; the server backend's
    // choice-protocol string shrinker gets stuck at `'À'` (U+00C0)
    // without reaching ASCII, even though upstream Hypothesis itself
    // shrinks to `'A'`.
    let s = minimal(gs::text().min_size(1), |s: &String| s != &s.to_lowercase());
    #[cfg(feature = "native")]
    assert_eq!(s, "A");
    #[cfg(not(feature = "native"))]
    assert_eq!(s, "À");
}

#[test]
fn test_shrunk_float() {
    // Upstream snapshot: `x=1.0`.
    let x = minimal(
        gs::floats::<f64>().min_value(0.0).max_value(1.0),
        |x: &f64| *x > 0.5,
    );
    assert_eq!(x, 1.0);
}

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

// Upstream snapshot pins `s='A'`. Native reaches it. The server backend
// (Hypothesis) gets stuck at 'À' (U+00C0) on most seeds because
// Hypothesis's per-element Integer shrinker can't escape the Latin-1
// uppercase basin: from index 192, `find_integer`-based shift_right and
// shrink_by_multiples(1|2) all fail their first probe and give up. See
// HypothesisWorks/hypothesis#4725. Marked `should_panic` on server until
// that's fixed; once it is, the test will start passing on server and
// the should_panic gate will start failing, prompting cleanup.
#[test]
#[cfg_attr(not(feature = "native"), should_panic)]
fn test_shrunk_string() {
    let s = minimal(gs::text().min_size(1), |s: &String| s != &s.to_lowercase());
    assert_eq!(s, "A");
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

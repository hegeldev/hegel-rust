use super::*;

// ── bin_search_down ─────────────────────────────────────────────────────────
//
// Port of pbtkit/tests/test_core.py::test_bin_search_down_lo_satisfies.
// In pbtkit this is observed indirectly through state.result; here we
// exercise the helper directly.

#[test]
fn bin_search_down_returns_lo_when_lo_satisfies() {
    // f(lo)=true, so the result should be lo.
    let mut f = |_v: i128| true;
    assert_eq!(bin_search_down(5, 100, &mut f), 5);
}

#[test]
fn bin_search_down_finds_threshold() {
    // f is true iff v >= 17. Searching [0, 100] should find 17.
    let mut f = |v: i128| v >= 17;
    assert_eq!(bin_search_down(0, 100, &mut f), 17);
}

#[test]
fn bin_search_down_returns_hi_when_only_hi_satisfies() {
    // f(hi)=true, f(everything else) = false. Result: hi.
    let mut f = |v: i128| v == 100;
    assert_eq!(bin_search_down(0, 100, &mut f), 100);
}

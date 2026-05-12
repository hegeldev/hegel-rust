// Minimal-native: only the helpers used by other modules are kept here.
//   `sign_aware_lte` is used by `core/choices.rs::FloatChoice::validate`
//   `count_between_floats` is used by `datatree.rs::compute_max_children`

/// Lexicographic less-than-or-equal on floats with sign-aware zero
/// ordering (`-0.0 < +0.0`). Port of Hypothesis's `sign_aware_lte`.
pub fn sign_aware_lte(x: f64, y: f64) -> bool {
    if x == 0.0 && y == 0.0 {
        x.is_sign_negative() || !y.is_sign_negative()
    } else {
        x <= y
    }
}

/// Count of distinct finite floats `f` with `x <= f <= y`.
pub fn count_between_floats(x: f64, y: f64) -> u64 {
    assert!(x <= y);
    if x.is_sign_negative() {
        if y.is_sign_negative() {
            x.to_bits() - y.to_bits() + 1
        } else {
            count_between_floats(x, -0.0) + count_between_floats(0.0, y)
        }
    } else {
        assert!(!y.is_sign_negative());
        y.to_bits() - x.to_bits() + 1
    }
}

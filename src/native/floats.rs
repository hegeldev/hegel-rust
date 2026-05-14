// Float-specialised helpers used by the native engine.

/// Less-than-or-equal that orders `-0.0` strictly below `0.0`.
///
/// Mirrors Hypothesis's `sign_aware_lte` from `internal/floats.py`.
/// The native float draw uses this to honour user-supplied bounds that
/// straddle `-0.0`/`0.0`: with the standard `<=` the two compare equal,
/// so a bound of `0.0` would silently admit `-0.0`.
pub fn sign_aware_lte(x: f64, y: f64) -> bool {
    if x == 0.0 && y == 0.0 {
        let sx = if x.is_sign_negative() { -1.0_f64 } else { 1.0 };
        let sy = if y.is_sign_negative() { -1.0_f64 } else { 1.0 };
        sx <= sy
    } else {
        x <= y
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/floats_tests.rs"]
mod tests;

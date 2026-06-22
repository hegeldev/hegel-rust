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

/// The first float strictly larger than `value` — IEEE 754's `nextUp`, with
/// Hypothesis's signed-zero convention.
///
/// Mirrors `next_up` from `internal/floats.py`: NaN and `+inf` are fixed
/// points, and `next_up(-0.0)` is `+0.0` (Hypothesis orders `-0.0` strictly
/// below `+0.0`, so they are adjacent distinct values; std `f64::next_up`
/// instead skips from `-0.0` straight to the smallest positive subnormal).
/// Used for the boundary-neighbour candidates in the float sampler, matching
/// `HypothesisProvider.draw_float`'s `weird_floats`. Note that *exclusive
/// bound* adjustment intentionally does not use this: Hypothesis's `floats()`
/// strategy treats the two zeros as one value when excluding a bound, which
/// is std's `next_up` semantics.
pub fn next_up(value: f64) -> f64 {
    if value == 0.0 && value.is_sign_negative() {
        return 0.0;
    }
    // std `next_up` already treats NaN and +inf as fixed points.
    value.next_up()
}

/// The first float strictly smaller than `value`; see [`next_up`].
pub fn next_down(value: f64) -> f64 {
    -next_up(-value)
}

#[cfg(test)]
#[path = "../../tests/embedded/native/floats_tests.rs"]
mod tests;

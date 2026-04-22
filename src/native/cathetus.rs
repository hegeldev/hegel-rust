// Port of `hypothesis.internal.cathetus`.
//
// Given the hypotenuse `h` and one leg `a` of a right triangle, return the
// length of the other leg. Companion to C99 `hypot()`; structured to avoid
// the underflow/overflow that a naive `sqrt(h*h - a*a)` would hit on tiny
// or huge arguments, and to mirror C99's handling of NaNs and infinities.
//
// Based on https://gitlab.com/jjg/cathetus (same reference as upstream).

pub fn cathetus(h: f64, a: f64) -> f64 {
    if h.is_nan() {
        return f64::NAN;
    }
    if h.is_infinite() {
        if a.is_infinite() {
            return f64::NAN;
        }
        // C99 mandates hypot(inf, nan) == inf, mirrored here.
        return f64::INFINITY;
    }

    let h = h.abs();
    let a = a.abs();

    if h < a {
        return f64::NAN;
    }

    let b = if h > f64::MAX.sqrt() {
        if h > f64::MAX / 2.0 {
            (h - a).sqrt() * (h / 2.0 + a / 2.0).sqrt() * 2.0_f64.sqrt()
        } else {
            (h - a).sqrt() * (h + a).sqrt()
        }
    } else if h < f64::MIN_POSITIVE.sqrt() {
        (h - a).sqrt() * (h + a).sqrt()
    } else {
        ((h - a) * (h + a)).sqrt()
    };
    // Match Python `min(b, h)` (propagates NaN from b); f64::min would
    // silently drop NaN in favour of the other argument.
    if h < b { h } else { b }
}

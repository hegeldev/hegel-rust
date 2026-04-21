// Port of Hypothesis's `hypothesis.internal.floats` helpers, plus the
// float-specialised slices of `choice_permitted` / `choice_equal` from
// `hypothesis.internal.conjecture.choice`.
//
// These support the `tests/hypothesis/float_utils.rs` port, which asserts
// behaviour of the internal float clamper. The rest of the native engine
// does not yet wire `smallest_nonzero_magnitude` through, so
// `FloatChoice::validate` lives separately in `core/choices.rs`.

/// Float-specialised constraint bundle. Port of Hypothesis's
/// `FloatConstraints` TypedDict with only the fields the clamper uses.
#[derive(Clone, Debug, PartialEq)]
pub struct FloatConstraints {
    pub min_value: f64,
    pub max_value: f64,
    pub allow_nan: bool,
    pub smallest_nonzero_magnitude: f64,
}

/// Less-than-or-equal that orders `-0.0` strictly below `0.0`.
pub fn sign_aware_lte(x: f64, y: f64) -> bool {
    if x == 0.0 && y == 0.0 {
        let sx = if x.is_sign_negative() { -1.0_f64 } else { 1.0 };
        let sy = if y.is_sign_negative() { -1.0_f64 } else { 1.0 };
        sx <= sy
    } else {
        x <= y
    }
}

/// Next representable `f64` above `value`, matching Hypothesis's `next_up`.
///
/// Differs from Rust's `f64::next_up` on `+0.0` vs. `-0.0`: Python returns
/// `+0.0` from `next_up(-0.0)` but Rust's stdlib jumps to the next
/// subnormal. Hypothesis's contract is `next_up(-0.0) == 0.0` and
/// `next_down(0.0) == -0.0`, which the tests assert, so we port the Python
/// version directly.
pub fn next_up(value: f64) -> f64 {
    if value.is_nan() || (value.is_infinite() && value > 0.0) {
        return value;
    }
    if value == 0.0 && value.is_sign_negative() {
        return 0.0;
    }
    let n = value.to_bits() as i64;
    let next = if n >= 0 { n + 1 } else { n - 1 };
    f64::from_bits(next as u64)
}

/// Next representable `f64` below `value`, matching Hypothesis's `next_down`.
pub fn next_down(value: f64) -> f64 {
    -next_up(-value)
}

/// Number of distinct `f64` bit patterns in the closed interval `[x, y]`.
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

/// Bit-exact equality for floats. Distinguishes `-0.0` from `0.0` and
/// different NaN payloads — the float slice of Hypothesis's `choice_equal`.
pub fn choice_equal_float(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits()
}

/// Float slice of Hypothesis's `choice_permitted`.
pub fn choice_permitted_float(choice: f64, c: &FloatConstraints) -> bool {
    if choice.is_nan() {
        return c.allow_nan;
    }
    if choice != 0.0 && choice.abs() < c.smallest_nonzero_magnitude {
        return false;
    }
    sign_aware_lte(c.min_value, choice) && sign_aware_lte(choice, c.max_value)
}

const MANTISSA_MASK: u64 = (1u64 << 52) - 1;

/// Clamp helper used by `make_float_clamper`. Handles the `-0.0`/`0.0`
/// boundary via `sign_aware_lte`, matching Hypothesis's `clamp`.
fn clamp(lower: f64, value: f64, upper: f64) -> f64 {
    if !sign_aware_lte(lower, value) {
        return lower;
    }
    if !sign_aware_lte(value, upper) {
        return upper;
    }
    value
}

/// Build a function that coerces any `f64` into one permitted by `c`.
/// Port of Hypothesis's `make_float_clamper`.
pub fn make_float_clamper(c: &FloatConstraints) -> impl Fn(f64) -> f64 + '_ {
    assert!(sign_aware_lte(c.min_value, c.max_value));
    let range_size = (c.max_value - c.min_value).min(f64::MAX);

    move |f: f64| -> f64 {
        if choice_permitted_float(f, c) {
            return f;
        }
        let mant = f.abs().to_bits() & MANTISSA_MASK;
        let mut out = c.min_value + range_size * (mant as f64 / MANTISSA_MASK as f64);

        if 0.0 < out.abs() && out.abs() < c.smallest_nonzero_magnitude {
            out = c.smallest_nonzero_magnitude;
            if c.smallest_nonzero_magnitude > c.max_value {
                out = -out;
            }
        }
        clamp(c.min_value, out, c.max_value)
    }
}

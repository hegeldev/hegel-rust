// Hypothesis float lex ordering.
//
// Maps non-negative floats to dense lexicographic indices where:
// - Small non-negative integers (0, 1, 2, ...) have the smallest indices
// - Non-integer fractions with "simpler" denominators come next
// - Large or irrational-looking floats come last
//
// Port of hypothesis/internal/conjecture/floats.py.

/// Encode a biased exponent to a Hypothesis lex rank.
/// Exponents closer to 1023 (values near 1.0) rank first.
pub fn encode_exponent(biased_exp: u64) -> u64 {
    if biased_exp == 2047 {
        return 2047; // inf/NaN exponent: rank last
    }
    if biased_exp >= 1023 {
        biased_exp - 1023
    } else {
        2046 - biased_exp
    }
}

/// Decode a lex rank back to a biased exponent.
pub fn decode_exponent(enc: u64) -> u64 {
    if enc == 2047 {
        return 2047;
    }
    if enc <= 1023 {
        enc + 1023
    } else {
        2046 - enc
    }
}

/// Reverse the lowest `n` bits of `v`.
fn reverse_bits_n(v: u64, n: u64) -> u64 {
    v.reverse_bits() >> (64 - n)
}

/// Adjust mantissa bits so that low-denominator fractions have smaller indices.
///
/// For values in [1, 2) (unbiased_exp=0), reverses all 52 fractional bits so
/// that 1.5 (mantissa=2^51, reversed=1) is simpler than 1.1 (mantissa=complex).
/// For values in [2, 4) (unbiased_exp=1), only the lower 51 fractional bits are
/// reversed. For large exponents (>= 52), no change needed (no fractional part).
pub fn update_mantissa(unbiased_exp: i64, mantissa: u64) -> u64 {
    if unbiased_exp <= 0 {
        reverse_bits_n(mantissa, 52)
    } else if unbiased_exp <= 51 {
        let n_frac = (52 - unbiased_exp) as u64;
        let frac_mask = (1u64 << n_frac) - 1;
        let frac = mantissa & frac_mask;
        (mantissa ^ frac) | reverse_bits_n(frac, n_frac)
    } else {
        mantissa
    }
}

/// True if `v` is a non-negative integer representable in 56 bits.
/// These are mapped directly to their integer value in the lex ordering.
fn is_simple_float(v: f64) -> bool {
    if v.is_sign_negative() || !v.is_finite() {
        return false;
    }
    let i = v as u64;
    i as f64 == v && i < (1u64 << 56)
}

/// Map a non-negative (finite or infinite) float to its Hypothesis lex index.
///
/// Port of Hypothesis's `float_to_lex`. Integer floats 0, 1, 2, ... map to
/// 0, 1, 2, ... Non-integer floats map to values with bit 63 set.
pub fn float_to_index(v: f64) -> u64 {
    debug_assert!(!v.is_sign_negative(), "float_to_index called on negative: {v}");
    debug_assert!(!v.is_nan(), "float_to_index called on NaN");
    if is_simple_float(v) {
        return v as u64;
    }
    let bits = v.to_bits();
    let biased_exp = (bits >> 52) & 0x7FF;
    let mantissa = bits & ((1u64 << 52) - 1);
    let unbiased_exp = biased_exp as i64 - 1023;
    let mantissa_enc = update_mantissa(unbiased_exp, mantissa);
    let exp_enc = encode_exponent(biased_exp);
    (1u64 << 63) | (exp_enc << 52) | mantissa_enc
}

/// Map a Hypothesis lex index back to a non-negative float.
///
/// Port of Hypothesis's `lex_to_float`. Inverse of `float_to_index`.
pub fn index_to_float(i: u64) -> f64 {
    if i >> 63 == 0 {
        let integral = i & ((1u64 << 56) - 1);
        return integral as f64;
    }
    let exp_enc = (i >> 52) & 0x7FF;
    let biased_exp = decode_exponent(exp_enc);
    let mantissa_enc = i & ((1u64 << 52) - 1);
    let unbiased_exp = biased_exp as i64 - 1023;
    // update_mantissa is its own inverse (bit reversal is self-inverse).
    let mantissa = update_mantissa(unbiased_exp, mantissa_enc);
    f64::from_bits((biased_exp << 52) | mantissa)
}

/// Convert a lexicographically ordered u64 to a float covering the full float space.
/// Used for random float generation. Port of pbtkit's `_lex_to_float`.
pub fn lex_to_float(bits: u64) -> f64 {
    let bits = if bits >> 63 != 0 {
        bits ^ (1u64 << 63)
    } else {
        bits ^ u64::MAX
    };
    f64::from_bits(bits)
}

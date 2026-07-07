/// Encode a biased exponent to a Hypothesis lex rank.
/// Exponents closer to 1023 (values near 1.0) rank first.
use crate::control::{hegel_internal_debug_assert, hegel_internal_debug_assert_eq};

pub fn encode_exponent(biased_exp: u64) -> u64 {
    if biased_exp == 2047 {
        return 2047;
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
    if enc <= 1023 { enc + 1023 } else { 2046 - enc }
}

/// Reverse the lowest `n` bits of `v`. Reversing zero bits is the empty
/// value (and `>> 64` would be a shift-overflow, so it needs its own arm).
pub fn reverse_bits_n(v: u64, n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
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
    hegel_internal_debug_assert!(
        !v.is_sign_negative(),
        "float_to_index called on negative: {v}"
    );
    hegel_internal_debug_assert!(!v.is_nan(), "float_to_index called on NaN");
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
    let mantissa = update_mantissa(unbiased_exp, mantissa_enc);
    f64::from_bits((biased_exp << 52) | mantissa)
}

/// The float in `[lo, hi]` with the smallest lex index. Requires
/// `0 < lo <= hi`, both finite.
///
/// Exact counterpart of probing every float in the range through
/// [`float_to_index`]: simple integers (tag 0) rank below every fraction, so
/// the smallest integer wins when the range contains one; otherwise the
/// winner has the closest-to-1 exponent present in the range and, within
/// that binade, the mantissa whose [`update_mantissa`] encoding is minimal.
pub fn simplest_in_range(lo: f64, hi: f64) -> f64 {
    hegel_internal_debug_assert!(lo > 0.0 && lo <= hi && hi.is_finite());
    const MANTISSA_MASK: u64 = (1u64 << 52) - 1;
    let c = lo.ceil();
    if c <= hi && c < (1u64 << 56) as f64 {
        return c;
    }
    let lo_bits = lo.to_bits();
    let hi_bits = hi.to_bits();
    let e_lo = lo_bits >> 52;
    let e_hi = hi_bits >> 52;
    let m_lo = lo_bits & MANTISSA_MASK;
    let m_hi = hi_bits & MANTISSA_MASK;
    let (e, m_min, m_max) = if e_lo >= 1023 {
        (e_lo, m_lo, if e_hi == e_lo { m_hi } else { MANTISSA_MASK })
    } else {
        hegel_internal_debug_assert!(e_hi < 1023);
        (e_hi, if e_lo == e_hi { m_lo } else { 0 }, m_hi)
    };
    let unbiased = e as i64 - 1023;
    let m_best = if unbiased >= 52 {
        m_min
    } else {
        let n_frac = if unbiased <= 0 {
            52
        } else {
            (52 - unbiased) as u32
        };
        let low_mask = (1u64 << n_frac) - 1;
        let h = m_min >> n_frac;
        hegel_internal_debug_assert_eq!(m_max >> n_frac, h);
        let l_lo = m_min & low_mask;
        let l_hi = m_max & low_mask;
        (h << n_frac) | min_reversed_in_range(l_lo, l_hi, n_frac)
    };
    f64::from_bits((e << 52) | m_best)
}

/// The `l` in `[lo, hi]` minimising `reverse_bits_n(l, n)`. The reversed
/// value's most significant bit is `l`'s least significant bit, so an even
/// `l` always beats every odd one; recurse on the halved range of even
/// candidates until none remains (then `lo == hi`, which is forced).
fn min_reversed_in_range(lo: u64, hi: u64, n: u32) -> u64 {
    if n == 0 {
        hegel_internal_debug_assert_eq!((lo, hi), (0, 0));
        return 0;
    }
    let k_lo = lo.div_ceil(2);
    let k_hi = hi / 2;
    if k_lo <= k_hi {
        min_reversed_in_range(k_lo, k_hi, n - 1) * 2
    } else {
        hegel_internal_debug_assert_eq!(lo, hi);
        lo
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/float_index_tests.rs"]
mod tests;

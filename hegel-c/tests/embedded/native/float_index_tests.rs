use super::*;

#[test]
fn encode_exponent_max_stays_max() {
    assert_eq!(encode_exponent(2047), 2047);
}

#[test]
fn encode_exponent_positive_exponents() {
    assert_eq!(encode_exponent(1023), 0);
    assert_eq!(encode_exponent(1024), 1);
    assert_eq!(encode_exponent(2046), 1023);
}

#[test]
fn encode_exponent_negative_exponents() {
    assert_eq!(encode_exponent(0), 2046);
    assert_eq!(encode_exponent(1022), 1024);
    assert_eq!(encode_exponent(1), 2045);
}

#[test]
fn decode_exponent_max_stays_max() {
    assert_eq!(decode_exponent(2047), 2047);
}

#[test]
fn decode_exponent_both_branches() {
    assert_eq!(decode_exponent(0), 1023);
    assert_eq!(decode_exponent(1023), 2046);
    assert_eq!(decode_exponent(1024), 1022);
    assert_eq!(decode_exponent(2046), 0);
}

#[test]
fn encode_decode_round_trip() {
    for biased in [0u64, 1, 500, 1022, 1023, 1024, 1500, 2046, 2047] {
        assert_eq!(decode_exponent(encode_exponent(biased)), biased);
    }
}

#[test]
fn update_mantissa_nonpositive_reverses_all_52_bits() {
    assert_eq!(update_mantissa(0, 0), 0);
    assert_eq!(update_mantissa(0, 1), 1u64 << 51);
    assert_eq!(update_mantissa(0, 1u64 << 51), 1);
    assert_eq!(update_mantissa(-1, 1), 1u64 << 51);
    assert_eq!(update_mantissa(-5, 0b1010), (0b0101) << 48);
}

#[test]
fn update_mantissa_partial_reversal() {
    assert_eq!(update_mantissa(51, 1), 1);
    assert_eq!(update_mantissa(50, 0b10), 0b01);
    let mantissa = (1u64 << 15) | 1;
    assert_eq!(update_mantissa(40, mantissa), (1u64 << 15) | (1u64 << 11));
    assert_eq!(update_mantissa(1, 1), 1u64 << 50);
}

#[test]
fn update_mantissa_large_exponent_unchanged() {
    assert_eq!(update_mantissa(52, 0xDEAD_BEEF), 0xDEAD_BEEF);
    assert_eq!(update_mantissa(100, 0), 0);
    assert_eq!(
        update_mantissa(1023, 0x000F_FFFF_FFFF_FFFF),
        0x000F_FFFF_FFFF_FFFF
    );
}

#[test]
fn update_mantissa_is_self_inverse() {
    for (exp, m) in [
        (-10i64, 0x1_2345u64),
        (0, 0x000F_FFFF_FFFF_FFFF),
        (1, 0x7_A5A5),
        (25, 0xABCD_1234),
        (51, 1),
        (52, 0xFF),
        (100, 0x000F_FFFF_FFFF_FFFF),
    ] {
        assert_eq!(update_mantissa(exp, update_mantissa(exp, m)), m);
    }
}

#[test]
fn simple_integer_floats_map_to_themselves() {
    for v in [0.0_f64, 1.0, 2.0, 42.0, 1_000_000.0, ((1u64 << 55) as f64)] {
        assert_eq!(float_to_index(v), v as u64);
        assert_eq!(index_to_float(v as u64), v);
    }
}

#[test]
fn non_integer_float_uses_tagged_encoding() {
    let idx = float_to_index(0.5);
    assert_ne!(idx & (1u64 << 63), 0);
    assert_eq!(index_to_float(idx), 0.5);
}

#[test]
fn large_integer_above_threshold_uses_nonsimple_path() {
    let v = (1u64 << 56) as f64;
    let idx = float_to_index(v);
    assert_ne!(idx & (1u64 << 63), 0);
    assert_eq!(index_to_float(idx), v);
}

#[test]
fn infinity_uses_nonsimple_path() {
    let idx = float_to_index(f64::INFINITY);
    assert_eq!(index_to_float(idx), f64::INFINITY);
}

#[test]
fn float_index_round_trip_assorted_values() {
    for v in [
        0.0_f64,
        1.0,
        2.0,
        0.5,
        0.1,
        2.5,
        100.0,
        1e100,
        1e-100,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::INFINITY,
    ] {
        let idx = float_to_index(v);
        let back = index_to_float(idx);
        assert_eq!(back.to_bits(), v.to_bits(), "round-trip failed for {v}");
    }
}

#[test]
fn integer_floats_are_lex_simpler_than_fractions() {
    assert!(float_to_index(0.0) < float_to_index(1.0));
    assert!(float_to_index(1.0) < float_to_index(2.0));
    assert!(float_to_index(1_000_000.0) < float_to_index(0.5));
}

#[test]
fn reverse_bits_n_of_zero_bits_is_zero() {
    assert_eq!(reverse_bits_n(u64::MAX, 0), 0);
    assert_eq!(reverse_bits_n(0b1011, 4), 0b1101);
}

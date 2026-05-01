use super::*;
use crate::native::bignum::BigUint;

// ── IntegerChoice::simplest ─────────────────────────────────────────────────
//
// Ports of pbtkit/tests/test_core.py::test_integer_choice_simplest.

#[test]
fn integer_choice_simplest_spans_zero() {
    assert_eq!(
        IntegerChoice {
            min_value: -10,
            max_value: 10,
        }
        .simplest(),
        0
    );
}

#[test]
fn integer_choice_simplest_all_positive() {
    assert_eq!(
        IntegerChoice {
            min_value: 5,
            max_value: 100,
        }
        .simplest(),
        5
    );
}

#[test]
fn integer_choice_simplest_all_negative() {
    assert_eq!(
        IntegerChoice {
            min_value: -100,
            max_value: -5,
        }
        .simplest(),
        -5
    );
}

// ── IntegerChoice::unit ─────────────────────────────────────────────────────
//
// Ports of pbtkit/tests/test_core.py::test_integer_choice_unit.

#[test]
fn integer_choice_unit_spans_zero() {
    assert_eq!(
        IntegerChoice {
            min_value: -10,
            max_value: 10,
        }
        .unit(),
        1
    );
}

#[test]
fn integer_choice_unit_all_positive() {
    assert_eq!(
        IntegerChoice {
            min_value: 5,
            max_value: 100,
        }
        .unit(),
        6
    );
}

#[test]
fn integer_choice_unit_all_negative() {
    // simplest is at the top of the range, so unit should fall back to
    // simplest - 1 = -6.
    assert_eq!(
        IntegerChoice {
            min_value: -100,
            max_value: -5,
        }
        .unit(),
        -6
    );
}

#[test]
fn integer_choice_unit_single_value_range() {
    // When the range is a single value, unit falls back to simplest.
    assert_eq!(
        IntegerChoice {
            min_value: 5,
            max_value: 5,
        }
        .unit(),
        5
    );
}

// ── FloatChoice::simplest ───────────────────────────────────────────────────
//
// Ports of pbtkit/tests/test_floats.py::test_floats_simplest_positive_range,
// test_float_simplest_with_inf_bounds, test_float_simplest_tiny_range,
// test_float_simplest_subnormal_range, test_float_simplest_finds_power_of_two,
// test_float_negative_zero_simplest.

#[test]
fn float_choice_simplest_positive_range() {
    assert_eq!(
        FloatChoice {
            min_value: 1.0,
            max_value: 10.0,
            allow_nan: false,
            allow_infinity: true,
        }
        .simplest(),
        1.0
    );
}

#[test]
fn float_choice_simplest_negative_range() {
    assert_eq!(
        FloatChoice {
            min_value: -10.0,
            max_value: -1.0,
            allow_nan: false,
            allow_infinity: true,
        }
        .simplest(),
        -1.0
    );
}

#[test]
fn float_choice_simplest_spans_zero() {
    assert_eq!(
        FloatChoice {
            min_value: -1.0,
            max_value: 1.0,
            allow_nan: false,
            allow_infinity: true,
        }
        .simplest(),
        0.0
    );
}

#[test]
fn float_choice_simplest_with_inf_bounds() {
    let fc = FloatChoice {
        min_value: f64::NEG_INFINITY,
        max_value: f64::INFINITY,
        allow_nan: false,
        allow_infinity: false,
    };
    assert_eq!(fc.simplest(), 0.0);
    let fc2 = FloatChoice {
        min_value: 1.0,
        max_value: f64::INFINITY,
        allow_nan: false,
        allow_infinity: false,
    };
    assert_eq!(fc2.simplest(), 1.0);
    let fc3 = FloatChoice {
        min_value: f64::NEG_INFINITY,
        max_value: -1.0,
        allow_nan: false,
        allow_infinity: false,
    };
    assert_eq!(fc3.simplest(), -1.0);
}

#[test]
fn float_choice_simplest_tiny_range() {
    // Tiny range where no power of 2 is in range.
    let fc = FloatChoice {
        min_value: 1.5,
        max_value: 1.75,
        allow_nan: false,
        allow_infinity: false,
    };
    assert_eq!(fc.simplest(), 1.5);
}

#[test]
fn float_choice_simplest_subnormal_range() {
    // Subnormal-only range — simplest must be a valid subnormal in the range.
    // The Rust ordering uses update_mantissa's bit-reversal, so the "simpler"
    // subnormal in this tiny range is 2e-323 rather than the lower boundary.
    let fc = FloatChoice {
        min_value: 1e-323,
        max_value: 2e-323,
        allow_nan: false,
        allow_infinity: false,
    };
    let s = fc.simplest();
    assert!(fc.validate(s));
    assert!(s == 1e-323 || s == 2e-323);
}

#[test]
fn float_choice_simplest_finds_power_of_two() {
    // Range [0.5, 2.0] — simplest is 1.0, found by the integer search.
    let fc = FloatChoice {
        min_value: 0.5,
        max_value: 2.0,
        allow_nan: false,
        allow_infinity: false,
    };
    assert_eq!(fc.simplest(), 1.0);
}

#[test]
fn float_choice_simplest_negative_zero_range() {
    // Range containing 0.0 yields 0.0 directly.
    let fc = FloatChoice {
        min_value: -1.0,
        max_value: 0.0,
        allow_nan: false,
        allow_infinity: false,
    };
    assert_eq!(fc.simplest(), 0.0);
}

// ── FloatChoice::validate ───────────────────────────────────────────────────
//
// Port of pbtkit/tests/test_floats.py::test_floats_validate_edge_cases.

#[test]
fn float_choice_validate_edge_cases() {
    let kind = FloatChoice {
        min_value: f64::NEG_INFINITY,
        max_value: f64::INFINITY,
        allow_nan: true,
        allow_infinity: true,
    };
    assert!(kind.validate(f64::NAN));
    assert!(kind.validate(f64::INFINITY));
    assert!(kind.validate(f64::NEG_INFINITY));
    assert!(kind.validate(0.0));

    let no_nan = FloatChoice {
        min_value: f64::NEG_INFINITY,
        max_value: f64::INFINITY,
        allow_nan: false,
        allow_infinity: true,
    };
    assert!(!no_nan.validate(f64::NAN));

    let no_inf = FloatChoice {
        min_value: f64::NEG_INFINITY,
        max_value: f64::INFINITY,
        allow_nan: true,
        allow_infinity: false,
    };
    assert!(!no_inf.validate(f64::INFINITY));
    assert!(!no_inf.validate(f64::NEG_INFINITY));

    let bounded = FloatChoice {
        min_value: 0.0,
        max_value: 1.0,
        allow_nan: false,
        allow_infinity: false,
    };
    assert!(!bounded.validate(2.0));
    assert!(bounded.validate(0.5));
}

// ── FloatChoice::sort_index ─────────────────────────────────────────────────
//
// Port of pbtkit/tests/test_floats.py::test_floats_sort_key_ordering. Rust's
// FloatChoice::sort_index returns `(magnitude_index, is_negative)`, which
// orders values as: smallest non-negative finite < larger non-negative finite
// < +inf < -inf < NaN. Simpler positive finites sort before more complex ones.

#[test]
fn float_choice_sort_index_ordering() {
    let kind = FloatChoice {
        min_value: f64::NEG_INFINITY,
        max_value: f64::INFINITY,
        allow_nan: true,
        allow_infinity: true,
    };
    // Finite < inf < -inf < NaN
    assert!(kind.sort_index(0.0) < kind.sort_index(f64::INFINITY));
    assert!(kind.sort_index(f64::INFINITY) < kind.sort_index(f64::NEG_INFINITY));
    assert!(kind.sort_index(f64::NEG_INFINITY) < kind.sort_index(f64::NAN));
    // Simpler finite values sort earlier.
    assert!(kind.sort_index(1.0) < kind.sort_index(2.0));
    assert!(kind.sort_index(1.0) < kind.sort_index(1.5));
    assert!(kind.sort_index(1.0) < kind.sort_index(-1.0));
}

// ── FloatChoice::to_index regression ────────────────────────────────────────
//
// Regression for a failure surfaced by `tests/pbtkit/choice_index.rs`: a tiny
// non-integer-spanning range like [65672.5, 65673.0] picks `simplest = 65673.0`
// (the integer wins under native's Hypothesis-lex sort_key), but the original
// pbtkit-style raw-index implementation computed `to_index(value)` as
// `raw_idx(value) - raw_idx(simplest)` — which underflowed because in raw-idx
// terms 65672.5 < 65672.80222519021 < 65673.0. The fix is to base the index
// API on the same `sort_key` ordering used elsewhere in native.

#[test]
fn float_choice_to_index_does_not_underflow_when_simplest_is_above_value() {
    let fc = FloatChoice {
        min_value: 65672.5,
        max_value: 65673.0,
        allow_nan: false,
        allow_infinity: false,
    };
    // Must not panic. The exact value is unimportant beyond being in range.
    let _ = fc.to_index(65672.80222519021);
    let _ = fc.to_index(65672.5);
    let _ = fc.to_index(65673.0);
}

#[test]
fn float_choice_index_roundtrip_tiny_range() {
    let fc = FloatChoice {
        min_value: 65672.5,
        max_value: 65673.0,
        allow_nan: false,
        allow_infinity: false,
    };
    for &v in &[65672.5_f64, 65672.80222519021, 65673.0] {
        let idx = fc.to_index(v);
        assert_eq!(fc.from_index(idx).map(f64::to_bits), Some(v.to_bits()));
    }
}

#[test]
fn float_choice_from_index_zero_is_simplest_tiny_range() {
    let fc = FloatChoice {
        min_value: 65672.5,
        max_value: 65673.0,
        allow_nan: false,
        allow_infinity: false,
    };
    assert_eq!(
        fc.from_index(BigUint::from(0u32)).map(f64::to_bits),
        Some(fc.simplest().to_bits())
    );
}

// Regression: `simplest` for a fractional-only range like [0.25, 0.5] is
// `0.5`, whose Hypothesis lex index ((1<<63) | (1024<<52)) exceeds
// `float_to_index(f64::MAX)` ((1<<63) | (1023<<52) | mantissa_max). The old
// `max_finite_global_rank` used the latter as the bound, so `from_index(0)`
// landed in the +inf slot above max_finite and returned None.
#[test]
fn float_choice_from_index_zero_is_simplest_fractional_range() {
    let fc = FloatChoice {
        min_value: 0.25,
        max_value: 0.5,
        allow_nan: false,
        allow_infinity: false,
    };
    assert_eq!(
        fc.from_index(BigUint::from(0u32)).map(f64::to_bits),
        Some(fc.simplest().to_bits())
    );
}

#[test]
fn float_choice_index_roundtrip_fractional_value() {
    let fc = FloatChoice {
        min_value: 0.0,
        max_value: 1.0,
        allow_nan: false,
        allow_infinity: false,
    };
    let v = 0.5_f64;
    let idx = fc.to_index(v);
    assert_eq!(fc.from_index(idx).map(f64::to_bits), Some(v.to_bits()));
}

// ── FloatChoice::unit ───────────────────────────────────────────────────────
//
// Port of pbtkit/tests/test_floats.py::test_float_choice_unit, adapted to the
// Rust implementation's (index, is_negative) ordering (the Python version
// uses (exponent_rank, mantissa, sign)).

#[test]
fn float_choice_unit_spans_zero() {
    // Rust ordering: simplest is 0.0 (index 0); offset 1 maps to 1.0.
    let fc = FloatChoice {
        min_value: -10.0,
        max_value: 10.0,
        allow_nan: false,
        allow_infinity: false,
    };
    assert_eq!(fc.unit(), 1.0);
}

#[test]
fn float_choice_unit_single_value_range() {
    // Single-value range — unit falls back to simplest.
    let fc = FloatChoice {
        min_value: 5.0,
        max_value: 5.0,
        allow_nan: false,
        allow_infinity: false,
    };
    assert_eq!(fc.unit(), 5.0);
}

#[test]
fn float_choice_unit_negative_range() {
    // Rust ordering: simplest is -5.0 (index 5, is_neg=true); offset 1 maps
    // to index_to_float(6) = 6.0, negated is -6.0 which is valid in [-10, -5].
    let fc = FloatChoice {
        min_value: -10.0,
        max_value: -5.0,
        allow_nan: false,
        allow_infinity: false,
    };
    assert_eq!(fc.simplest(), -5.0);
    assert_eq!(fc.unit(), -6.0);
}

// ── BytesChoice ───────────────────────────────────────────────────────────
//
// Ports of pbtkit/tests/test_bytes.py::test_bytes_choice_unit and related.

#[test]
fn bytes_choice_simplest_empty_min() {
    let bc = BytesChoice {
        min_size: 0,
        max_size: 10,
    };
    assert_eq!(bc.simplest(), Vec::<u8>::new());
}

#[test]
fn bytes_choice_simplest_nonempty_min() {
    let bc = BytesChoice {
        min_size: 3,
        max_size: 10,
    };
    assert_eq!(bc.simplest(), vec![0u8; 3]);
}

#[test]
fn bytes_choice_unit_empty_min_positive_max() {
    let bc = BytesChoice {
        min_size: 0,
        max_size: 10,
    };
    assert_eq!(bc.unit(), vec![1u8]);
}

#[test]
fn bytes_choice_unit_empty_min_zero_max() {
    let bc = BytesChoice {
        min_size: 0,
        max_size: 0,
    };
    assert_eq!(bc.unit(), Vec::<u8>::new());
}

#[test]
fn bytes_choice_unit_nonempty_min() {
    let bc = BytesChoice {
        min_size: 3,
        max_size: 10,
    };
    // simplest except last byte is 1
    assert_eq!(bc.unit(), vec![0u8, 0u8, 1u8]);
}

#[test]
fn bytes_choice_validate() {
    let bc = BytesChoice {
        min_size: 2,
        max_size: 4,
    };
    assert!(bc.validate(&[0, 0]));
    assert!(bc.validate(&[0xff, 0xff, 0xff, 0xff]));
    assert!(!bc.validate(&[]));
    assert!(!bc.validate(&[0]));
    assert!(!bc.validate(&[0u8; 5]));
}

#[test]
fn bytes_choice_sort_key_shortlex() {
    let bc = BytesChoice {
        min_size: 0,
        max_size: 10,
    };
    // Shorter sorts before longer.
    assert!(bc.sort_key(&[]) < bc.sort_key(&[0]));
    // At equal length, lexicographic order.
    assert!(bc.sort_key(&[0, 0]) < bc.sort_key(&[0, 1]));
    assert!(bc.sort_key(&[0, 0xff]) < bc.sort_key(&[1, 0]));
}

// ── StringChoice ──────────────────────────────────────────────────────────
//
// Ports of pbtkit/tests/test_text.py::test_string_* and related. Note that
// `StringChoice::simplest`/`unit` return codepoint sequences (`Vec<u32>`) —
// the `String` boundary lives one level up in `NativeTestCase::draw_string`.

/// Helper: turn a short ASCII `&str` literal into a `Vec<u32>` for use in
/// assertions against codepoint-sequence results.
fn cps(s: &str) -> Vec<u32> {
    s.chars().map(|c| c as u32).collect()
}

#[test]
fn string_choice_simplest_ascii_range() {
    let sc = StringChoice {
        min_codepoint: 0x20,
        max_codepoint: 0x7e,
        min_size: 3,
        max_size: 10,
    };
    // Under codepoint_key ordering, '0' (48) is the simplest codepoint.
    assert_eq!(sc.simplest(), cps("000"));
}

#[test]
fn string_choice_simplest_no_ascii_overlap() {
    let sc = StringChoice {
        min_codepoint: 0x2000,
        max_codepoint: 0x200f,
        min_size: 2,
        max_size: 5,
    };
    // No ASCII overlap: simplest codepoint is min_codepoint.
    assert_eq!(sc.simplest(), vec![0x2000, 0x2000]);
}

#[test]
fn string_choice_simplest_partial_ascii_overlap() {
    // Range [50..200]: the ASCII portion is [50..127], digit '0' (48) is NOT
    // in range, so the simplest is whichever of [50..127] has the smallest
    // codepoint_key. Keys for 50..127 are {(50-48)%128, (51-48)%128, ...} =
    // {2, 3, 4, ..., 79}. So codepoint 50 ('2') has the smallest key.
    let sc = StringChoice {
        min_codepoint: 50,
        max_codepoint: 200,
        min_size: 1,
        max_size: 5,
    };
    assert_eq!(sc.simplest(), cps("2"));
}

#[test]
fn string_choice_simplest_empty_min() {
    let sc = StringChoice {
        min_codepoint: 0x20,
        max_codepoint: 0x7e,
        min_size: 0,
        max_size: 10,
    };
    assert_eq!(sc.simplest(), Vec::<u32>::new());
}

#[test]
fn string_choice_unit_positive_min_size() {
    // With ASCII range and min_size=3, simplest is "000" and unit is "001".
    let sc = StringChoice {
        min_codepoint: 0x20,
        max_codepoint: 0x7e,
        min_size: 3,
        max_size: 10,
    };
    assert_eq!(sc.unit(), cps("001"));
}

#[test]
fn string_choice_unit_zero_min_size() {
    // With ASCII range and min_size=0, unit is "1" (second-simplest codepoint,
    // single char since min_size was 0 and max_size >= 1).
    let sc = StringChoice {
        min_codepoint: 0x20,
        max_codepoint: 0x7e,
        min_size: 0,
        max_size: 10,
    };
    assert_eq!(sc.unit(), cps("1"));
}

#[test]
fn string_choice_validate() {
    let sc = StringChoice {
        min_codepoint: b'a' as u32,
        max_codepoint: b'z' as u32,
        min_size: 2,
        max_size: 4,
    };
    assert!(sc.validate(&cps("aa")));
    assert!(sc.validate(&cps("zzzz")));
    assert!(!sc.validate(&cps("")));
    assert!(!sc.validate(&cps("a")));
    assert!(!sc.validate(&cps("aaaaa")));
    assert!(!sc.validate(&cps("aA"))); // 'A' out of range
}

#[test]
fn string_choice_validate_rejects_surrogates() {
    // The engine's codepoint model can represent surrogates as raw `u32`s,
    // so `validate` has to reject them explicitly rather than rely on the
    // `char` type's scalar-value guarantee.
    let sc = StringChoice {
        min_codepoint: 0xD000,
        max_codepoint: 0xE000,
        min_size: 0,
        max_size: 4,
    };
    // Empty value is valid.
    assert!(sc.validate(&[]));
    // A non-surrogate codepoint in range passes.
    assert!(sc.validate(&[0xD000]));
    // An in-range surrogate is rejected.
    assert!(!sc.validate(&[0xD800]));
}

#[test]
fn string_choice_sort_key_shortlex_on_codepoint_keys() {
    let sc = StringChoice {
        min_codepoint: 0x20,
        max_codepoint: 0x7e,
        min_size: 0,
        max_size: 10,
    };
    // Shorter is always simpler.
    assert!(sc.sort_key(&cps("")) < sc.sort_key(&cps("0")));
    // '0' has key 0, '1' has key 1, so "0" < "1".
    assert!(sc.sort_key(&cps("0")) < sc.sort_key(&cps("1")));
    // '0' < 'A' (key 17, since (65-48)%128 = 17).
    assert!(sc.sort_key(&cps("0")) < sc.sort_key(&cps("A")));
    // Digits simpler than space (key (32-48)%128 = 112).
    assert!(sc.sort_key(&cps("0")) < sc.sort_key(&cps(" ")));
}

// ── StringChoice::unit (single-codepoint alphabet) ────────────────────────
//
// Ports of pbtkit/tests/test_text.py::test_string_single_codepoint_unit.

#[test]
fn string_choice_single_codepoint_unit_variable_length() {
    // Single codepoint '0', variable length: unit lengthens by one codepoint.
    let kind = StringChoice {
        min_codepoint: 48,
        max_codepoint: 48,
        min_size: 0,
        max_size: 5,
    };
    assert_eq!(kind.unit(), cps("0"));
    assert_eq!(kind.simplest(), Vec::<u32>::new());
}

#[test]
fn string_choice_single_codepoint_unit_fixed_length() {
    // Single codepoint '0', fixed length: unit degenerates to simplest.
    let kind = StringChoice {
        min_codepoint: 48,
        max_codepoint: 48,
        min_size: 2,
        max_size: 2,
    };
    assert_eq!(kind.unit(), kind.simplest());
}

#[test]
fn string_choice_single_codepoint_unit_non_zero() {
    // Single codepoint 'A' — the second-simplest candidate ('1') falls outside
    // the range, so the general unit path falls back to the simplest codepoint.
    let kind = StringChoice {
        min_codepoint: 65,
        max_codepoint: 65,
        min_size: 0,
        max_size: 5,
    };
    assert_eq!(kind.unit(), cps("A"));
}

// ── StringChoice index helpers ────────────────────────────────────────────
//
// Ports of pbtkit/tests/test_text.py::test_string_from_index_out_of_range,
// test_string_from_index_past_end, test_string_codepoint_rank_with_surrogates.

#[test]
fn string_choice_from_index_out_of_range() {
    let sc = StringChoice {
        min_codepoint: 32,
        max_codepoint: 126,
        min_size: 0,
        max_size: 2,
    };
    assert!(
        sc.from_index(sc.max_index() + BigUint::from(1u32))
            .is_none()
    );
}

#[test]
fn string_choice_from_index_past_end() {
    // alpha_size = 95; max_index = 95^0 + 95^1 + 95^2 - 1 = 9120;
    // index 9121 exhausts all length buckets.
    let sc = StringChoice {
        min_codepoint: 32,
        max_codepoint: 126,
        min_size: 0,
        max_size: 2,
    };
    assert_eq!(sc.alpha_size(), 95);
    assert_eq!(sc.max_index(), BigUint::from(9120u32));
    assert!(sc.from_index(BigUint::from(9121u32)).is_none());
}

#[test]
fn string_choice_codepoint_rank_with_surrogates() {
    // Range spanning the surrogate block (0xD800..=0xDFFF).
    let sc = StringChoice {
        min_codepoint: 0xD700,
        max_codepoint: 0xE000,
        min_size: 0,
        max_size: 1,
    };
    let rank = sc.codepoint_rank(0xE000);
    // 0xE000 is at the top of the (surrogate-filtered) range.
    let expected: u64 = (0xE000u32 - 0xD700u32) as u64 - (0xDFFFu32 - 0xD800u32 + 1) as u64;
    assert_eq!(rank, expected);
    // Round-trip through to_index/from_index.
    let v = vec![0xE000u32];
    let idx = sc.to_index(&v);
    assert_eq!(sc.from_index(idx).unwrap(), v);
}

#[test]
fn string_choice_to_index_from_index_roundtrip_ascii() {
    let sc = StringChoice {
        min_codepoint: 32,
        max_codepoint: 126,
        min_size: 0,
        max_size: 3,
    };
    for s in ["", "0", "1", "A", "00", "abc", "z!@"] {
        let v = cps(s);
        let idx = sc.to_index(&v);
        assert_eq!(sc.from_index(idx), Some(v), "round-trip failed for {s:?}");
    }
}

#[test]
fn string_choice_max_index_exceeds_u128() {
    // Regression: alpha ≈ 1,112,064 (all of Unicode minus surrogates) and
    // max_size = 16 yields max_index ≈ 10^97, which vastly overflows u128
    // (~3.4·10^38). The bignum port must return this without panicking,
    // and a round-trip at the top of the final length bucket must preserve
    // the value.
    let sc = StringChoice {
        min_codepoint: 0,
        max_codepoint: 0x10FFFF,
        min_size: 0,
        max_size: 16,
    };
    let idx = sc.max_index();
    assert!(idx > BigUint::from(u128::MAX));

    // A value of length max_size should sit near the top of the bucket
    // (strictly less than max_index, but beyond anything u128 could hold).
    let v = vec![0x10FFFDu32; 16];
    let v_idx = sc.to_index(&v);
    assert!(v_idx > BigUint::from(u128::MAX));
    assert!(v_idx <= idx);
    assert_eq!(sc.from_index(v_idx), Some(v));
}

#[test]
fn string_choice_alpha_size_no_surrogate_overlap() {
    let sc = StringChoice {
        min_codepoint: 32,
        max_codepoint: 126,
        min_size: 0,
        max_size: 2,
    };
    assert_eq!(sc.alpha_size(), 95);
}

#[test]
fn string_choice_alpha_size_skips_surrogates() {
    let sc = StringChoice {
        min_codepoint: 0,
        max_codepoint: 0xFFFF,
        min_size: 0,
        max_size: 1,
    };
    assert_eq!(sc.alpha_size(), 0x10000 - 0x800);
}

#[test]
#[should_panic(expected = "ChoiceKind::to_index: kind/value mismatch")]
fn choice_kind_to_index_panics_on_kind_value_mismatch() {
    // Asking an Integer kind to index a Boolean value is a programmer error;
    // ChoiceKind::to_index must panic loudly rather than return a bogus index.
    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: 0,
        max_value: 100,
    });
    let _ = kind.to_index(&ChoiceValue::Boolean(true));
}

use super::*;
use crate::native::bignum::BigInt;

// ── IntegerChoice::simplest ─────────────────────────────────────────────────
//
// Ports of pbtkit/tests/test_core.py::test_integer_choice_simplest.

#[test]
fn integer_choice_simplest_spans_zero() {
    assert_eq!(
        IntegerChoice {
            min_value: BigInt::from(-10),
            max_value: BigInt::from(10),
            shrink_towards: BigInt::from(0),
        }
        .simplest(),
        0
    );
}

#[test]
fn integer_choice_simplest_all_positive() {
    assert_eq!(
        IntegerChoice {
            min_value: BigInt::from(5),
            max_value: BigInt::from(100),
            shrink_towards: BigInt::from(0),
        }
        .simplest(),
        5
    );
}

#[test]
fn integer_choice_simplest_all_negative() {
    assert_eq!(
        IntegerChoice {
            min_value: BigInt::from(-100),
            max_value: BigInt::from(-5),
            shrink_towards: BigInt::from(0),
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
            min_value: BigInt::from(-10),
            max_value: BigInt::from(10),
            shrink_towards: BigInt::from(0),
        }
        .unit(),
        1
    );
}

#[test]
fn integer_choice_unit_all_positive() {
    assert_eq!(
        IntegerChoice {
            min_value: BigInt::from(5),
            max_value: BigInt::from(100),
            shrink_towards: BigInt::from(0),
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
            min_value: BigInt::from(-100),
            max_value: BigInt::from(-5),
            shrink_towards: BigInt::from(0),
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
            min_value: BigInt::from(5),
            max_value: BigInt::from(5),
            shrink_towards: BigInt::from(0),
        }
        .unit(),
        5
    );
}

#[test]
#[should_panic(expected = "ChoiceKind::to_index: kind/value mismatch")]
fn choice_kind_to_index_panics_on_kind_value_mismatch() {
    // Asking an Integer kind to index a Boolean value is a programmer error;
    // ChoiceKind::to_index must panic loudly rather than return a bogus index.
    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: BigInt::from(0),
        max_value: BigInt::from(100),
        shrink_towards: BigInt::from(0),
    });
    let _ = kind.to_index(&ChoiceValue::Boolean(true));
}

// ── ChoiceKind::max_children ──────────────────────────────────────────────
//
// Ports of `compute_max_children` tests from
// `hypothesis-python/tests/conjecture/test_utils.py` plus hegel-specific
// checks for the choice kinds hegel's native engine actually records.

fn bu(n: u64) -> crate::native::bignum::BigInt {
    crate::native::bignum::BigInt::from(n)
}

#[test]
fn integer_bounded_range_gives_exact_count() {
    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: BigInt::from(0),
        max_value: BigInt::from(200),
        shrink_towards: BigInt::from(0),
    });
    assert_eq!(kind.max_children(), bu(201));
}

#[test]
fn integer_negative_range_gives_exact_count() {
    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: BigInt::from(-10),
        max_value: BigInt::from(10),
        shrink_towards: BigInt::from(0),
    });
    assert_eq!(kind.max_children(), bu(21));
}

#[test]
fn integer_full_i128_range_is_two_pow_128() {
    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: BigInt::from(i128::MIN),
        max_value: BigInt::from(i128::MAX),
        shrink_towards: BigInt::from(0),
    });
    // 2^128 = u128::MAX + 1.
    let expected = crate::native::bignum::BigInt::from(u128::MAX) + bu(1);
    assert_eq!(kind.max_children(), expected);
}

// ── IntegerChoice::to_index / from_index round-trips ────────────────────────
//
// `to_index` and `from_index` are inverses over the value range. The shrinker
// uses both heavily, so the round-trip property anchors any future
// optimisation of the binary-search implementation.

fn integer_choice(min: i128, max: i128) -> IntegerChoice {
    IntegerChoice {
        min_value: BigInt::from(min),
        max_value: BigInt::from(max),
        shrink_towards: BigInt::from(0),
    }
}

#[test]
fn integer_choice_index_round_trip_symmetric_around_zero() {
    let ic = integer_choice(-10, 10);
    for v in -10i128..=10 {
        let idx = ic.to_index(v);
        assert_eq!(
            ic.from_index(idx),
            Some(BigInt::from(v)),
            "round-trip failed for v={v}"
        );
    }
}

#[test]
fn integer_choice_index_round_trip_all_positive() {
    let ic = integer_choice(5, 25);
    for v in 5i128..=25 {
        let idx = ic.to_index(v);
        assert_eq!(
            ic.from_index(idx),
            Some(BigInt::from(v)),
            "round-trip failed for v={v}"
        );
    }
}

#[test]
fn integer_choice_index_round_trip_all_negative() {
    let ic = integer_choice(-25, -5);
    for v in -25i128..=-5 {
        let idx = ic.to_index(v);
        assert_eq!(
            ic.from_index(idx),
            Some(BigInt::from(v)),
            "round-trip failed for v={v}"
        );
    }
}

#[test]
fn integer_choice_index_round_trip_asymmetric() {
    // shrink_towards = 0 sits 5 above the floor and 100 below the ceiling.
    let ic = integer_choice(-5, 100);
    for v in -5i128..=100 {
        let idx = ic.to_index(v);
        assert_eq!(
            ic.from_index(idx),
            Some(BigInt::from(v)),
            "round-trip failed for v={v}"
        );
    }
}

#[test]
fn integer_choice_index_round_trip_full_i128_range() {
    // Boundary cases for the full i128 range: above + below = u128::MAX, so
    // any u128-native binary search must handle exactly the largest valid d.
    let ic = integer_choice(i128::MIN, i128::MAX);
    for v in [
        0i128,
        1,
        -1,
        i128::MIN,
        i128::MAX,
        i128::MIN + 1,
        i128::MAX - 1,
        1 << 100,
        -(1 << 100),
    ] {
        let idx = ic.to_index(v);
        assert_eq!(
            ic.from_index(idx),
            Some(BigInt::from(v)),
            "round-trip failed for v={v}"
        );
    }
}

#[test]
fn integer_choice_index_round_trip_single_value() {
    let ic = integer_choice(42, 42);
    let idx = ic.to_index(42);
    assert_eq!(idx, crate::native::bignum::BigInt::from(0u32));
    assert_eq!(ic.from_index(idx), Some(BigInt::from(42)));
}

#[test]
fn integer_choice_from_index_past_max_returns_none() {
    let ic = integer_choice(0, 5);
    // Range has 6 values (indices 0..=5); index 100 is past max.
    let big = crate::native::bignum::BigInt::from(100u32);
    assert_eq!(ic.from_index(big), None);
}

#[test]
fn integer_choice_from_index_overflowing_u128_returns_none() {
    // Even on the full i128 range, max_index is `u128::MAX` — any index
    // strictly larger than that has no valid value. The u128-native
    // implementation short-circuits via the `u128::try_from` step.
    let ic = integer_choice(i128::MIN, i128::MAX);
    let too_big =
        crate::native::bignum::BigInt::from(u128::MAX) + crate::native::bignum::BigInt::from(1u32);
    assert_eq!(ic.from_index(too_big), None);
}

#[test]
fn boolean_is_always_two() {
    assert_eq!((ChoiceKind::Boolean(BooleanChoice)).max_children(), bu(2));
}

// ── FloatChoice ──────────────────────────────────────────────────────────

fn fc(min: f64, max: f64, allow_nan: bool, allow_infinity: bool) -> FloatChoice {
    FloatChoice {
        min_value: min,
        max_value: max,
        allow_nan,
        allow_infinity,
    }
}

#[test]
fn float_choice_simplest_picks_zero_when_in_range() {
    assert_eq!(fc(-1.0, 1.0, false, false).simplest(), 0.0);
    assert_eq!(fc(0.0, 10.0, false, false).simplest(), 0.0);
}

#[test]
fn float_choice_simplest_picks_closest_endpoint_when_zero_excluded() {
    // [1.5, 10.0]: simplest is the nearest integer above 1.5, which is 2.0.
    assert_eq!(fc(1.5, 10.0, false, false).simplest(), 2.0);
    // [-5.0, -1.5]: simplest is the nearest integer below -1.5, which is -2.0.
    assert_eq!(fc(-5.0, -1.5, false, false).simplest(), -2.0);
}

#[test]
fn float_choice_simplest_finds_simple_fraction_in_tight_range() {
    // [1.4, 1.6] contains no integer, so the search falls through to the
    // exponent/mantissa loop that scans "simple fractions". `1.5` has the
    // smallest lex index in that loop, becomes the running best, and the
    // next mantissa probe trips the inner-loop early break.
    assert_eq!(fc(1.4, 1.6, false, false).simplest(), 1.5);
}

#[test]
fn float_choice_simplest_falls_back_to_infinity_when_no_finite_values() {
    // Empty finite range, but +inf allowed.
    let fc = fc(f64::INFINITY, f64::INFINITY, false, true);
    assert_eq!(fc.simplest(), f64::INFINITY);
    let fc = fc_neg();
    assert_eq!(fc.simplest(), f64::NEG_INFINITY);
}

fn fc_neg() -> FloatChoice {
    fc(f64::NEG_INFINITY, f64::NEG_INFINITY, false, true)
}

#[test]
fn float_choice_simplest_falls_back_to_nan_when_only_nan_allowed() {
    // Empty finite range, no infinities, but NaN allowed.
    let fc = FloatChoice {
        min_value: f64::INFINITY,
        max_value: f64::NEG_INFINITY,
        allow_nan: true,
        allow_infinity: false,
    };
    assert!(fc.simplest().is_nan());
}

#[test]
#[should_panic(expected = "FloatChoice::simplest: no valid float")]
fn float_choice_simplest_panics_when_nothing_valid() {
    let fc = FloatChoice {
        min_value: f64::INFINITY,
        max_value: f64::NEG_INFINITY,
        allow_nan: false,
        allow_infinity: false,
    };
    let _ = fc.simplest();
}

#[test]
fn float_choice_unit_falls_through_to_simplest_on_nan_start() {
    // When simplest() returns NaN, unit() short-circuits to that NaN.
    let fc = FloatChoice {
        min_value: f64::INFINITY,
        max_value: f64::NEG_INFINITY,
        allow_nan: true,
        allow_infinity: false,
    };
    assert!(fc.unit().is_nan());
}

#[test]
fn float_choice_enumerate_returns_none() {
    // The float space is too large to enumerate under any reasonable cap.
    let kind = ChoiceKind::Float(fc(0.0, 1.0, false, false));
    assert!(kind.enumerate(u64::MAX).is_none());
}

#[test]
fn float_choice_to_from_index_round_trip() {
    let kind = ChoiceKind::Float(fc(-10.0, 10.0, false, false));
    for v in [0.0_f64, 1.0, 2.0, -1.0, -2.0, 0.5, -0.5, 4.25] {
        let idx = kind.to_index(&ChoiceValue::Float(v));
        let back = kind.from_index(idx).unwrap();
        assert_eq!(back, ChoiceValue::Float(v));
    }
}

#[test]
fn float_choice_to_from_index_round_trip_for_infinity_and_nan() {
    let fc = FloatChoice {
        min_value: f64::NEG_INFINITY,
        max_value: f64::INFINITY,
        allow_nan: true,
        allow_infinity: true,
    };
    for v in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        let idx = fc.to_index(v);
        let back = fc.from_index(idx).expect("rank is valid");
        if v.is_nan() {
            assert!(back.is_nan());
        } else {
            assert_eq!(back, v);
        }
    }
}

// ── BytesChoice ────────────────────────────────────────────────────────

#[test]
fn bytes_choice_simplest_is_min_size_zeros() {
    let bc = BytesChoice {
        min_size: 3,
        max_size: 10,
    };
    assert_eq!(bc.simplest(), vec![0, 0, 0]);
}

#[test]
fn bytes_choice_unit_with_zero_min_zero_max_falls_back_to_simplest() {
    // min == 0 && max == 0: the unit() helper has no representable
    // "second-simplest" — it returns the simplest (empty) by definition.
    let bc = BytesChoice {
        min_size: 0,
        max_size: 0,
    };
    assert_eq!(bc.unit(), Vec::<u8>::new());
}

#[test]
fn bytes_choice_unit_with_zero_min_nonzero_max_is_single_one() {
    let bc = BytesChoice {
        min_size: 0,
        max_size: 5,
    };
    assert_eq!(bc.unit(), vec![1u8]);
}

#[test]
fn bytes_choice_unit_with_nonzero_min_is_zeros_with_trailing_one() {
    let bc = BytesChoice {
        min_size: 3,
        max_size: 10,
    };
    assert_eq!(bc.unit(), vec![0, 0, 1]);
}

#[test]
fn bytes_choice_index_round_trip_across_lengths() {
    let bc = BytesChoice {
        min_size: 0,
        max_size: 3,
    };
    for v in [
        Vec::<u8>::new(),
        vec![0u8],
        vec![1u8],
        vec![0xffu8],
        vec![0u8, 0u8],
        vec![0u8, 1u8],
        vec![1u8, 2u8, 3u8],
        vec![0xff, 0xff, 0xff],
    ] {
        let idx = bc.to_index(&v);
        assert_eq!(bc.from_index(idx), Some(v));
    }
}

#[test]
fn bytes_choice_from_index_past_max_returns_none() {
    // Sequences of length 0..=1 over 256 bytes give 1 + 256 = 257 options.
    let bc = BytesChoice {
        min_size: 0,
        max_size: 1,
    };
    assert!(
        bc.from_index(crate::native::bignum::BigInt::from(1000u32))
            .is_none()
    );
}

#[test]
fn bytes_choice_kind_enumerate_zero_max_size_returns_single_empty() {
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 0,
    });
    assert_eq!(
        kind.enumerate(u64::MAX),
        Some(vec![ChoiceValue::Bytes(Vec::new())])
    );
}

#[test]
fn bytes_choice_kind_enumerate_positive_max_returns_none() {
    // Once max_size > 0, the total cardinality (`Σ 256^k`) exceeds the cap.
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 4,
    });
    assert!(kind.enumerate(u64::MAX).is_none());
}

// ── StringChoice ──────────────────────────────────────────────────────

fn string_choice(intervals: Vec<(u32, u32)>, min_size: usize, max_size: usize) -> StringChoice {
    StringChoice {
        intervals: crate::native::intervalsets::IntervalSet::new(intervals),
        min_size,
        max_size,
    }
}

#[test]
fn string_choice_simplest_is_first_shrink_order_position() {
    // Alphabet `[a-z]`: shrink order is alphabet-relative, so position 0 is
    // 'a' (the smallest codepoint in this alphabet — no '0' or other digits
    // available to override).
    let sc = string_choice(vec![(b'a' as u32, b'z' as u32)], 0, 1);
    assert_eq!(sc.simplest_codepoint(), b'a' as u32);
}

#[test]
fn string_choice_simplest_prefers_zero_when_alphabet_contains_digits() {
    // Full alphabet: shrink order starts with '0'.
    let sc = string_choice(vec![(0, 0xD7FF), (0xE000, 0x10FFFF)], 0, 1);
    assert_eq!(sc.simplest_codepoint(), b'0' as u32);
}

#[test]
fn string_choice_unit_single_codepoint_alphabet_at_max_size_falls_back_to_simplest() {
    // Alphabet of one codepoint ('A') at fixed length: `unit()` has no
    // "second-simplest" to swap in and no room to lengthen, so it falls back
    // to `simplest()`.
    let sc = string_choice(vec![(0x41, 0x41)], 2, 2);
    assert_eq!(sc.unit(), vec![0x41, 0x41]);
}

#[test]
fn string_choice_unit_empty_fixed_length_falls_back_to_simplest() {
    // min_size == max_size == 0: `unit()` has no slot to insert the
    // "second-simplest" codepoint into.
    let sc = string_choice(vec![(0, 100)], 0, 0);
    assert_eq!(sc.unit(), Vec::<u32>::new());
}

#[test]
fn string_choice_kind_enumerate_zero_max_size_returns_single_empty() {
    let kind = ChoiceKind::String(string_choice(vec![(b'a' as u32, b'z' as u32)], 0, 0));
    assert_eq!(
        kind.enumerate(u64::MAX),
        Some(vec![ChoiceValue::String(Vec::new())])
    );
}

#[test]
fn string_choice_kind_enumerate_positive_max_returns_none() {
    let kind = ChoiceKind::String(string_choice(vec![(b'a' as u32, b'z' as u32)], 0, 4));
    assert!(kind.enumerate(u64::MAX).is_none());
}

#[test]
fn string_choice_codepoint_key_is_alphabet_relative() {
    // In a `[a-z]` alphabet, 'a' is shrink-order position 0 (no '0' or 'A'
    // available to take precedence). In the full alphabet, '0' is position 0
    // and 'a' falls past the digits + uppercase letters.
    let lower = string_choice(vec![(b'a' as u32, b'z' as u32)], 0, 5);
    assert_eq!(lower.codepoint_key(b'a' as u32), 0);
    assert_eq!(lower.codepoint_key(b'z' as u32), 25);

    let full = string_choice(vec![(0, 0xD7FF), (0xE000, 0x10FFFF)], 0, 5);
    assert_eq!(full.codepoint_key(b'0' as u32), 0);
    assert_eq!(full.codepoint_key(b'Z' as u32), 42);
    // '/' (cp 47) is "below '0'", so it lands just past 'Z' in shrink order.
    assert_eq!(full.codepoint_key(b'/' as u32), 43);
}

#[test]
fn string_choice_index_round_trip_across_lengths() {
    let sc = string_choice(vec![(b'a' as u32, b'c' as u32)], 0, 2);
    for v in [
        Vec::<u32>::new(),
        vec![b'a' as u32],
        vec![b'b' as u32],
        vec![b'c' as u32],
        vec![b'a' as u32, b'a' as u32],
        vec![b'b' as u32, b'c' as u32],
    ] {
        let idx = sc.to_index(&v);
        assert_eq!(sc.from_index(idx), Some(v));
    }
}

#[test]
fn string_choice_from_index_past_max_returns_none() {
    let sc = string_choice(vec![(b'a' as u32, b'b' as u32)], 0, 1);
    // Sequences of length 0 or 1 over a 2-char alphabet: 1 + 2 = 3 options.
    assert!(
        sc.from_index(crate::native::bignum::BigInt::from(1000u32))
            .is_none()
    );
}

// ── NodeSortKeyRef + NodesSortKey ───────────────────────────────────────────
//
// Direct tests for the allocation-free comparison machinery. Most of the
// engine reaches these through `sort_key(...) < sort_key(...)`, but the
// `PartialEq::eq`, `PartialOrd::partial_cmp`, and cross-variant Scalar↔Sequence
// paths only fire in defensive branches.

fn integer_node(min: i128, max: i128, value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(min),
            max_value: BigInt::from(max),
            shrink_towards: BigInt::from(0),
        }),
        value: ChoiceValue::Integer(BigInt::from(value)),
        was_forced: false,
    }
}

fn bytes_node(min: usize, max: usize, value: Vec<u8>) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Bytes(BytesChoice {
            min_size: min,
            max_size: max,
        }),
        value: ChoiceValue::Bytes(value),
        was_forced: false,
    }
}

fn string_node(intervals: Vec<(u32, u32)>, min: usize, max: usize, value: Vec<u32>) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::String(string_choice(intervals, min, max)),
        value: ChoiceValue::String(value),
        was_forced: false,
    }
}

#[test]
fn node_sort_key_ref_scalar_equality_and_partial_cmp() {
    use std::cmp::Ordering;
    let a = integer_node(-10, 10, 3);
    let b = integer_node(-10, 10, 3);
    let c = integer_node(-10, 10, 4);
    // PartialEq::eq path (direct `==`, not through cmp).
    assert!(a.sort_key_ref() == b.sort_key_ref());
    assert!(a.sort_key_ref() != c.sort_key_ref());
    // PartialOrd::partial_cmp path (used by `<`/`>`).
    assert_eq!(
        a.sort_key_ref().partial_cmp(&c.sort_key_ref()),
        Some(Ordering::Less)
    );
}

#[test]
fn node_sort_key_ref_bytes_orders_shortlex() {
    let short = bytes_node(0, 4, vec![0xff, 0xff]);
    let longer = bytes_node(0, 4, vec![0x00, 0x00, 0x00]);
    // Shortlex: shorter wins regardless of element values.
    assert!(short.sort_key_ref() < longer.sort_key_ref());
    let equal_a = bytes_node(0, 4, vec![1, 2, 3]);
    let equal_b = bytes_node(0, 4, vec![1, 2, 3]);
    assert!(equal_a.sort_key_ref() == equal_b.sort_key_ref());
    // Same length: lex on bytes.
    let lex_lo = bytes_node(0, 4, vec![1, 2, 3]);
    let lex_hi = bytes_node(0, 4, vec![1, 2, 4]);
    assert!(lex_lo.sort_key_ref() < lex_hi.sort_key_ref());
}

#[test]
fn node_sort_key_ref_string_orders_by_codepoint_key() {
    // In a `[a-z]` alphabet, codepoint_key('a')=0 < codepoint_key('b')=1.
    let a = string_node(
        vec![(b'a' as u32, b'z' as u32)],
        0,
        4,
        vec![b'a' as u32, b'a' as u32],
    );
    let b = string_node(
        vec![(b'a' as u32, b'z' as u32)],
        0,
        4,
        vec![b'a' as u32, b'b' as u32],
    );
    assert!(a.sort_key_ref() < b.sort_key_ref());
    let a2 = string_node(
        vec![(b'a' as u32, b'z' as u32)],
        0,
        4,
        vec![b'a' as u32, b'a' as u32],
    );
    assert!(a.sort_key_ref() == a2.sort_key_ref());
}

#[test]
fn node_sort_key_ref_cross_variant_scalar_lt_sequence() {
    // Engine guarantees kinds don't change at a given index, but the
    // total ordering on `NodeSortKeyRef` mirrors the derived ordering on
    // `NodeSortKey`: `Scalar < Sequence`.
    let scalar = integer_node(0, 10, 5);
    let bytes_seq = bytes_node(0, 4, vec![0, 0]);
    let str_seq = string_node(vec![(b'a' as u32, b'z' as u32)], 0, 4, vec![b'a' as u32]);
    assert!(scalar.sort_key_ref() < bytes_seq.sort_key_ref());
    assert!(scalar.sort_key_ref() < str_seq.sort_key_ref());
    assert!(bytes_seq.sort_key_ref() > scalar.sort_key_ref());
}

#[test]
fn nodes_sort_key_shortlex_orders_by_length_then_element() {
    use crate::native::core::sort_key;
    let a = vec![integer_node(0, 10, 1)];
    let b = vec![integer_node(0, 10, 1), integer_node(0, 10, 0)];
    assert!(sort_key(&a) < sort_key(&b));
    let same_a = vec![integer_node(0, 10, 2), integer_node(0, 10, 3)];
    let same_b = vec![integer_node(0, 10, 2), integer_node(0, 10, 3)];
    assert!(sort_key(&same_a) == sort_key(&same_b));
    let elem_lo = vec![integer_node(0, 10, 1), integer_node(0, 10, 2)];
    let elem_hi = vec![integer_node(0, 10, 1), integer_node(0, 10, 5)];
    assert!(sort_key(&elem_lo) < sort_key(&elem_hi));
    // Empty sequence is simplest.
    let empty: Vec<ChoiceNode> = Vec::new();
    assert!(sort_key(&empty) < sort_key(&a));
}

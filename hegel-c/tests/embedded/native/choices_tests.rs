use super::*;
use crate::native::core::choices::BooleanChoice;
use alloc::string::ToString;

#[test]
fn integer_choice_simplest_spans_zero() {
    assert_eq!(
        IntegerChoice {
            min_value: BigInt::from(-10),
            max_value: BigInt::from(10),
            shrink_towards: BigInt::from(0),
        }
        .simplest(),
        BigInt::from(0)
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
        BigInt::from(5)
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
        BigInt::from(-5)
    );
}

#[test]
fn integer_choice_unit_spans_zero() {
    assert_eq!(
        IntegerChoice {
            min_value: BigInt::from(-10),
            max_value: BigInt::from(10),
            shrink_towards: BigInt::from(0),
        }
        .unit(),
        BigInt::from(1)
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
        BigInt::from(6)
    );
}

#[test]
fn integer_choice_unit_all_negative() {
    assert_eq!(
        IntegerChoice {
            min_value: BigInt::from(-100),
            max_value: BigInt::from(-5),
            shrink_towards: BigInt::from(0),
        }
        .unit(),
        BigInt::from(-6)
    );
}

#[test]
fn integer_choice_unit_single_value_range() {
    assert_eq!(
        IntegerChoice {
            min_value: BigInt::from(5),
            max_value: BigInt::from(5),
            shrink_towards: BigInt::from(0),
        }
        .unit(),
        BigInt::from(5)
    );
}

#[test]
fn choice_kind_resolve_rejects_kind_value_mismatch() {
    let kind = ChoiceKind::Integer(std::sync::Arc::new(IntegerChoice {
        min_value: BigInt::from(0),
        max_value: BigInt::from(100),
        shrink_towards: BigInt::from(0),
    }));
    assert_eq!(kind.resolve(&ChoiceValue::Boolean(true)), None);
}

fn bu(n: u64) -> crate::native::bignum::BigUint {
    crate::native::bignum::BigUint::from(n)
}

#[test]
fn integer_bounded_range_gives_exact_count() {
    let kind = ChoiceKind::Integer(std::sync::Arc::new(IntegerChoice {
        min_value: BigInt::from(0),
        max_value: BigInt::from(200),
        shrink_towards: BigInt::from(0),
    }));
    assert_eq!(kind.max_children(), Some(bu(201)));
}

#[test]
fn integer_negative_range_gives_exact_count() {
    let kind = ChoiceKind::Integer(std::sync::Arc::new(IntegerChoice {
        min_value: BigInt::from(-10),
        max_value: BigInt::from(10),
        shrink_towards: BigInt::from(0),
    }));
    assert_eq!(kind.max_children(), Some(bu(21)));
}

#[test]
fn integer_full_i128_range_is_two_pow_128() {
    let kind = ChoiceKind::Integer(std::sync::Arc::new(IntegerChoice {
        min_value: BigInt::from(i128::MIN),
        max_value: BigInt::from(i128::MAX),
        shrink_towards: BigInt::from(0),
    }));
    let expected = crate::native::bignum::BigUint::from(u128::MAX) + bu(1);
    assert_eq!(kind.max_children(), Some(expected));
}

#[test]
fn max_children_saturating_boolean() {
    let kind = ChoiceKind::Boolean(BooleanChoice { p: 0.5 });
    assert_eq!(kind.max_children_saturating(1), 1);
    assert_eq!(kind.max_children_saturating(10), 2);
}

#[test]
fn max_children_saturating_integer_native() {
    let kind = ChoiceKind::Integer(std::sync::Arc::new(IntegerChoice {
        min_value: BigInt::from(0),
        max_value: BigInt::from(200),
        shrink_towards: BigInt::from(0),
    }));
    assert_eq!(kind.max_children_saturating(1000), 201);
    assert_eq!(kind.max_children_saturating(50), 50);
}

#[test]
fn max_children_saturating_integer_beyond_u128_saturates_to_cap() {
    use crate::native::bignum::BigUint;
    let ic = IntegerChoice {
        min_value: BigInt::from(0),
        max_value: BigInt::from(BigUint::from(2u32).pow(200)),
        shrink_towards: BigInt::from(0),
    };
    let kind = ChoiceKind::Integer(std::sync::Arc::new(ic));
    assert_eq!(kind.max_children_saturating(100), 100);
}

#[test]
fn max_children_saturating_float_matches_capped_exact() {
    use crate::native::bignum::ToPrimitive;
    let kind = ChoiceKind::Float(fc(0.0, 1.0, false, false));
    let exact = kind.max_children().unwrap().to_u128().unwrap();
    assert_eq!(kind.max_children_saturating(u128::MAX), exact);
    assert_eq!(kind.max_children_saturating(5), 5);
}

#[test]
fn max_children_saturating_bytes() {
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 2,
    });
    assert_eq!(kind.max_children_saturating(u128::MAX), 65793);
    assert_eq!(kind.max_children_saturating(1000), 1000);

    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 2,
        max_size: 3,
    });
    assert_eq!(kind.max_children_saturating(u128::MAX), 16_842_752);
}

#[test]
fn max_children_saturating_string() {
    let kind = ChoiceKind::String(string_choice(vec![(b'a' as u32, b'z' as u32)], 0, 2));
    assert_eq!(kind.max_children_saturating(u128::MAX), 703);

    let kind = ChoiceKind::String(string_choice(vec![(0, 0x10FFFF)], 0, 40));
    assert_eq!(kind.max_children_saturating(u128::MAX), u128::MAX);
}

#[test]
fn max_children_saturating_degenerate_and_huge_sizes_stay_cheap() {
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 1_000_000_000,
        max_size: usize::MAX,
    });
    assert_eq!(kind.max_children_saturating(1024), 1024);

    let kind = ChoiceKind::String(string_choice(vec![], 0, 0));
    assert_eq!(kind.max_children_saturating(1024), 1);

    let kind = ChoiceKind::String(string_choice(vec![(b'a' as u32, b'a' as u32)], 2, 5));
    assert_eq!(kind.max_children_saturating(1024), 4);
    let kind = ChoiceKind::String(string_choice(
        vec![(b'a' as u32, b'a' as u32)],
        0,
        usize::MAX,
    ));
    assert_eq!(kind.max_children_saturating(1024), 1024);
}

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
        let bv = BigInt::from(v);
        let idx = ic.to_index(&bv);
        assert_eq!(ic.from_index(idx), Some(bv), "round-trip failed for v={v}");
    }
}

#[test]
fn integer_choice_index_round_trip_all_positive() {
    let ic = integer_choice(5, 25);
    for v in 5i128..=25 {
        let bv = BigInt::from(v);
        let idx = ic.to_index(&bv);
        assert_eq!(ic.from_index(idx), Some(bv), "round-trip failed for v={v}");
    }
}

#[test]
fn integer_choice_index_round_trip_all_negative() {
    let ic = integer_choice(-25, -5);
    for v in -25i128..=-5 {
        let bv = BigInt::from(v);
        let idx = ic.to_index(&bv);
        assert_eq!(ic.from_index(idx), Some(bv), "round-trip failed for v={v}");
    }
}

#[test]
fn integer_choice_index_round_trip_asymmetric() {
    let ic = integer_choice(-5, 100);
    for v in -5i128..=100 {
        let bv = BigInt::from(v);
        let idx = ic.to_index(&bv);
        assert_eq!(ic.from_index(idx), Some(bv), "round-trip failed for v={v}");
    }
}

#[test]
fn integer_choice_index_round_trip_full_i128_range() {
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
        let bv = BigInt::from(v);
        let idx = ic.to_index(&bv);
        assert_eq!(ic.from_index(idx), Some(bv), "round-trip failed for v={v}");
    }
}

#[test]
fn integer_choice_index_round_trip_single_value() {
    let ic = integer_choice(42, 42);
    let idx = ic.to_index(&BigInt::from(42));
    assert_eq!(idx, crate::native::bignum::BigUint::from(0u32));
    assert_eq!(ic.from_index(idx), Some(BigInt::from(42)));
}

#[test]
fn integer_choice_from_index_past_max_returns_none() {
    let ic = integer_choice(0, 5);
    let big = crate::native::bignum::BigUint::from(100u32);
    assert_eq!(ic.from_index(big), None);
}

#[test]
fn integer_choice_from_index_overflowing_u128_returns_none() {
    let ic = integer_choice(i128::MIN, i128::MAX);
    let too_big = crate::native::bignum::BigUint::from(u128::MAX)
        + crate::native::bignum::BigUint::from(1u32);
    assert_eq!(ic.from_index(too_big), None);
}

#[test]
fn integer_choice_index_round_trip_nonzero_shrink_towards() {
    let ic = IntegerChoice {
        min_value: BigInt::from(-5),
        max_value: BigInt::from(40),
        shrink_towards: BigInt::from(7),
    };
    for v in -5i128..=40 {
        let bv = BigInt::from(v);
        let idx = ic.to_index(&bv);
        assert_eq!(ic.from_index(idx), Some(bv), "round-trip failed for v={v}");
    }
}

#[test]
fn integer_choice_index_round_trip_shrink_towards_clamped_outside_range() {
    let ic = IntegerChoice {
        min_value: BigInt::from(10),
        max_value: BigInt::from(30),
        shrink_towards: BigInt::from(-100),
    };
    for v in 10i128..=30 {
        let bv = BigInt::from(v);
        let idx = ic.to_index(&bv);
        assert_eq!(ic.from_index(idx), Some(bv), "round-trip failed for v={v}");
    }
}

#[test]
fn boolean_is_always_two() {
    assert_eq!(
        ChoiceKind::Boolean(BooleanChoice { p: 0.5 }).max_children(),
        Some(bu(2))
    );
}

fn fc(min: f64, max: f64, allow_nan: bool, allow_infinity: bool) -> FloatChoice {
    FloatChoice {
        min_value: min,
        max_value: max,
        allow_nan,
        allow_infinity,
        smallest_nonzero_magnitude: 5e-324,
    }
}

#[test]
fn float_choice_simplest_picks_zero_when_in_range() {
    assert_eq!(fc(-1.0, 1.0, false, false).simplest().unwrap(), 0.0);
    assert_eq!(fc(0.0, 10.0, false, false).simplest().unwrap(), 0.0);
}

#[test]
fn float_choice_simplest_picks_closest_endpoint_when_zero_excluded() {
    assert_eq!(fc(1.5, 10.0, false, false).simplest().unwrap(), 2.0);
    assert_eq!(fc(-5.0, -1.5, false, false).simplest().unwrap(), -2.0);
}

#[test]
fn float_choice_simplest_finds_simple_fraction_in_tight_range() {
    assert_eq!(fc(1.4, 1.6, false, false).simplest().unwrap(), 1.5);
}

#[test]
fn float_choice_simplest_falls_back_to_infinity_when_no_finite_values() {
    let fc = fc(f64::INFINITY, f64::INFINITY, false, true);
    assert_eq!(fc.simplest().unwrap(), f64::INFINITY);
    let fc = fc_neg();
    assert_eq!(fc.simplest().unwrap(), f64::NEG_INFINITY);
}

fn fc_neg() -> FloatChoice {
    fc(f64::NEG_INFINITY, f64::NEG_INFINITY, false, true)
}

#[test]
fn float_choice_simplest_falls_back_to_nan_when_only_nan_allowed() {
    let fc = FloatChoice {
        min_value: f64::INFINITY,
        max_value: f64::NEG_INFINITY,
        allow_nan: true,
        allow_infinity: false,
        smallest_nonzero_magnitude: 5e-324,
    };
    assert!(fc.simplest().unwrap().is_nan());
}

#[test]
fn float_choice_simplest_errors_when_nothing_valid() {
    let fc = FloatChoice {
        min_value: f64::INFINITY,
        max_value: f64::NEG_INFINITY,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: 5e-324,
    };
    let msg = fc.simplest().unwrap_err().to_string();
    assert!(
        msg.contains("FloatChoice::simplest: no valid float"),
        "{msg}"
    );
    assert!(msg.contains("bug in hegel"), "{msg}");
}

#[test]
fn float_choice_unit_falls_through_to_simplest_on_nan_start() {
    let fc = FloatChoice {
        min_value: f64::INFINITY,
        max_value: f64::NEG_INFINITY,
        allow_nan: true,
        allow_infinity: false,
        smallest_nonzero_magnitude: 5e-324,
    };
    assert!(fc.unit().unwrap().is_nan());
}

#[test]
fn float_choice_unit_finds_a_distinct_value_in_sub_integer_ranges() {
    let c = fc(0.0, 0.5, false, false);
    let u = c.unit().unwrap();
    assert!(c.validate(u));
    assert_ne!(
        u.to_bits(),
        c.simplest().unwrap().to_bits(),
        "[0, 0.5] has a second value, so unit() must not collapse to simplest"
    );

    let c = fc(-0.5, 0.0, false, false);
    let u = c.unit().unwrap();
    assert!(c.validate(u));
    assert_ne!(u.to_bits(), c.simplest().unwrap().to_bits());
}

#[test]
fn float_choice_enumerate_returns_none() {
    let kind = ChoiceKind::Float(fc(0.0, 1.0, false, false));
    assert!(kind.enumerate(u64::MAX).is_none());
}

#[test]
fn float_choice_to_from_index_round_trip() {
    let kind = ChoiceKind::Float(fc(-10.0, 10.0, false, false));
    for v in [0.0_f64, 1.0, 2.0, -1.0, -2.0, 0.5, -0.5, 4.25] {
        let data = kind.resolve(&ChoiceValue::Float(v)).unwrap();
        let idx = data.to_index().unwrap().unwrap();
        let back = kind.from_index(idx).unwrap().unwrap();
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
        smallest_nonzero_magnitude: 5e-324,
    };
    for v in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        let idx = fc.to_index(v).unwrap();
        let back = fc.from_index(idx).unwrap().unwrap();
        if v.is_nan() {
            assert!(back.is_nan());
        } else {
            assert_eq!(back, v);
        }
    }
}

#[test]
fn float_choice_simplest_finds_simple_fraction_below_one() {
    assert_eq!(fc(0.1, 0.9, false, false).simplest().unwrap(), 0.5);
    assert_eq!(fc(-0.9, -0.1, false, false).simplest().unwrap(), -0.5);
}

#[test]
fn float_choice_simplest_is_exact_for_deep_fractions() {
    assert_eq!(fc(1.10, 1.11, false, false).simplest().unwrap(), 1.109375);
    assert_eq!(fc(0.3, 0.4, false, false).simplest().unwrap(), 0.375);
}

#[test]
fn float_choice_to_index_does_not_underflow_for_fraction_only_ranges() {
    let choice = fc(0.1, 0.9, false, false);
    for v in [0.1, 0.5, 0.9, 0.25, 0.125, 0.7] {
        let idx = choice.to_index(v).unwrap();
        assert_eq!(choice.from_index(idx).unwrap(), Some(v));
    }
}

#[test]
fn float_choice_simplest_dominates_sampled_probes() {
    let ranges = [
        (0.1, 0.9),
        (1.4, 1.6),
        (1.10, 1.11),
        (2.5, 2.7),
        (1e-10, 1e-5),
        (1e300, 1.7e308),
        (65672.5, 65673.0),
        (-0.9, -0.1),
        (-1e-5, -1e-10),
        (-3.7, 9.2),
        (0.1, 1e10),
        (5e-324, 4e-324),
    ];
    for (lo, hi) in ranges {
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        let choice = fc(lo, hi, false, false);
        let s = choice.simplest().unwrap();
        assert!(choice.validate(s), "simplest {s} invalid for [{lo}, {hi}]");
        let key_s = choice.sort_key(s);
        for i in 0..=1000 {
            let probe = lo + (hi - lo) * (i as f64 / 1000.0);
            if !choice.validate(probe) {
                continue;
            }
            assert!(
                key_s <= choice.sort_key(probe),
                "simplest {s} ranks above {probe} for [{lo}, {hi}]"
            );
        }
    }
}

#[test]
fn float_choice_validate_respects_smallest_nonzero_magnitude() {
    let c = FloatChoice {
        min_value: -1.0,
        max_value: 1.0,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: f64::MIN_POSITIVE,
    };
    assert!(c.validate(0.0));
    assert!(c.validate(-0.0));
    assert!(c.validate(f64::MIN_POSITIVE));
    assert!(c.validate(-f64::MIN_POSITIVE));
    assert!(!c.validate(5e-324), "subnormal must be excluded");
    assert!(!c.validate(-1e-310), "subnormal must be excluded");
}

#[test]
fn float_choice_simplest_respects_smallest_nonzero_magnitude() {
    let c = FloatChoice {
        min_value: 5e-324,
        max_value: 1.0,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: f64::MIN_POSITIVE,
    };
    assert_eq!(c.simplest().unwrap(), 1.0);
    let c2 = FloatChoice {
        min_value: 5e-324,
        max_value: 1e-300,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: f64::MIN_POSITIVE,
    };
    let s = c2.simplest().unwrap();
    assert!(
        c2.validate(s),
        "simplest {s} must respect the magnitude floor"
    );
}

#[test]
fn float_choice_nan_payloads_round_trip_through_index() {
    let choice = fc(f64::NEG_INFINITY, f64::INFINITY, true, true);
    for bits in [
        0x7FF8_0000_0000_0000_u64,
        0x7FF0_0000_0000_0001,
        0xFFF8_0000_0000_0000,
        0xFFF0_0000_0000_0001,
        0x7FFF_FFFF_FFFF_FFFF,
    ] {
        let v = f64::from_bits(bits);
        let idx = choice.to_index(v).unwrap();
        let back = choice.from_index(idx).unwrap().unwrap();
        assert_eq!(back.to_bits(), bits, "NaN payload mangled");
    }
}

#[test]
fn float_choice_from_index_rejects_past_max_index() {
    let choice = fc(f64::NEG_INFINITY, f64::INFINITY, true, true);
    assert!(choice.from_index(choice.max_index()).unwrap().is_some());
    for extra in [1u32, 2, 50] {
        assert_eq!(
            choice
                .from_index(choice.max_index() + BigUint::from(extra))
                .unwrap(),
            None,
            "index past max_index must be rejected"
        );
    }
}

#[test]
fn float_choice_from_index_rejects_non_canonical_tag0_ranks() {
    let choice = fc(f64::NEG_INFINITY, f64::INFINITY, true, true);
    assert_eq!(choice.from_index(BigUint::from(1u128 << 57)).unwrap(), None);
}

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
        assert_eq!(bc.from_index(idx).unwrap(), Some(v));
    }
}

#[test]
fn bytes_choice_from_index_past_max_returns_none() {
    let bc = BytesChoice {
        min_size: 0,
        max_size: 1,
    };
    assert!(
        bc.from_index(crate::native::bignum::BigUint::from(1000u32))
            .unwrap()
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
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 4,
    });
    assert!(kind.enumerate(u64::MAX).is_none());
}

fn string_choice(intervals: Vec<(u32, u32)>, min_size: usize, max_size: usize) -> StringChoice {
    StringChoice {
        intervals: crate::native::intervalsets::IntervalSet::new(intervals)
            .unwrap()
            .into(),
        min_size,
        max_size,
    }
}

#[test]
fn string_choice_simplest_is_first_shrink_order_position() {
    let sc = string_choice(vec![(b'a' as u32, b'z' as u32)], 0, 1);
    assert_eq!(sc.simplest_codepoint().unwrap(), b'a' as u32);
}

#[test]
fn string_choice_simplest_prefers_zero_when_alphabet_contains_digits() {
    let sc = string_choice(vec![(0, 0xD7FF), (0xE000, 0x10FFFF)], 0, 1);
    assert_eq!(sc.simplest_codepoint().unwrap(), b'0' as u32);
}

#[test]
fn string_choice_unit_single_codepoint_alphabet_at_max_size_falls_back_to_simplest() {
    let sc = string_choice(vec![(0x41, 0x41)], 2, 2);
    assert_eq!(sc.unit().unwrap(), vec![0x41, 0x41]);
}

#[test]
fn string_choice_unit_empty_fixed_length_falls_back_to_simplest() {
    let sc = string_choice(vec![(0, 100)], 0, 0);
    assert_eq!(sc.unit().unwrap(), Vec::<u32>::new());
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
    let lower = string_choice(vec![(b'a' as u32, b'z' as u32)], 0, 5);
    assert_eq!(lower.codepoint_key(b'a' as u32), 0);
    assert_eq!(lower.codepoint_key(b'z' as u32), 25);

    let full = string_choice(vec![(0, 0xD7FF), (0xE000, 0x10FFFF)], 0, 5);
    assert_eq!(full.codepoint_key(b'0' as u32), 0);
    assert_eq!(full.codepoint_key(b'Z' as u32), 42);
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
        assert_eq!(sc.from_index(idx).unwrap(), Some(v));
    }
}

#[test]
fn string_choice_from_index_past_max_returns_none() {
    let sc = string_choice(vec![(b'a' as u32, b'b' as u32)], 0, 1);
    assert!(
        sc.from_index(crate::native::bignum::BigUint::from(1000u32))
            .unwrap()
            .is_none()
    );
}

fn integer_node(min: i128, max: i128, value: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(min),
            max_value: BigInt::from(max),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
        false,
    )
}

fn bytes_node(min: usize, max: usize, value: Vec<u8>) -> ChoiceNode {
    ChoiceNode::bytes(
        BytesChoice {
            min_size: min,
            max_size: max,
        },
        value,
        false,
    )
}

fn string_node(intervals: Vec<(u32, u32)>, min: usize, max: usize, value: Vec<u32>) -> ChoiceNode {
    ChoiceNode::string(string_choice(intervals, min, max), value, false)
}

#[test]
fn node_sort_key_ref_scalar_equality_and_partial_cmp() {
    use std::cmp::Ordering;
    let a = integer_node(-10, 10, 3);
    let b = integer_node(-10, 10, 3);
    let c = integer_node(-10, 10, 4);
    assert!(a.sort_key_ref() == b.sort_key_ref());
    assert!(a.sort_key_ref() != c.sort_key_ref());
    assert_eq!(
        a.sort_key_ref().partial_cmp(&c.sort_key_ref()),
        Some(Ordering::Less)
    );
}

#[test]
fn node_sort_key_ref_bytes_orders_shortlex() {
    let short = bytes_node(0, 4, vec![0xff, 0xff]);
    let longer = bytes_node(0, 4, vec![0x00, 0x00, 0x00]);
    assert!(short.sort_key_ref() < longer.sort_key_ref());
    let equal_a = bytes_node(0, 4, vec![1, 2, 3]);
    let equal_b = bytes_node(0, 4, vec![1, 2, 3]);
    assert!(equal_a.sort_key_ref() == equal_b.sort_key_ref());
    let lex_lo = bytes_node(0, 4, vec![1, 2, 3]);
    let lex_hi = bytes_node(0, 4, vec![1, 2, 4]);
    assert!(lex_lo.sort_key_ref() < lex_hi.sort_key_ref());
}

#[test]
fn node_sort_key_ref_string_orders_by_codepoint_key() {
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
    let empty: Vec<ChoiceNode> = Vec::new();
    assert!(sort_key(&empty) < sort_key(&a));
}

#[test]
fn choice_kind_unit_dispatches_to_each_sub_kind() {
    let ic = IntegerChoice {
        min_value: BigInt::from(0),
        max_value: BigInt::from(10),
        shrink_towards: BigInt::from(0),
    };
    assert_eq!(
        ChoiceKind::Integer(std::sync::Arc::new(ic.clone()))
            .unit()
            .unwrap(),
        ChoiceValue::Integer(ic.unit())
    );

    assert_eq!(
        ChoiceKind::Boolean(BooleanChoice { p: 0.5 })
            .unit()
            .unwrap(),
        ChoiceValue::Boolean(BooleanChoice { p: 0.5 }.unit())
    );

    let fch = fc(0.0, 10.0, false, false);
    assert_eq!(
        ChoiceKind::Float(fch.clone()).unit().unwrap(),
        ChoiceValue::Float(fch.unit().unwrap())
    );

    let bc = BytesChoice {
        min_size: 0,
        max_size: 4,
    };
    assert_eq!(
        ChoiceKind::Bytes(bc.clone()).unit().unwrap(),
        ChoiceValue::Bytes(bc.unit())
    );

    let sc = string_choice(vec![(b'a' as u32, b'z' as u32)], 0, 4);
    assert_eq!(
        ChoiceKind::String(sc.clone()).unit().unwrap(),
        ChoiceValue::String(sc.unit().unwrap())
    );
}

#[test]
fn engine_error_display_covers_all_variants() {
    assert!(EngineError::Overrun.to_string().contains("Overrun"));
    assert!(EngineError::InvalidTestCase.to_string().contains("Invalid"));
    assert!(EngineError::AssumeViolation.to_string().contains("Assume"));
    assert_eq!(
        EngineError::InvalidArgument("nope".to_string()).to_string(),
        "nope"
    );
    let internal = EngineError::from(crate::control::InternalError::new(format_args!("boom")));
    assert!(matches!(internal, EngineError::Internal(_)));
    let msg = internal.to_string();
    assert!(msg.contains("boom"), "{msg}");
    assert!(msg.contains("bug in hegel"), "{msg}");
}

fn big_integer_node(distance_beyond_u128: u32) -> ChoiceNode {
    let huge = BigInt::from(u128::MAX) * BigInt::from(4) + BigInt::from(distance_beyond_u128);
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(0),
            max_value: huge.clone(),
            shrink_towards: BigInt::from(0),
        },
        huge,
        false,
    )
}

#[test]
fn node_sort_key_big_integer_orders_correctly() {
    use crate::native::core::sort_key;
    let scalar = vec![integer_node(0, 100, 50)];
    let big = vec![big_integer_node(0)];
    assert!(sort_key(&scalar) < sort_key(&big));
    let big_small = vec![big_integer_node(0)];
    let big_large = vec![big_integer_node(7)];
    assert!(sort_key(&big_small) < sort_key(&big_large));
    let bytes = vec![ChoiceNode::bytes(
        BytesChoice {
            min_size: 0,
            max_size: 4,
        },
        vec![1, 2, 3],
        false,
    )];
    assert!(sort_key(&big) < sort_key(&bytes));
}

#[test]
fn enumerate_large_max_size_bytes_returns_none_without_blowup() {
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 1_000_000,
    });
    assert_eq!(kind.enumerate(256), None);
}

#[test]
fn bytes_max_index_and_max_children() {
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 2,
    });
    assert_eq!(kind.max_index(), Some(bu(65792)));
    assert_eq!(kind.max_children(), Some(bu(65793)));
}

#[test]
fn string_max_index_and_max_children() {
    let kind = ChoiceKind::String(StringChoice {
        intervals: crate::native::intervalsets::IntervalSet::new(vec![(b'a' as u32, b'c' as u32)])
            .unwrap()
            .into(),
        min_size: 0,
        max_size: 2,
    });
    assert_eq!(kind.max_index(), Some(bu(12)));
    assert_eq!(kind.max_children(), Some(bu(13)));
}

#[test]
fn bytes_to_index_via_dispatch() {
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 4,
    });
    let data = |v: Vec<u8>| kind.resolve(&ChoiceValue::Bytes(v)).unwrap();
    assert_eq!(data(vec![]).to_index().unwrap(), Some(bu(0)));
    assert_eq!(data(vec![0]).to_index().unwrap(), Some(bu(1)));
}

#[test]
fn string_to_index_via_dispatch() {
    let kind = ChoiceKind::String(StringChoice {
        intervals: crate::native::intervalsets::IntervalSet::new(vec![(b'a' as u32, b'c' as u32)])
            .unwrap()
            .into(),
        min_size: 0,
        max_size: 4,
    });
    let data = kind.resolve(&ChoiceValue::String(vec![])).unwrap();
    assert_eq!(data.to_index().unwrap(), Some(bu(0)));
}

/// A boolean drawn via `random_value` must sample from the kind's recorded
/// `p`; a hardcoded 0.5 used to flip every recorded boolean half the time.
#[test]
fn random_value_boolean_respects_p() {
    use crate::native::rng::EngineRng;

    let rare = ChoiceKind::Boolean(BooleanChoice { p: 0.01 });
    let mut rng = EngineRng::seeded(2024);
    let mut trues = 0;
    for _ in 0..1000 {
        if rare.random_value(&mut rng).unwrap().unwrap() == ChoiceValue::Boolean(true) {
            trues += 1;
        }
    }
    assert!(trues < 50, "p = 0.01 produced {trues}/1000 trues");

    let common = ChoiceKind::Boolean(BooleanChoice { p: 0.99 });
    let mut trues = 0;
    for _ in 0..1000 {
        if common.random_value(&mut rng).unwrap().unwrap() == ChoiceValue::Boolean(true) {
            trues += 1;
        }
    }
    assert!(trues > 950, "p = 0.99 produced only {trues}/1000 trues");
}

/// `p <= 0` and `p >= 1` must resolve deterministically rather than reach
/// the precise sampler, which panics outside `(0, 1)`.
#[test]
fn random_value_boolean_degenerate_p_is_deterministic() {
    use crate::native::rng::EngineRng;

    let mut rng = EngineRng::seeded(1);
    let never = ChoiceKind::Boolean(BooleanChoice { p: 0.0 });
    let always = ChoiceKind::Boolean(BooleanChoice { p: 1.0 });
    for _ in 0..20 {
        assert_eq!(
            never.random_value(&mut rng).unwrap().unwrap(),
            ChoiceValue::Boolean(false)
        );
        assert_eq!(
            always.random_value(&mut rng).unwrap().unwrap(),
            ChoiceValue::Boolean(true)
        );
    }
}

#[test]
fn choice_value_equality_is_false_across_variants() {
    let values = [
        ChoiceValue::Integer(BigInt::from(0)),
        ChoiceValue::Boolean(false),
        ChoiceValue::Float(0.0),
        ChoiceValue::Bytes(Vec::new()),
        ChoiceValue::String(Vec::new()),
    ];
    for (i, a) in values.iter().enumerate() {
        for (j, b) in values.iter().enumerate() {
            if i == j {
                assert_eq!(a, b);
            } else {
                assert_ne!(a, b);
            }
        }
    }
}

#[test]
fn string_choice_empty_alphabet_zero_max_size_has_empty_simplest_and_unit() {
    let sc = string_choice(vec![], 0, 0);
    assert_eq!(sc.simplest().unwrap(), Vec::<u32>::new());
    assert_eq!(sc.unit().unwrap(), Vec::<u32>::new());
    assert!(sc.validate(&[]));
}

#[test]
fn string_choice_empty_alphabet_zero_max_size_indexing_round_trips() {
    use crate::native::bignum::BigUint;
    let sc = string_choice(vec![], 0, 0);
    assert_eq!(sc.max_index(), BigUint::from(0u32));
    assert_eq!(sc.to_index(&[]), BigUint::from(0u32));
    assert_eq!(
        sc.from_index(BigUint::from(0u32)).unwrap(),
        Some(Vec::new())
    );
    assert_eq!(sc.from_index(BigUint::from(1u32)).unwrap(), None);
}

#[test]
fn string_choice_simplest_on_empty_alphabet_is_an_internal_error() {
    let sc = string_choice(vec![], 0, 1);
    let msg = sc.simplest_codepoint().unwrap_err().to_string();
    assert!(msg.contains("empty alphabet"), "{msg}");
    assert!(msg.contains("bug in hegel"), "{msg}");
}

fn boolean_node(value: bool) -> ChoiceNode {
    ChoiceNode::boolean(BooleanChoice { p: 0.5 }, value, false)
}

fn clone_node(children: Vec<ChoiceNode>) -> ChoiceNode {
    ChoiceNode::clone_stream(
        std::sync::Arc::new(RealizedStream::new(children, Vec::new(), Vec::new())),
        false,
    )
}

fn values_clone_value(values: Vec<ChoiceValue>) -> ChoiceValue {
    ChoiceValue::Clone(std::sync::Arc::new(CloneRecord::from_values(values)))
}

fn value_hash(v: &ChoiceValue) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

#[test]
fn clone_data_with_value_accepts_only_realized_clone_values() {
    let data = clone_node(Vec::new()).data;
    assert!(data.with_value(&clone_node(Vec::new()).value()).is_some());
    assert!(data.with_value(&values_clone_value(Vec::new())).is_none());
    assert!(
        data.with_value(&ChoiceValue::Integer(BigInt::from(0)))
            .is_none()
    );
    assert!(data.with_value(&ChoiceValue::Boolean(false)).is_none());
    assert!(
        boolean_node(false)
            .with_value(&clone_node(Vec::new()).value())
            .is_none()
    );
    assert!(
        integer_node(0, 10, 0)
            .with_value(&clone_node(Vec::new()).value())
            .is_none()
    );
}

#[test]
fn clone_kind_simplest_and_unit_are_the_empty_clone() {
    let empty = values_clone_value(Vec::new());
    assert_eq!(ChoiceKind::Clone.simplest().unwrap(), empty);
    assert_eq!(ChoiceKind::Clone.unit().unwrap(), empty);
}

#[test]
fn simplest_clone_value_sort_key_compares_equal_to_an_executed_empty_stream() {
    let executed = clone_node(Vec::new());
    let punned = executed
        .with_value(&ChoiceKind::Clone.simplest().unwrap())
        .unwrap();
    assert!(punned.sort_key_ref() == executed.sort_key_ref());
    assert!(executed.sort_key_ref() == punned.sort_key_ref());
}

#[test]
fn clone_kind_max_children_saturating_is_cap() {
    assert_eq!(ChoiceKind::Clone.max_children_saturating(17), 17);
    assert_eq!(
        ChoiceKind::Clone.max_children_saturating(u128::MAX),
        u128::MAX
    );
}

#[test]
fn clone_kind_enumerate_returns_none() {
    assert!(ChoiceKind::Clone.enumerate(1024).is_none());
}

#[test]
fn clone_value_equality_ignores_realized_info() {
    let children = vec![boolean_node(true), integer_node(0, 10, 3)];
    let realized = ChoiceValue::Clone(std::sync::Arc::new(CloneRecord::from_run(
        children.clone(),
        vec![Span {
            start: 0,
            end: 2,
            label: "17".to_string(),
            depth: 0,
            parent: None,
            discarded: false,
        }],
        vec![(0, SpanEvent::Open { label: 17 })],
    )));
    let values_only = values_clone_value(children.iter().map(|n| n.value().clone()).collect());
    assert_eq!(realized, values_only);
    assert_eq!(value_hash(&realized), value_hash(&values_only));
}

#[test]
fn clone_value_equality_compares_child_values_recursively() {
    let a = clone_node(vec![
        boolean_node(true),
        clone_node(vec![integer_node(0, 10, 3)]),
    ])
    .value();
    let same = clone_node(vec![
        boolean_node(true),
        clone_node(vec![integer_node(0, 10, 3)]),
    ])
    .value();
    let different = clone_node(vec![
        boolean_node(true),
        clone_node(vec![integer_node(0, 10, 4)]),
    ])
    .value();
    let shorter = clone_node(vec![boolean_node(true)]).value();
    assert_eq!(a, same);
    assert_eq!(value_hash(&a), value_hash(&same));
    assert_ne!(a, different);
    assert_ne!(a, shorter);
    assert_ne!(a, ChoiceValue::Boolean(true));
}

#[test]
fn flattened_len_counts_clone_children_recursively() {
    let nodes = vec![
        boolean_node(false),
        clone_node(vec![
            boolean_node(true),
            clone_node(vec![integer_node(0, 10, 1)]),
        ]),
    ];
    assert_eq!(flattened_len(&nodes), 5);
    assert_eq!(flattened_len(&[boolean_node(true)]), 1);
    assert_eq!(flattened_len(&[clone_node(Vec::new())]), 1);
    assert_eq!(flattened_len(&[]), 0);
}

#[test]
fn clone_record_flat_len_agrees_across_representations() {
    let children = vec![boolean_node(true), clone_node(vec![boolean_node(false)])];
    let from_values =
        CloneRecord::from_values(children.iter().map(|n| n.value().clone()).collect());
    let from_run = CloneRecord::from_run(children, Vec::new(), Vec::new());
    assert_eq!(from_values.flat_len(), 3);
    assert_eq!(from_run.flat_len(), 3);
}

#[test]
fn nodes_sort_key_orders_by_flattened_count_first() {
    use crate::native::core::sort_key;
    let wrapped = vec![clone_node(vec![boolean_node(false), boolean_node(false)])];
    let flat = vec![boolean_node(true), boolean_node(true)];
    assert!(sort_key(&flat) < sort_key(&wrapped));
}

#[test]
fn nodes_sort_key_shrinking_inside_clone_is_smaller() {
    use crate::native::core::sort_key;
    let big = vec![clone_node(vec![integer_node(0, 10, 5)])];
    let small = vec![clone_node(vec![integer_node(0, 10, 2)])];
    assert!(sort_key(&small) < sort_key(&big));
    let fewer = vec![clone_node(vec![boolean_node(true)])];
    let more = vec![clone_node(vec![boolean_node(false), boolean_node(false)])];
    assert!(sort_key(&fewer) < sort_key(&more));
}

#[test]
fn nodes_sort_key_equal_flat_count_shorter_top_level_wins() {
    use crate::native::core::sort_key;
    let nested = vec![clone_node(vec![boolean_node(true)]), boolean_node(true)];
    let flat = vec![
        boolean_node(false),
        boolean_node(false),
        boolean_node(false),
    ];
    assert_eq!(flattened_len(&nested), flattened_len(&flat));
    assert!(sort_key(&nested) < sort_key(&flat));
}

#[test]
fn node_sort_key_ref_clone_category_above_scalars_and_sequences() {
    let scalar = boolean_node(true);
    let bytes = bytes_node(0, 4, vec![1, 2]);
    let clone = clone_node(vec![boolean_node(true)]);
    assert!(scalar.sort_key_ref() < clone.sort_key_ref());
    assert!(bytes.sort_key_ref() < clone.sort_key_ref());
    assert!(clone.sort_key_ref() == clone_node(vec![boolean_node(true)]).sort_key_ref());
}

#[test]
fn clone_record_accessors_expose_children_and_realized_info() {
    let span = Span {
        start: 0,
        end: 1,
        label: "42".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    };
    let record = CloneRecord::from_run(
        vec![boolean_node(true)],
        vec![span.clone()],
        vec![(0, SpanEvent::Open { label: 42 })],
    );
    assert_eq!(record.len(), 1);
    assert!(!record.is_empty());
    assert!(record.value_at(0) == ChoiceValue::Boolean(true));
    assert_eq!(record.owned_values(), vec![ChoiceValue::Boolean(true)]);
    assert_eq!(record.realized_nodes().unwrap().len(), 1);
    let stream = record.realized().unwrap();
    assert_eq!(stream.spans(), &[span]);
    assert_eq!(stream.span_events(), &[(0, SpanEvent::Open { label: 42 })]);

    let values_only = CloneRecord::from_values(vec![ChoiceValue::Boolean(true)]);
    assert_eq!(values_only.len(), 1);
    assert!(values_only.value_at(0) == ChoiceValue::Boolean(true));
    assert_eq!(values_only.owned_values(), vec![ChoiceValue::Boolean(true)]);
    assert!(values_only.realized_nodes().is_none());
    assert!(values_only.realized().is_none());

    let empty = CloneRecord::empty();
    assert!(empty.is_empty());
    assert_eq!(empty.flat_len(), 0);
}

#[test]
fn clone_vs_clone_ordering_compares_flat_len_then_child_count() {
    use crate::native::core::sort_key;
    let a = vec![
        clone_node(vec![boolean_node(false)]),
        clone_node(vec![boolean_node(false), boolean_node(false)]),
    ];
    let b = vec![
        clone_node(vec![boolean_node(false), boolean_node(false)]),
        clone_node(vec![boolean_node(false)]),
    ];
    assert_eq!(flattened_len(&a), flattened_len(&b));
    assert!(sort_key(&a) < sort_key(&b));

    let shallow = vec![clone_node(vec![boolean_node(true), boolean_node(true)])];
    let deep = vec![clone_node(vec![clone_node(vec![boolean_node(true)])])];
    assert_eq!(flattened_len(&deep), flattened_len(&shallow));
    assert!(sort_key(&deep) < sort_key(&shallow));
}

#[test]
fn string_choice_from_index_on_empty_alphabet_with_nonzero_max_is_an_internal_error() {
    use crate::native::bignum::BigUint;
    let sc = string_choice(vec![], 0, 1);
    let msg = sc.from_index(BigUint::from(0u32)).unwrap_err().to_string();
    assert!(msg.contains("empty alphabet"), "{msg}");
}

#[test]
fn choice_value_ref_equality_is_false_across_variants() {
    let int_v = ChoiceValue::Integer(BigInt::from(0));
    let bool_v = ChoiceValue::Boolean(false);
    assert_ne!(ChoiceValueRef::from(&int_v), ChoiceValueRef::from(&bool_v));
    assert_eq!(ChoiceValueRef::from(&int_v), ChoiceValueRef::from(&int_v));
}

#[test]
fn clone_values_is_empty_matches_child_count() {
    let empty = CloneRecord::from_values(Vec::new());
    assert!(CloneValues::Record(&empty).is_empty());
    let stream = RealizedStream::new(vec![boolean_node(false)], Vec::new(), Vec::new());
    assert!(!CloneValues::Stream(&stream).is_empty());
}

#[test]
fn clone_kind_random_value_is_none() {
    let mut rng = crate::native::rng::EngineRng::seeded(0);
    assert!(ChoiceKind::Clone.random_value(&mut rng).unwrap().is_none());
}

#[test]
fn choice_data_equality_compares_constraint_and_value() {
    let pairs = [
        (integer_node(0, 10, 3).data, integer_node(0, 10, 4).data),
        (
            ChoiceData::Boolean(BooleanChoice { p: 0.5 }, false),
            ChoiceData::Boolean(BooleanChoice { p: 0.5 }, true),
        ),
        (
            ChoiceNode::float(fc(0.0, 1.0, false, false), 0.25, false).data,
            ChoiceNode::float(fc(0.0, 1.0, false, false), 0.5, false).data,
        ),
        (
            bytes_node(0, 4, vec![1]).data,
            bytes_node(0, 4, vec![2]).data,
        ),
        (
            string_node(vec![(b'a' as u32, b'z' as u32)], 0, 4, vec![b'a' as u32]).data,
            string_node(vec![(b'a' as u32, b'z' as u32)], 0, 4, vec![b'b' as u32]).data,
        ),
    ];
    for (a, b) in &pairs {
        assert_eq!(a, a);
        assert_ne!(a, b);
    }
    for (i, (a, _)) in pairs.iter().enumerate() {
        for (j, (b, _)) in pairs.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}

#[test]
fn choice_data_equality_differs_on_constraint_with_equal_values() {
    assert_ne!(integer_node(0, 10, 3).data, integer_node(0, 20, 3).data);
    assert_ne!(
        ChoiceNode::float(fc(0.0, 1.0, false, false), 0.5, false).data,
        ChoiceNode::float(fc(0.0, 2.0, false, false), 0.5, false).data
    );
    assert_ne!(
        bytes_node(0, 4, vec![1]).data,
        bytes_node(0, 5, vec![1]).data
    );
    assert_ne!(
        string_node(vec![(b'a' as u32, b'z' as u32)], 0, 4, vec![b'a' as u32]).data,
        string_node(vec![(b'a' as u32, b'z' as u32)], 0, 5, vec![b'a' as u32]).data
    );
}

#[test]
fn choice_data_as_float_is_none_for_other_kinds() {
    assert!(integer_node(0, 10, 3).data.as_float().is_none());
    assert!(
        ChoiceNode::float(fc(0.0, 1.0, false, false), 0.5, false)
            .data
            .as_float()
            .is_some()
    );
}

#[test]
fn clone_kind_has_no_dense_index() {
    assert_eq!(
        ChoiceKind::Clone
            .from_index(crate::native::bignum::BigUint::from(0u32))
            .unwrap(),
        None
    );
    assert_eq!(ChoiceKind::Clone.max_index(), None);
}

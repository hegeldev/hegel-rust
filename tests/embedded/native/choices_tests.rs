use super::*;

// ── IntegerChoice::simplest ─────────────────────────────────────────────────
//
// Ports of pbtkit/tests/test_core.py::test_integer_choice_simplest.

#[test]
fn integer_choice_simplest_spans_zero() {
    assert_eq!(
        IntegerChoice {
            min_value: -10,
            max_value: 10,
            shrink_towards: 0,
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
            shrink_towards: 0,
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
            shrink_towards: 0,
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
            shrink_towards: 0,
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
            shrink_towards: 0,
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
            shrink_towards: 0,
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
            shrink_towards: 0,
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
        min_value: 0,
        max_value: 100,
        shrink_towards: 0,
    });
    let _ = kind.to_index(&ChoiceValue::Boolean(true));
}

// ── ChoiceKind::max_children ──────────────────────────────────────────────
//
// Ports of `compute_max_children` tests from
// `hypothesis-python/tests/conjecture/test_utils.py` plus hegel-specific
// checks for the choice kinds hegel's native engine actually records.

fn bu(n: u64) -> crate::native::bignum::BigUint {
    crate::native::bignum::BigUint::from(n)
}

#[test]
fn integer_bounded_range_gives_exact_count() {
    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: 0,
        max_value: 200,
        shrink_towards: 0,
    });
    assert_eq!(kind.max_children(), bu(201));
}

#[test]
fn integer_negative_range_gives_exact_count() {
    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: -10,
        max_value: 10,
        shrink_towards: 0,
    });
    assert_eq!(kind.max_children(), bu(21));
}

#[test]
fn integer_full_i128_range_is_two_pow_128() {
    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: i128::MIN,
        max_value: i128::MAX,
        shrink_towards: 0,
    });
    // 2^128 = u128::MAX + 1.
    let expected = crate::native::bignum::BigUint::from(u128::MAX) + bu(1);
    assert_eq!(kind.max_children(), expected);
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
        bc.from_index(crate::native::bignum::BigUint::from(1000u32))
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

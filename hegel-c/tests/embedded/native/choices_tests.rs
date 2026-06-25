use super::*;

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
    // simplest is at the top of the range, so unit should fall back to
    // simplest - 1 = -6.
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
    // When the range is a single value, unit falls back to simplest.
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

fn bu(n: u64) -> crate::native::bignum::BigUint {
    crate::native::bignum::BigUint::from(n)
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
    let expected = crate::native::bignum::BigUint::from(u128::MAX) + bu(1);
    assert_eq!(kind.max_children(), expected);
}

// ── ChoiceKind::max_children_saturating ─────────────────────────────────────
//
// `max_children_saturating(cap)` must equal `min(max_children(), cap)` for
// every kind, computed without materialising the huge sequence-cardinality
// `BigUint` (the `pow` that dominated generation profiles).

#[test]
fn max_children_saturating_boolean() {
    let kind = ChoiceKind::Boolean(BooleanChoice);
    assert_eq!(kind.max_children_saturating(1), 1); // capped below the count
    assert_eq!(kind.max_children_saturating(10), 2); // exact
}

#[test]
fn max_children_saturating_integer_native() {
    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: BigInt::from(0),
        max_value: BigInt::from(200),
        shrink_towards: BigInt::from(0),
    });
    assert_eq!(kind.max_children_saturating(1000), 201); // exact (span + 1)
    assert_eq!(kind.max_children_saturating(50), 50); // capped
}

#[test]
fn max_children_saturating_integer_beyond_u128_saturates_to_cap() {
    use crate::native::bignum::BigUint;
    // A span wider than u128 makes `max_index().to_u128()` return `None`, so
    // the result saturates to `cap` instead of the astronomical exact count.
    let ic = IntegerChoice {
        min_value: BigInt::from(0),
        max_value: BigInt::from(BigUint::from(2u32).pow(200)),
        shrink_towards: BigInt::from(0),
    };
    let kind = ChoiceKind::Integer(ic);
    assert_eq!(kind.max_children_saturating(100), 100);
}

#[test]
fn max_children_saturating_float_matches_capped_exact() {
    use crate::native::bignum::ToPrimitive;
    let kind = ChoiceKind::Float(fc(0.0, 1.0, false, false));
    let exact = kind.max_children().to_u128().unwrap();
    assert_eq!(kind.max_children_saturating(u128::MAX), exact); // large cap: exact
    assert_eq!(kind.max_children_saturating(5), 5); // small cap: saturates
}

#[test]
fn max_children_saturating_bytes() {
    // min 0, max 2: 256^0 + 256^1 + 256^2 = 65793.
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 2,
    });
    assert_eq!(kind.max_children_saturating(u128::MAX), 65793);
    assert_eq!(kind.max_children_saturating(1000), 1000); // returns at the cap

    // min 2, max 3 skips the len-0 and len-1 terms: 256^2 + 256^3 = 16842752.
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 2,
        max_size: 3,
    });
    assert_eq!(kind.max_children_saturating(u128::MAX), 16_842_752);
}

#[test]
fn max_children_saturating_string() {
    // Alphabet 'a'..'z' = 26; min 0, max 2: 1 + 26 + 676 = 703.
    let kind = ChoiceKind::String(string_choice(vec![(b'a' as u32, b'z' as u32)], 0, 2));
    assert_eq!(kind.max_children_saturating(u128::MAX), 703);

    // A near-full Unicode alphabet makes `power` overflow u128 within the
    // length range, exercising the saturating multiply: the total pins at the
    // (here maximal) cap.
    let kind = ChoiceKind::String(string_choice(vec![(0, 0x10FFFF)], 0, 40));
    assert_eq!(kind.max_children_saturating(u128::MAX), u128::MAX);
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
    // shrink_towards = 0 sits 5 above the floor and 100 below the ceiling.
    let ic = integer_choice(-5, 100);
    for v in -5i128..=100 {
        let bv = BigInt::from(v);
        let idx = ic.to_index(&bv);
        assert_eq!(ic.from_index(idx), Some(bv), "round-trip failed for v={v}");
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
    // Range has 6 values (indices 0..=5); index 100 is past max.
    let big = crate::native::bignum::BigUint::from(100u32);
    assert_eq!(ic.from_index(big), None);
}

#[test]
fn integer_choice_from_index_overflowing_u128_returns_none() {
    // Even on the full i128 range, max_index is `u128::MAX` — any index
    // strictly larger than that has no valid value. The u128-native
    // implementation short-circuits via the `u128::try_from` step.
    let ic = integer_choice(i128::MIN, i128::MAX);
    let too_big = crate::native::bignum::BigUint::from(u128::MAX)
        + crate::native::bignum::BigUint::from(1u32);
    assert_eq!(ic.from_index(too_big), None);
}

#[test]
fn integer_choice_index_round_trip_nonzero_shrink_towards() {
    // shrink_towards is the index-0 anchor and biases the up/down interleave,
    // so exercise a non-zero one (inside range) across the full span.
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
    // shrink_towards below min clamps to min, making the choice one-sided.
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
    assert_eq!((ChoiceKind::Boolean(BooleanChoice)).max_children(), bu(2));
}

// ── FloatChoice ──────────────────────────────────────────────────────────

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
        smallest_nonzero_magnitude: 5e-324,
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
        smallest_nonzero_magnitude: 5e-324,
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
        smallest_nonzero_magnitude: 5e-324,
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
        smallest_nonzero_magnitude: 5e-324,
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

#[test]
fn float_choice_simplest_finds_simple_fraction_below_one() {
    // [0.1, 0.9] contains no integer; the float with the smallest lex index
    // in the range is 0.5. The legacy candidate probing never examined
    // magnitudes below 1.0, returned the 0.1 endpoint, and `to_index` then
    // underflowed (and panicked) for any simpler in-range value.
    assert_eq!(fc(0.1, 0.9, false, false).simplest(), 0.5);
    assert_eq!(fc(-0.9, -0.1, false, false).simplest(), -0.5);
}

#[test]
fn float_choice_simplest_is_exact_for_deep_fractions() {
    // [1.10, 1.11]: the minimum-lex float is 1.109375 (= 1 + 7/64), beyond
    // the eight mantissa encodings the legacy probe examined per exponent.
    assert_eq!(fc(1.10, 1.11, false, false).simplest(), 1.109375);
    assert_eq!(fc(0.3, 0.4, false, false).simplest(), 0.375);
}

#[test]
fn float_choice_to_index_does_not_underflow_for_fraction_only_ranges() {
    // Regression: rank(0.5) < rank(0.1), so with simplest() == 0.1 the
    // subtraction in to_index produced a negative BigUint and the engine
    // panicked mid-shrink for a plain `floats().min_value(0.1).max_value(0.9)`
    // failing test.
    let choice = fc(0.1, 0.9, false, false);
    for v in [0.1, 0.5, 0.9, 0.25, 0.125, 0.7] {
        let idx = choice.to_index(v);
        assert_eq!(choice.from_index(idx), Some(v));
    }
}

#[test]
fn float_choice_simplest_dominates_sampled_probes() {
    // simplest() must be valid and rank no higher than any other in-range
    // value; probe endpoints plus a spread of interior values.
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
        let s = choice.simplest();
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
    // Zero is out of range, and the magnitudes below MIN_POSITIVE are
    // excluded, so the search must start at the smallest allowed magnitude.
    let c = FloatChoice {
        min_value: 5e-324,
        max_value: 1.0,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: f64::MIN_POSITIVE,
    };
    assert_eq!(c.simplest(), 1.0);
    let c2 = FloatChoice {
        min_value: 5e-324,
        max_value: 1e-300,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: f64::MIN_POSITIVE,
    };
    let s = c2.simplest();
    assert!(
        c2.validate(s),
        "simplest {s} must respect the magnitude floor"
    );
}

#[test]
fn float_choice_nan_payloads_round_trip_through_index() {
    // Every NaN bit pattern (quiet or signaling, both signs) must round-trip
    // bit-exactly through to_index/from_index. The legacy encoding forced
    // mantissa bit 51 on decode, mangling signaling payloads.
    let choice = fc(f64::NEG_INFINITY, f64::INFINITY, true, true);
    for bits in [
        0x7FF8_0000_0000_0000_u64, // canonical quiet NaN
        0x7FF0_0000_0000_0001,     // signaling NaN, payload 1
        0xFFF8_0000_0000_0000,     // negative quiet NaN
        0xFFF0_0000_0000_0001,     // negative signaling NaN
        0x7FFF_FFFF_FFFF_FFFF,     // all payload bits set
    ] {
        let v = f64::from_bits(bits);
        let idx = choice.to_index(v);
        let back = choice.from_index(idx).unwrap();
        assert_eq!(back.to_bits(), bits, "NaN payload mangled");
    }
}

#[test]
fn float_choice_from_index_rejects_past_max_index() {
    // from_index must return None beyond max_index instead of aliasing
    // further NaN values.
    let choice = fc(f64::NEG_INFINITY, f64::INFINITY, true, true);
    assert!(choice.from_index(choice.max_index()).is_some());
    for extra in [1u32, 2, 50] {
        assert_eq!(
            choice.from_index(choice.max_index() + BigUint::from(extra)),
            None,
            "index past max_index must be rejected"
        );
    }
}

#[test]
fn float_choice_from_index_rejects_non_canonical_tag0_ranks() {
    // Magnitude indices in [2^56, 2^63) are non-canonical: the tag-0 decoder
    // ignores bits 56..62, so these ranks would silently alias small
    // integers (breaking the to_index/from_index inverse) instead of being
    // rejected.
    let choice = fc(f64::NEG_INFINITY, f64::INFINITY, true, true);
    // simplest is 0.0 (rank 0), so the index equals the global rank;
    // rank 2^57 has magnitude index 2^56.
    assert_eq!(choice.from_index(BigUint::from(1u128 << 57)), None);
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
        sc.from_index(crate::native::bignum::BigUint::from(1000u32))
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
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(min),
            max_value: BigInt::from(max),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn bytes_node(min: usize, max: usize, value: Vec<u8>) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Bytes(BytesChoice {
            min_size: min,
            max_size: max,
        }),
        ChoiceValue::Bytes(value),
        false,
    )
}

fn string_node(intervals: Vec<(u32, u32)>, min: usize, max: usize, value: Vec<u32>) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::String(string_choice(intervals, min, max)),
        ChoiceValue::String(value),
        false,
    )
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

// ── ChoiceKind::unit dispatch ────────────────────────────────────────────────

#[test]
fn choice_kind_unit_dispatches_to_each_sub_kind() {
    // `ChoiceKind::unit()` (used by data-tree simulation to predict punned
    // replays) must forward to the matching sub-kind's `unit()` for every
    // variant.
    let ic = IntegerChoice {
        min_value: BigInt::from(0),
        max_value: BigInt::from(10),
        shrink_towards: BigInt::from(0),
    };
    assert_eq!(
        ChoiceKind::Integer(ic.clone()).unit(),
        ChoiceValue::Integer(ic.unit())
    );

    assert_eq!(
        ChoiceKind::Boolean(BooleanChoice).unit(),
        ChoiceValue::Boolean(BooleanChoice.unit())
    );

    let fch = fc(0.0, 10.0, false, false);
    assert_eq!(
        ChoiceKind::Float(fch.clone()).unit(),
        ChoiceValue::Float(fch.unit())
    );

    let bc = BytesChoice {
        min_size: 0,
        max_size: 4,
    };
    assert_eq!(
        ChoiceKind::Bytes(bc.clone()).unit(),
        ChoiceValue::Bytes(bc.unit())
    );

    let sc = string_choice(vec![(b'a' as u32, b'z' as u32)], 0, 4);
    assert_eq!(
        ChoiceKind::String(sc.clone()).unit(),
        ChoiceValue::String(sc.unit())
    );
}

// ── EngineError Display ──────────────────────────────────────────────────────

#[test]
fn engine_error_display_covers_all_variants() {
    assert!(EngineError::Overrun.to_string().contains("Overrun"));
    assert!(EngineError::InvalidTestCase.to_string().contains("Invalid"));
    assert!(EngineError::AssumeViolation.to_string().contains("Assume"));
    assert_eq!(
        EngineError::InvalidArgument("nope".to_string()).to_string(),
        "nope"
    );
}

// ── NodeSortKey ordering for BigInt distances beyond u128 ────────────────────

fn big_integer_node(distance_beyond_u128: u32) -> ChoiceNode {
    let huge = BigInt::from(u128::MAX) * BigInt::from(4) + BigInt::from(distance_beyond_u128);
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(0),
            max_value: huge.clone(),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(huge),
        false,
    )
}

#[test]
fn node_sort_key_big_integer_orders_correctly() {
    use crate::native::core::sort_key;
    // A native integer node sorts before a BigInt node whose distance
    // exceeds u128.
    let scalar = vec![integer_node(0, 100, 50)];
    let big = vec![big_integer_node(0)];
    assert!(sort_key(&scalar) < sort_key(&big));
    // Two big-integer nodes order by magnitude.
    let big_small = vec![big_integer_node(0)];
    let big_large = vec![big_integer_node(7)];
    assert!(sort_key(&big_small) < sort_key(&big_large));
    // Scalar sort keys sort before any bytes sequence.
    let bytes = vec![ChoiceNode::new(
        ChoiceKind::Bytes(BytesChoice {
            min_size: 0,
            max_size: 4,
        }),
        ChoiceValue::Bytes(vec![1, 2, 3]),
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

// ── Bytes/String dispatch coverage ────────────────────────────────────
//
// The index-based shrink passes skip sequence kinds entirely, so the
// Bytes/String arms of the ChoiceKind dispatch methods need direct tests.

#[test]
fn bytes_max_index_and_max_children() {
    // lengths 0..=2 over 256 symbols: 1 + 256 + 65536 = 65793 values
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 2,
    });
    assert_eq!(kind.max_index(), bu(65792));
    assert_eq!(kind.max_children(), bu(65793));
}

#[test]
fn string_max_index_and_max_children() {
    // alphabet {a,b,c}, lengths 0..=2: 1 + 3 + 9 = 13 values
    let kind = ChoiceKind::String(StringChoice {
        intervals: crate::native::intervalsets::IntervalSet::new(vec![(b'a' as u32, b'c' as u32)]),
        min_size: 0,
        max_size: 2,
    });
    assert_eq!(kind.max_index(), bu(12));
    assert_eq!(kind.max_children(), bu(13));
}

#[test]
fn bytes_to_index_via_dispatch() {
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 4,
    });
    assert_eq!(kind.to_index(&ChoiceValue::Bytes(vec![])), bu(0));
    assert_eq!(kind.to_index(&ChoiceValue::Bytes(vec![0])), bu(1));
}

#[test]
fn string_to_index_via_dispatch() {
    let kind = ChoiceKind::String(StringChoice {
        intervals: crate::native::intervalsets::IntervalSet::new(vec![(b'a' as u32, b'c' as u32)]),
        min_size: 0,
        max_size: 4,
    });
    assert_eq!(kind.to_index(&ChoiceValue::String(vec![])), bu(0));
}

// ── ChoiceKind::random_value boolean entropy ────────────────────────────────

/// An unbiased boolean drawn via `random_value` must spend exactly one byte of
/// entropy (it routes through `weighted_boolean_sample(0.5, …)`), not a whole
/// `u32`. The urandom backend feeds every byte from the fuzzer, so a one-bit
/// decision must cost one byte. Regression for a bare `rng.random::<bool>()`.
#[test]
fn random_value_boolean_consumes_exactly_one_byte() {
    use crate::native::rng::EngineRng;
    use rand::Rng;

    let kind = ChoiceKind::Boolean(BooleanChoice);
    let mut a = EngineRng::seeded(2024);
    let mut b = EngineRng::seeded(2024);

    let value = kind.random_value(&mut a);
    let ChoiceValue::Boolean(got) = value else {
        panic!("expected a boolean choice value");
    };

    // Consume one byte from `b`; it is the same byte `a` drew.
    let mut byte = [0u8; 1];
    b.fill_bytes(&mut byte);
    // p = 0.5 → falsey = 128, so the boolean is `byte >= 128`.
    assert_eq!(got, u32::from(byte[0]) >= 128);
    // Exactly one byte was consumed: the two RNGs are now in lockstep.
    assert_eq!(a.next_u64(), b.next_u64());
}

#[test]
fn choice_value_equality_is_false_across_variants() {
    // `ChoiceValue`'s `PartialEq` only matches like-with-like; the cross-variant
    // fallback arm makes values of different kinds unequal even when they look
    // alike (e.g. integer 0, boolean false, float 0.0, empty bytes/string).
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
fn string_choice_simplest_on_empty_alphabet_is_an_internal_error() {
    // The schema layer must reject empty alphabets before a `StringChoice`
    // is built; reaching here with one is a hegel bug, surfaced as an
    // internal-error unwind rather than a shrinkable failure.
    let sc = string_choice(vec![], 0, 1);
    let payload =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sc.simplest_codepoint()))
            .unwrap_err();
    // Outside a test context the internal error panics directly with its
    // message (inside one it would unwind as an `InternalError` payload).
    let msg = payload.downcast_ref::<String>().unwrap();
    assert!(msg.contains("empty alphabet"), "{msg}");
    assert!(msg.contains("bug in hegel"), "{msg}");
}

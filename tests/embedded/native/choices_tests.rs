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

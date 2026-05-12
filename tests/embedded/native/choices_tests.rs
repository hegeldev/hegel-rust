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

// ── FloatChoice::simplest ───────────────────────────────────────────────────
//
// Ports of pbtkit/tests/test_floats.py::test_floats_simplest_positive_range,
// test_float_simplest_with_inf_bounds, test_float_simplest_tiny_range,
// test_float_simplest_subnormal_range, test_float_simplest_finds_power_of_two,
// test_float_negative_zero_simplest.









// ── N18.core_choices: FloatChoice::simplest fall-through paths ───────────
//
// When the min/max range admits no finite value, simplest() falls through
// past the boundary/integer/fraction searches. The remaining branches at
// choices.rs:279-288 select +/-Infinity (if permitted), NaN (if
// `allow_nan`), or panic. Existing tests only cover the finite-range
// happy paths; these exercise the pathological fall-through tail and the
// matching `unit()` NaN early-return at line 295-297.




// ── FloatChoice::validate ───────────────────────────────────────────────────
//
// Port of pbtkit/tests/test_floats.py::test_floats_validate_edge_cases.


// ── FloatChoice::sort_index ─────────────────────────────────────────────────
//
// Port of pbtkit/tests/test_floats.py::test_floats_sort_key_ordering. Rust's
// FloatChoice::sort_index returns `(magnitude_index, is_negative)`, which
// orders values as: smallest non-negative finite < larger non-negative finite
// < +inf < -inf < NaN. Simpler positive finites sort before more complex ones.


// ── FloatChoice::to_index regression ────────────────────────────────────────
//
// Regression for a failure surfaced by `tests/pbtkit/choice_index.rs`: a tiny
// non-integer-spanning range like [65672.5, 65673.0] picks `simplest = 65673.0`
// (the integer wins under native's Hypothesis-lex sort_key), but the original
// pbtkit-style raw-index implementation computed `to_index(value)` as
// `raw_idx(value) - raw_idx(simplest)` — which underflowed because in raw-idx
// terms 65672.5 < 65672.80222519021 < 65673.0. The fix is to base the index
// API on the same `sort_key` ordering used elsewhere in native.






// ── FloatChoice::unit ───────────────────────────────────────────────────────
//
// Port of pbtkit/tests/test_floats.py::test_float_choice_unit, adapted to the
// Rust implementation's (index, is_negative) ordering (the Python version
// uses (exponent_rank, mantissa, sign)).




// ── BytesChoice ───────────────────────────────────────────────────────────
//
// Ports of pbtkit/tests/test_bytes.py::test_bytes_choice_unit and related.








// ── StringChoice ──────────────────────────────────────────────────────────
//
// Ports of pbtkit/tests/test_text.py::test_string_* and related. Note that
// `StringChoice::simplest`/`unit` return codepoint sequences (`Vec<u32>`) —
// the `String` boundary lives one level up in `NativeTestCase::draw_string`.









// ── StringChoice::unit (single-codepoint alphabet) ────────────────────────
//
// Ports of pbtkit/tests/test_text.py::test_string_single_codepoint_unit.




// ── StringChoice index helpers ────────────────────────────────────────────
//
// Ports of pbtkit/tests/test_text.py::test_string_from_index_out_of_range,
// test_string_from_index_past_end, test_string_codepoint_rank_with_surrogates.








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

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

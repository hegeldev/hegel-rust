//! Ports of `compute_max_children` tests from
//! `hypothesis-python/tests/conjecture/test_utils.py` plus hegel-specific
//! checks for the choice kinds hegel's native engine actually records.

use crate::native::bignum::BigUint;
use crate::native::core::{BooleanChoice, ChoiceKind, IntegerChoice};

fn bu(n: u64) -> BigUint {
    BigUint::from(n)
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
    let expected = BigUint::from(u128::MAX) + bu(1);
    assert_eq!(kind.max_children(), expected);
}

#[test]
fn boolean_is_always_two() {
    assert_eq!(
        (ChoiceKind::Boolean(BooleanChoice)).max_children(),
        bu(2)
    );
}










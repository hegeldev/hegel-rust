//! Ports of `compute_max_children` tests from
//! `hypothesis-python/tests/conjecture/test_utils.py` plus hegel-specific
//! checks for the choice kinds hegel's native engine actually records.

use super::*;
use crate::native::bignum::BigUint;
use crate::native::core::{
    BooleanChoice, BytesChoice, ChoiceKind, FloatChoice, IntegerChoice, StringChoice,
};

fn bu(n: u64) -> BigUint {
    BigUint::from(n)
}

#[test]
fn integer_bounded_range_gives_exact_count() {
    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: 0,
        max_value: 200,
    });
    assert_eq!(compute_max_children(&kind), bu(201));
}

#[test]
fn integer_negative_range_gives_exact_count() {
    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: -10,
        max_value: 10,
    });
    assert_eq!(compute_max_children(&kind), bu(21));
}

#[test]
fn integer_full_i128_range_is_two_pow_128() {
    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: i128::MIN,
        max_value: i128::MAX,
    });
    // 2^128 = u128::MAX + 1.
    let expected = BigUint::from(u128::MAX) + bu(1);
    assert_eq!(compute_max_children(&kind), expected);
}

#[test]
fn boolean_is_always_two() {
    assert_eq!(
        compute_max_children(&ChoiceKind::Boolean(BooleanChoice)),
        bu(2)
    );
}

#[test]
fn float_unit_range_counts_bit_patterns() {
    // [0.0, 0.0] is a single bit pattern.
    let kind = ChoiceKind::Float(FloatChoice {
        min_value: 0.0,
        max_value: 0.0,
        allow_nan: false,
        allow_infinity: false,
    });
    assert_eq!(compute_max_children(&kind), bu(1));
}

#[test]
fn float_spans_zero_counts_both_signs() {
    // Spans 0; count_between_floats(-0.0, 0.0) == 2.
    let kind = ChoiceKind::Float(FloatChoice {
        min_value: -0.0,
        max_value: 0.0,
        allow_nan: false,
        allow_infinity: false,
    });
    assert_eq!(compute_max_children(&kind), bu(2));
}

#[test]
fn bytes_fixed_size_one_gives_256() {
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 1,
        max_size: 1,
    });
    assert_eq!(compute_max_children(&kind), bu(256));
}

#[test]
fn bytes_empty_range_is_one() {
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 0,
    });
    // Only the empty byte-string.
    assert_eq!(compute_max_children(&kind), bu(1));
}

#[test]
fn bytes_large_range_saturates_at_cap() {
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 1_000,
    });
    assert_eq!(
        compute_max_children(&kind),
        bu(MAX_CHILDREN_EFFECTIVELY_INFINITE)
    );
}

#[test]
fn string_single_codepoint_constant_alphabet_returns_length_range_plus_one() {
    // alphabet size 1 (single codepoint range with no surrogate overlap).
    let kind = ChoiceKind::String(StringChoice {
        min_codepoint: 'a' as u32,
        max_codepoint: 'a' as u32,
        min_size: 0,
        max_size: 10,
    });
    assert_eq!(compute_max_children(&kind), bu(11));
}

#[test]
fn string_empty_alphabet_is_one() {
    // Empty alphabet: min_codepoint > max_codepoint is disallowed, so
    // construct an alpha_size == 0 case via the whole surrogate block.
    let kind = ChoiceKind::String(StringChoice {
        min_codepoint: 0xD800,
        max_codepoint: 0xDFFF,
        min_size: 0,
        max_size: 5,
    });
    assert_eq!(compute_max_children(&kind), bu(1));
}

#[test]
fn string_abcd_alphabet_size_10_matches_geometric_series() {
    // Alphabet = {a, b, c, d}, sizes 0..=10 → sum_{k=0}^{10} 4^k.
    let kind = ChoiceKind::String(StringChoice {
        min_codepoint: 'a' as u32,
        max_codepoint: 'd' as u32,
        min_size: 0,
        max_size: 10,
    });
    // (4^11 - 1) / 3 == 1_398_101.
    assert_eq!(compute_max_children(&kind), bu(1_398_101));
}

#[test]
fn test_optimising_all_nodes_example_constraints_all_exceed_50() {
    // Regression check: the three `@example` rows of upstream's
    // `test_optimising_all_nodes` all satisfy the
    // `compute_max_children(node.type, node.constraints) > 50` assume.
    let bytes = ChoiceKind::Bytes(BytesChoice {
        min_size: 1,
        max_size: 1,
    });
    assert!(compute_max_children(&bytes) > bu(50));

    let string = ChoiceKind::String(StringChoice {
        min_codepoint: 'a' as u32,
        max_codepoint: 'd' as u32,
        min_size: 0,
        max_size: 10,
    });
    assert!(compute_max_children(&string) > bu(50));

    let integer = ChoiceKind::Integer(IntegerChoice {
        min_value: 0,
        max_value: 200,
    });
    assert!(compute_max_children(&integer) > bu(50));
}

//! Ported from hypothesis-python/tests/conjecture/test_choice.py
//!
//! Tests the choice-type surface of Hypothesis's engine: `compute_max_children`,
//! per-kind `to_index` / `from_index` inverse round-trips, `choice_permitted`
//! (Rust: `ChoiceKind::validate`), and replay via `for_choices` + `draw_*_forced`.
//!
//! Individually-skipped tests:
//!
//! - `test_compute_max_children_and_all_children_agree`,
//!   `test_compute_max_children_unbounded_integer_ranges`,
//!   `test_all_children_are_permitted_values`,
//!   `test_choice_to_index_injective`,
//!   `test_choice_from_value_injective` — require iterating every valid value
//!   for a `ChoiceKind` (`all_children` in upstream's `datatree.py`), which
//!   has no `src/native/` counterpart. `compute_max_children` is ported, but
//!   not the enumerator.
//! - `test_cannot_modify_forced_nodes` — asserts that calling
//!   `ChoiceNode.copy(with_value=…)` on a forced node raises
//!   `AssertionError`. Native `ChoiceNode::with_value` propagates
//!   `was_forced` unchanged rather than panicking. (The non-forced
//!   branch of `test_copy_choice_node` IS ported below.)
//! - `test_choice_node_equality` — asserts `node != 42` (cross-type). Rust
//!   `PartialEq` rejects mixed-type comparison at the type level, so this
//!   case is unrepresentable in Rust.
//! - Rows of `test_trivial_nodes` constrained by `shrink_towards` — native
//!   `IntegerChoice` has no `shrink_towards` field, so those rows don't
//!   port. The rows that depend only on `[min, max]` (plus one unbounded
//!   row using the full `i128` range as "bounded unbounded") are ported.
//!   Upstream also covers the `minimal(values()) == value` shrinking
//!   invariant in the same test; that half requires a shrinking/generator
//!   harness not built here, so only the `.trivial` half ports.
//! - `test_choice_node_is_hashable` — `ChoiceNode` does not implement
//!   `std::hash::Hash`.
//! - `test_choices_size_positive` — `choices_size([values])` (byte-width of
//!   a choice sequence) has no `src/native/` counterpart.
//! - `test_node_template_count`, `test_node_template_to_overrun`,
//!   `test_node_template_single_node_overruns`,
//!   `test_node_template_simplest_is_actually_trivial`,
//!   `test_node_template_overrun` — `ChoiceTemplate("simplest", count=n)` is
//!   a prefix primitive that tells `for_choices` to produce the `simplest`
//!   value of whatever kind is drawn for `n` steps. `NativeTestCase::for_choices`
//!   accepts only concrete `ChoiceValue`s; there is no template variant.
//! - Any `@example` of `test_compute_max_children` /
//!   `test_compute_max_children_is_positive` constrained by
//!   `smallest_nonzero_magnitude`, `weights`, or `shrink_towards` — native
//!   `FloatChoice` has no `smallest_nonzero_magnitude`, and native
//!   `IntegerChoice` has no `weights` / `shrink_towards`. Only the shape-
//!   compatible rows are ported.
//! - `test_choice_indices_are_positive` — trivially satisfied by Rust's type
//!   system: `ChoiceKind::{to,from}_index` returns `BigUint` (unsigned), so
//!   the non-negativity assertion is a tautology with no observable
//!   behaviour to check.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{
    BigUint, BooleanChoice, BytesChoice, ChoiceKind, ChoiceNode, ChoiceValue, FloatChoice,
    IntegerChoice, MAX_CHILDREN_EFFECTIVELY_INFINITE, NativeTestCase, Status, StringChoice,
    compute_max_children, next_down, next_up,
};
use rand::SeedableRng;
use rand::rngs::SmallRng;

/// Hypothesis's COLLECTION_DEFAULT_MAX_SIZE is 10^10; `BUFFER_SIZE` (8192) is
/// the native equivalent ceiling on a single test's choice count. We use a
/// large but representable constant where the test wants "unbounded-ish".
const LARGE_MAX_SIZE: usize = 10_000;

fn fresh() -> NativeTestCase {
    NativeTestCase::new_random(SmallRng::seed_from_u64(0))
}

// -- test_compute_max_children ------------------------------------------------
//
// One Rust test per upstream parametrize row that is expressible on the native
// `ChoiceKind` shapes (no `smallest_nonzero_magnitude` / `weights` /
// `shrink_towards`).

fn bu(n: u64) -> BigUint {
    BigUint::from(n)
}

#[test]
fn test_compute_max_children_string_empty_alphabet() {
    // Upstream: ("string", {min_size=0, max_size=100, intervals=""}, 1).
    // Native equivalent: alpha_size() == 0 via the surrogate block trick.
    let kind = ChoiceKind::String(StringChoice {
        min_codepoint: 0xD800,
        max_codepoint: 0xDFFF,
        min_size: 0,
        max_size: 100,
    });
    assert_eq!(compute_max_children(&kind), bu(1));
}

#[test]
fn test_compute_max_children_string_zero_length_nonempty_alphabet() {
    let kind = ChoiceKind::String(StringChoice {
        min_codepoint: b'a' as u32,
        max_codepoint: b'c' as u32,
        min_size: 0,
        max_size: 0,
    });
    assert_eq!(compute_max_children(&kind), bu(1));
}

#[test]
fn test_compute_max_children_string_fixed_length_three_letter_alphabet() {
    let kind = ChoiceKind::String(StringChoice {
        min_codepoint: b'a' as u32,
        max_codepoint: b'c' as u32,
        min_size: 8,
        max_size: 8,
    });
    assert_eq!(compute_max_children(&kind), bu(3u64.pow(8)));
}

#[test]
fn test_compute_max_children_string_range_four_letter_alphabet() {
    let kind = ChoiceKind::String(StringChoice {
        min_codepoint: b'a' as u32,
        max_codepoint: b'd' as u32,
        min_size: 2,
        max_size: 8,
    });
    let expected: u64 = (2..=8u32).map(|k| 4u64.pow(k)).sum();
    assert_eq!(compute_max_children(&kind), bu(expected));
}

#[test]
fn test_compute_max_children_string_large_range_saturates() {
    // Upstream uses max_size=10000 with alphabet="abcdefg"; native caps at
    // MAX_CHILDREN_EFFECTIVELY_INFINITE.
    let kind = ChoiceKind::String(StringChoice {
        min_codepoint: b'a' as u32,
        max_codepoint: b'g' as u32,
        min_size: 0,
        max_size: 10_000,
    });
    assert_eq!(
        compute_max_children(&kind),
        bu(MAX_CHILDREN_EFFECTIVELY_INFINITE)
    );
}

#[test]
fn test_compute_max_children_bytes_zero_to_two() {
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 2,
    });
    let expected: u64 = (0..=2u32).map(|k| 256u64.pow(k)).sum();
    assert_eq!(compute_max_children(&kind), bu(expected));
}

#[test]
fn test_compute_max_children_bytes_large_saturates() {
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 10_000,
    });
    assert_eq!(
        compute_max_children(&kind),
        bu(MAX_CHILDREN_EFFECTIVELY_INFINITE)
    );
}

#[test]
fn test_compute_max_children_boolean() {
    // Native `BooleanChoice` carries no `p`, so the Python rows p=0.0 / p=1.0
    // (which collapse to 1) are unrepresentable. The p=0.5 / p=0.001 / p=0.999
    // rows all collapse to "two values", which is what native reports.
    assert_eq!(
        compute_max_children(&ChoiceKind::Boolean(BooleanChoice)),
        bu(2)
    );
}

#[test]
fn test_compute_max_children_float_zero_zero() {
    let kind = ChoiceKind::Float(FloatChoice {
        min_value: 0.0,
        max_value: 0.0,
        allow_nan: false,
        allow_infinity: false,
    });
    assert_eq!(compute_max_children(&kind), bu(1));
}

#[test]
fn test_compute_max_children_float_neg_zero_neg_zero() {
    let kind = ChoiceKind::Float(FloatChoice {
        min_value: -0.0,
        max_value: -0.0,
        allow_nan: false,
        allow_infinity: false,
    });
    assert_eq!(compute_max_children(&kind), bu(1));
}

#[test]
fn test_compute_max_children_float_neg_zero_to_zero() {
    let kind = ChoiceKind::Float(FloatChoice {
        min_value: -0.0,
        max_value: 0.0,
        allow_nan: false,
        allow_infinity: false,
    });
    assert_eq!(compute_max_children(&kind), bu(2));
}

#[test]
fn test_compute_max_children_float_next_down_to_next_up() {
    let kind = ChoiceKind::Float(FloatChoice {
        min_value: next_down(-0.0),
        max_value: next_up(0.0),
        allow_nan: false,
        allow_infinity: false,
    });
    assert_eq!(compute_max_children(&kind), bu(4));
}

// -- test_compute_max_children_is_positive -------------------------------------
//
// Upstream has a `@given(choice_types_constraints())` PBT plus three
// `@example` rows using integer bounds of magnitude 2**200. Native
// `IntegerChoice` is `i128`-bounded, so only the in-range shapes port.

#[test]
fn test_compute_max_children_is_positive_integer() {
    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: i128::MIN,
        max_value: i128::MAX,
    });
    assert!(compute_max_children(&kind) >= bu(0));
}

#[test]
fn test_compute_max_children_is_positive_boolean() {
    assert!(compute_max_children(&ChoiceKind::Boolean(BooleanChoice)) >= bu(0));
}

#[test]
fn test_compute_max_children_is_positive_bytes_unbounded() {
    let kind = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: LARGE_MAX_SIZE,
    });
    assert!(compute_max_children(&kind) >= bu(0));
}

#[test]
fn test_compute_max_children_is_positive_string_full_unicode() {
    let kind = ChoiceKind::String(StringChoice {
        min_codepoint: 0,
        max_codepoint: 0x10FFFF,
        min_size: 0,
        max_size: LARGE_MAX_SIZE,
    });
    assert!(compute_max_children(&kind) >= bu(0));
}

// -- test_draw_string_single_interval_with_equal_bounds ----------------------
//
// Upstream: `data.draw_string(intervals, min_size=n, max_size=n) == s * n`
// where `s` is a single-character string and `n` a length. Native
// `draw_string` takes a codepoint range, not an IntervalSet, so we port
// by choosing single-codepoint ranges (`min_codepoint == max_codepoint`).

fn string_single_interval(cp: u32, n: usize) {
    let expected: String = std::iter::repeat_n(char::from_u32(cp).unwrap(), n).collect();
    let mut data = fresh();
    let drawn = data.draw_string(cp, cp, n, n).ok().unwrap();
    assert_eq!(drawn, expected);
}

#[test]
fn test_draw_string_single_interval_with_equal_bounds_a_zero() {
    string_single_interval(b'a' as u32, 0);
}

#[test]
fn test_draw_string_single_interval_with_equal_bounds_a_five() {
    string_single_interval(b'a' as u32, 5);
}

#[test]
fn test_draw_string_single_interval_with_equal_bounds_q_fifty() {
    string_single_interval(b'q' as u32, 50);
}

#[test]
fn test_draw_string_single_interval_with_equal_bounds_hiragana_seven() {
    // Non-ASCII codepoint (HIRAGANA LETTER A).
    string_single_interval(0x3042, 7);
}

// -- test_nodes ---------------------------------------------------------------
//
// Upstream makes a sequence of forced draws and asserts that `data.nodes`
// matches an explicit list of `ChoiceNode` values. `NativeTestCase` exposes
// `nodes` publicly; upstream's `start_span(42) / stop_span()` pair around
// two of the draws is incidental to the node-list assertion (spans are a
// separate tree) so we port without it.

#[test]
fn test_nodes() {
    let mut data = fresh();
    data.draw_float_forced(-10.0, 10.0, true, true, 5.0)
        .ok()
        .unwrap();
    data.weighted(0.5, Some(true)).ok().unwrap();
    data.draw_string_forced(b'a' as u32, b'd' as u32, 0, LARGE_MAX_SIZE, "abbcccdddd")
        .ok()
        .unwrap();
    data.draw_bytes_forced(8, 8, vec![0u8; 8]).ok().unwrap();
    data.draw_integer_forced(0, 100, 50).ok().unwrap();

    let expected = vec![
        ChoiceNode {
            kind: ChoiceKind::Float(FloatChoice {
                min_value: -10.0,
                max_value: 10.0,
                allow_nan: true,
                allow_infinity: true,
            }),
            value: ChoiceValue::Float(5.0),
            was_forced: true,
        },
        ChoiceNode {
            kind: ChoiceKind::Boolean(BooleanChoice),
            value: ChoiceValue::Boolean(true),
            was_forced: true,
        },
        ChoiceNode {
            kind: ChoiceKind::String(StringChoice {
                min_codepoint: b'a' as u32,
                max_codepoint: b'd' as u32,
                min_size: 0,
                max_size: LARGE_MAX_SIZE,
            }),
            value: ChoiceValue::String("abbcccdddd".chars().map(|c| c as u32).collect()),
            was_forced: true,
        },
        ChoiceNode {
            kind: ChoiceKind::Bytes(BytesChoice {
                min_size: 8,
                max_size: 8,
            }),
            value: ChoiceValue::Bytes(vec![0u8; 8]),
            was_forced: true,
        },
        ChoiceNode {
            kind: ChoiceKind::Integer(IntegerChoice {
                min_value: 0,
                max_value: 100,
            }),
            value: ChoiceValue::Integer(50),
            was_forced: true,
        },
    ];
    assert_eq!(data.nodes, expected);
}

// -- test_data_with_empty_choices_is_overrun ----------------------------------
//
// Upstream asserts that drawing any choice from an empty prefix raises
// `StopTest` and leaves `data.status is Status.OVERRUN`. Native has no
// distinct `Overrun` variant; the closest is `Status::EarlyStop`, which
// `pre_choice` sets on this exact path.

#[test]
fn test_data_with_empty_choices_is_overrun() {
    let mut data = NativeTestCase::for_choices(&[], None);
    assert!(data.draw_integer(0, 100).is_err());
    assert_eq!(data.status, Some(Status::EarlyStop));
}

// -- test_data_with_changed_forced_value --------------------------------------
//
// When a prefix node says `v1` but `draw_*_forced` asks for `v2 != v1`, the
// draw must return `v2` and record a forced node with value `v2`.

#[test]
fn test_data_with_changed_forced_value_integer() {
    let mut data = NativeTestCase::for_choices(&[ChoiceValue::Integer(10)], None);
    assert_eq!(data.draw_integer_forced(0, 100, 42).ok().unwrap(), 42);
}

#[test]
fn test_data_with_changed_forced_value_bytes() {
    let mut data = NativeTestCase::for_choices(&[ChoiceValue::Bytes(b"aaa".to_vec())], None);
    assert_eq!(
        data.draw_bytes_forced(0, 16, b"hello".to_vec())
            .ok()
            .unwrap(),
        b"hello".to_vec()
    );
}

#[test]
fn test_data_with_changed_forced_value_float() {
    let mut data = NativeTestCase::for_choices(&[ChoiceValue::Float(0.0)], None);
    let drawn = data
        .draw_float_forced(f64::NEG_INFINITY, f64::INFINITY, true, true, 3.5)
        .ok()
        .unwrap();
    assert_eq!(drawn, 3.5);
}

#[test]
fn test_data_with_changed_forced_value_string() {
    let mut data = NativeTestCase::for_choices(&[ChoiceValue::String(vec![b'a' as u32])], None);
    let drawn = data
        .draw_string_forced(0, 0x10FFFF, 0, 16, "world")
        .ok()
        .unwrap();
    assert_eq!(drawn, "world");
}

// -- test_data_with_same_forced_value_is_valid --------------------------------
//
// The prefix and forced value agree. No surprise — the draw still records a
// forced node.

#[test]
fn test_data_with_same_forced_value_is_valid_integer() {
    let mut data = NativeTestCase::for_choices(&[ChoiceValue::Integer(50)], None);
    assert_eq!(data.draw_integer_forced(0, 100, 50).ok().unwrap(), 50);
}

#[test]
fn test_data_with_same_forced_value_is_valid_bytes() {
    let mut data = NativeTestCase::for_choices(&[ChoiceValue::Bytes(vec![0u8; 8])], None);
    assert_eq!(
        data.draw_bytes_forced(8, 8, vec![0u8; 8]).ok().unwrap(),
        vec![0u8; 8]
    );
}

#[test]
fn test_data_with_same_forced_value_is_valid_float() {
    let mut data = NativeTestCase::for_choices(&[ChoiceValue::Float(0.0)], None);
    let drawn = data
        .draw_float_forced(f64::NEG_INFINITY, f64::INFINITY, true, true, 0.0)
        .ok()
        .unwrap();
    assert_eq!(drawn.to_bits(), 0.0_f64.to_bits());
}

// -- test_choice_permitted ----------------------------------------------------
//
// Upstream's `choice_permitted(value, constraints)` == Rust
// `ChoiceKind::validate(&ChoiceValue)`. One test per parametrize row (skipping
// the ones that depend on native-absent fields like allow_nan=False or
// smallest_nonzero_magnitude).

fn permitted(kind: ChoiceKind, v: ChoiceValue) -> bool {
    kind.validate(&v)
}

#[test]
fn test_choice_permitted_integer_below_range() {
    let k = ChoiceKind::Integer(IntegerChoice {
        min_value: 1,
        max_value: 2,
    });
    assert!(!permitted(k, ChoiceValue::Integer(0)));
}

#[test]
fn test_choice_permitted_integer_above_range() {
    let k = ChoiceKind::Integer(IntegerChoice {
        min_value: 0,
        max_value: 1,
    });
    assert!(!permitted(k, ChoiceValue::Integer(2)));
}

#[test]
fn test_choice_permitted_integer_in_range() {
    let k = ChoiceKind::Integer(IntegerChoice {
        min_value: 0,
        max_value: 20,
    });
    assert!(permitted(k, ChoiceValue::Integer(10)));
}

#[test]
fn test_choice_permitted_integer_full_i128_range() {
    // Upstream tests huge-magnitude values (2**128/2). Native's range is
    // i128-bounded; use the equivalent boundary values.
    let k = ChoiceKind::Integer(IntegerChoice {
        min_value: i128::MIN,
        max_value: i128::MAX,
    });
    assert!(permitted(k, ChoiceValue::Integer(i128::MAX)));
}

#[test]
fn test_choice_permitted_float_nan_with_allow_nan_default() {
    // Native FloatChoice has no allow_nan=False slot on the native
    // "allow_nan=True" path; we cover both branches.
    let k = ChoiceKind::Float(FloatChoice {
        min_value: 0.0,
        max_value: 0.0,
        allow_nan: true,
        allow_infinity: false,
    });
    assert!(permitted(k, ChoiceValue::Float(f64::NAN)));
}

#[test]
fn test_choice_permitted_float_nan_with_allow_nan_false() {
    let k = ChoiceKind::Float(FloatChoice {
        min_value: 0.0,
        max_value: 0.0,
        allow_nan: false,
        allow_infinity: false,
    });
    assert!(!permitted(k, ChoiceValue::Float(f64::NAN)));
}

#[test]
fn test_choice_permitted_float_unit_range() {
    let k = ChoiceKind::Float(FloatChoice {
        min_value: 1.0,
        max_value: 1.0,
        allow_nan: false,
        allow_infinity: false,
    });
    assert!(permitted(k, ChoiceValue::Float(1.0)));
}

#[test]
fn test_choice_permitted_string_too_long() {
    let k = ChoiceKind::String(StringChoice {
        min_codepoint: b'a' as u32,
        max_codepoint: b'd' as u32,
        min_size: 1,
        max_size: 3,
    });
    assert!(!permitted(
        k,
        ChoiceValue::String("abcd".chars().map(|c| c as u32).collect())
    ));
}

#[test]
fn test_choice_permitted_string_out_of_alphabet() {
    let k = ChoiceKind::String(StringChoice {
        min_codepoint: b'e' as u32,
        max_codepoint: b'e' as u32,
        min_size: 1,
        max_size: 10,
    });
    assert!(!permitted(
        k,
        ChoiceValue::String("abcd".chars().map(|c| c as u32).collect())
    ));
}

#[test]
fn test_choice_permitted_string_in_alphabet() {
    let k = ChoiceKind::String(StringChoice {
        min_codepoint: b'e' as u32,
        max_codepoint: b'e' as u32,
        min_size: 1,
        max_size: 10,
    });
    assert!(permitted(
        k,
        ChoiceValue::String("e".chars().map(|c| c as u32).collect())
    ));
}

#[test]
fn test_choice_permitted_bytes_too_short() {
    let k = ChoiceKind::Bytes(BytesChoice {
        min_size: 2,
        max_size: 2,
    });
    assert!(!permitted(k, ChoiceValue::Bytes(b"a".to_vec())));
}

#[test]
fn test_choice_permitted_bytes_exact_length() {
    let k = ChoiceKind::Bytes(BytesChoice {
        min_size: 2,
        max_size: 2,
    });
    assert!(permitted(k, ChoiceValue::Bytes(b"aa".to_vec())));
}

#[test]
fn test_choice_permitted_bytes_within_range() {
    let k = ChoiceKind::Bytes(BytesChoice {
        min_size: 0,
        max_size: 3,
    });
    assert!(permitted(k, ChoiceValue::Bytes(b"aa".to_vec())));
}

#[test]
fn test_choice_permitted_bytes_too_short_upper_range() {
    let k = ChoiceKind::Bytes(BytesChoice {
        min_size: 2,
        max_size: 10,
    });
    assert!(!permitted(k, ChoiceValue::Bytes(b"a".to_vec())));
}

// `test_choice_permitted` rows parameterized on booleans (p=0 / p=1 / p=0.5)
// aren't expressible on native `BooleanChoice` — it carries no `p` parameter;
// `validate` is a kind/value-type check only.

// -- test_shrink_towards_has_index_0 ------------------------------------------
//
// Native `IntegerChoice` has no `shrink_towards`; the upstream invariant
// degenerates to `to_index(simplest()) == 0` and `from_index(0) == simplest()`,
// which is the sort-order anchor every other index test depends on.

#[test]
fn test_simplest_has_index_0_integer() {
    let ic = IntegerChoice {
        min_value: -10,
        max_value: 10,
    };
    assert_eq!(ic.to_index(ic.simplest()), bu(0));
    assert_eq!(ic.from_index(bu(0)), Some(ic.simplest()));
}

#[test]
fn test_simplest_has_index_0_integer_positive_range() {
    let ic = IntegerChoice {
        min_value: 5,
        max_value: 100,
    };
    assert_eq!(ic.to_index(ic.simplest()), bu(0));
    assert_eq!(ic.from_index(bu(0)), Some(ic.simplest()));
}

#[test]
fn test_simplest_has_index_0_integer_negative_range() {
    let ic = IntegerChoice {
        min_value: -100,
        max_value: -5,
    };
    assert_eq!(ic.to_index(ic.simplest()), bu(0));
    assert_eq!(ic.from_index(bu(0)), Some(ic.simplest()));
}

// -- test_choice_index_and_value_are_inverses_explicit -------------------------
//
// Round-trip `to_index(x) -> from_index -> x` for explicit values of each kind.

#[test]
fn test_choice_index_inverses_boolean_p_effectively_zero() {
    // Native BooleanChoice doesn't carry `p`, but the invariant is
    // to_index(false) = 0 and from_index(0) = false regardless.
    let bc = BooleanChoice;
    assert_eq!(bc.to_index(false), bu(0));
    assert_eq!(bc.from_index(bu(0)), Some(false));
}

#[test]
fn test_choice_index_inverses_boolean_p_effectively_one() {
    let bc = BooleanChoice;
    assert_eq!(bc.to_index(true), bu(1));
    assert_eq!(bc.from_index(bu(1)), Some(true));
}

#[test]
fn test_choice_index_inverses_integer_min_only() {
    // Upstream: integer_constr(min_value=1, shrink_towards=4), range(1, 10).
    // Native has no shrink_towards, so we use bounded [1, 9] with simplest=1.
    let ic = IntegerChoice {
        min_value: 1,
        max_value: 9,
    };
    for v in 1..=9i128 {
        let idx = ic.to_index(v);
        assert_eq!(ic.from_index(idx), Some(v));
    }
}

#[test]
fn test_choice_index_inverses_integer_negative_to_positive() {
    let ic = IntegerChoice {
        min_value: -10,
        max_value: 5,
    };
    for v in -10..=5i128 {
        let idx = ic.to_index(v);
        assert_eq!(ic.from_index(idx), Some(v));
    }
}

#[test]
fn test_choice_index_inverses_float_three_neighbours() {
    let fc = FloatChoice {
        min_value: 1.0,
        max_value: next_up(next_up(1.0)),
        allow_nan: false,
        allow_infinity: false,
    };
    for &v in &[1.0_f64, next_up(1.0), next_up(next_up(1.0))] {
        let idx = fc.to_index(v);
        assert_eq!(fc.from_index(idx).map(f64::to_bits), Some(v.to_bits()));
    }
}

#[test]
fn test_choice_index_inverses_float_signed_zero_neighbours() {
    let fc = FloatChoice {
        min_value: next_down(-0.0),
        max_value: next_up(0.0),
        allow_nan: false,
        allow_infinity: false,
    };
    for &v in &[next_down(-0.0), -0.0_f64, 0.0_f64, next_up(0.0)] {
        let idx = fc.to_index(v);
        assert_eq!(fc.from_index(idx).map(f64::to_bits), Some(v.to_bits()));
    }
}

// -- test_integer_choice_index ------------------------------------------------
//
// The explicit integer `choices[i]` sequence tests below verify that
// `to_index` matches the expected shrink-order enumeration. Native has no
// `shrink_towards` and anchors simplest on zero (or the range endpoint
// closest to zero), so we port the rows that depend only on `[min, max]`.

fn assert_integer_choice_index(min_value: i128, max_value: i128, choices: &[i128]) {
    let ic = IntegerChoice {
        min_value,
        max_value,
    };
    for (i, &c) in choices.iter().enumerate() {
        assert_eq!(
            ic.to_index(c),
            BigUint::from(i as u64),
            "to_index({c}) should be {i}"
        );
    }
}

#[test]
fn test_integer_choice_index_bounded_spans_zero() {
    // Upstream: integer_constr(-3, 3), (0, 1, -1, 2, -2, 3, -3)
    assert_integer_choice_index(-3, 3, &[0, 1, -1, 2, -2, 3, -3]);
}

#[test]
fn test_integer_choice_index_unbounded_spans_zero() {
    // Upstream uses `integer_constr()` (unbounded). Native has no unbounded
    // shape; the bounded equivalent preserves the ordering check.
    assert_integer_choice_index(-1000, 1000, &[0, 1, -1, 2, -2, 3, -3]);
}

#[test]
fn test_integer_choice_index_semibounded_below() {
    // Upstream: integer_constr(min_value=3), (3, 4, 5, 6, 7). Native uses a
    // concrete upper bound; the order is the same up to that bound.
    assert_integer_choice_index(3, 1000, &[3, 4, 5, 6, 7]);
}

#[test]
fn test_integer_choice_index_semibounded_above() {
    // Upstream: integer_constr(max_value=-3), (-3, -4, -5, -6, -7).
    assert_integer_choice_index(-1000, -3, &[-3, -4, -5, -6, -7]);
}

// -- test_drawing_directly_matches_for_choices --------------------------------
//
// Upstream PBTs a list of nodes: for each, `draw_{type}(**constraints)` from
// a `for_choices` prefix returns the node's value. Ported as explicit rows
// because the PBT flavour needs nodes-from-strategy infrastructure we don't
// have.

#[test]
fn test_drawing_directly_matches_for_choices_integer() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Integer(42)], None);
    assert_eq!(d.draw_integer(-100, 100).ok().unwrap(), 42);
}

#[test]
fn test_drawing_directly_matches_for_choices_bytes() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Bytes(b"hello".to_vec())], None);
    assert_eq!(d.draw_bytes(0, 16).ok().unwrap(), b"hello".to_vec());
}

#[test]
fn test_drawing_directly_matches_for_choices_string() {
    let mut d = NativeTestCase::for_choices(
        &[ChoiceValue::String(
            "abc".chars().map(|c| c as u32).collect(),
        )],
        None,
    );
    assert_eq!(d.draw_string(0, 0x10FFFF, 0, 16).ok().unwrap(), "abc");
}

#[test]
fn test_drawing_directly_matches_for_choices_float() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Float(1.5)], None);
    assert_eq!(d.draw_float(0.0, 10.0, false, false).ok().unwrap(), 1.5);
}

#[test]
fn test_drawing_directly_matches_for_choices_multiple() {
    // Sequence draw: integer, bytes, float, in order.
    let mut d = NativeTestCase::for_choices(
        &[
            ChoiceValue::Integer(7),
            ChoiceValue::Bytes(b"xy".to_vec()),
            ChoiceValue::Float(2.5),
        ],
        None,
    );
    assert_eq!(d.draw_integer(0, 100).ok().unwrap(), 7);
    assert_eq!(d.draw_bytes(0, 16).ok().unwrap(), b"xy".to_vec());
    assert_eq!(d.draw_float(0.0, 10.0, false, false).ok().unwrap(), 2.5);
}

// -- test_draw_directly_explicit ----------------------------------------------
//
// Each assertion in upstream's `test_draw_directly_explicit` as a separate
// Rust test.

#[test]
fn test_draw_directly_explicit_string() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::String(vec![b'a' as u32])], None);
    assert_eq!(d.draw_string(0, 127, 1, LARGE_MAX_SIZE).ok().unwrap(), "a");
}

#[test]
fn test_draw_directly_explicit_bytes() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Bytes(b"a".to_vec())], None);
    assert_eq!(d.draw_bytes(0, LARGE_MAX_SIZE).ok().unwrap(), b"a".to_vec());
}

#[test]
fn test_draw_directly_explicit_float() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Float(1.0)], None);
    assert_eq!(d.draw_float(0.0, 2.0, false, true).ok().unwrap(), 1.0);
}

#[test]
fn test_draw_directly_explicit_boolean() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Boolean(true)], None);
    assert!(d.weighted(0.3, None).ok().unwrap());
}

#[test]
fn test_draw_directly_explicit_integer_unbounded() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Integer(42)], None);
    assert_eq!(d.draw_integer(i128::MIN, i128::MAX).ok().unwrap(), 42);
}

#[test]
fn test_draw_directly_explicit_integer_bounded() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Integer(-42)], None);
    assert_eq!(d.draw_integer(-50, 0).ok().unwrap(), -42);
}

// `integer(10, 11, weights={10: 0.1, 11: 0.3})` isn't expressible on native
// (no weights), but the forced-in-prefix invariant is the same:
#[test]
fn test_draw_directly_explicit_integer_small_range() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Integer(10)], None);
    assert_eq!(d.draw_integer(10, 11).ok().unwrap(), 10);
}

// -- test_choices_key_distinguishes_weird_cases -------------------------------
//
// Upstream uses `choices_key([v])` on tuples like `(True,)` and `(1,)` to
// check that dedup doesn't collapse bool `True` and int `1`. Native
// `ChoiceValue` is an enum whose `PartialEq` is branch-per-variant, so
// `Vec<ChoiceValue>` distinguishes these naturally — there is no separate
// `choices_key` to port.

#[test]
fn test_choices_key_distinguishes_true_and_integer_one() {
    let a = vec![ChoiceValue::Boolean(true)];
    let b = vec![ChoiceValue::Integer(1)];
    assert_ne!(a, b);
}

#[test]
fn test_choices_key_distinguishes_true_and_float_one() {
    let a = vec![ChoiceValue::Boolean(true)];
    let b = vec![ChoiceValue::Float(1.0)];
    assert_ne!(a, b);
}

#[test]
fn test_choices_key_distinguishes_false_and_integer_zero() {
    let a = vec![ChoiceValue::Boolean(false)];
    let b = vec![ChoiceValue::Integer(0)];
    assert_ne!(a, b);
}

#[test]
fn test_choices_key_distinguishes_false_and_float_zero() {
    let a = vec![ChoiceValue::Boolean(false)];
    let b = vec![ChoiceValue::Float(0.0)];
    assert_ne!(a, b);
}

#[test]
fn test_choices_key_distinguishes_false_and_neg_zero_float() {
    let a = vec![ChoiceValue::Boolean(false)];
    let b = vec![ChoiceValue::Float(-0.0)];
    assert_ne!(a, b);
}

#[test]
fn test_choices_key_distinguishes_pos_zero_and_neg_zero_float() {
    // `ChoiceValue::Float` compares by bitwise equality, so +0.0 and -0.0
    // (different bit patterns) are distinct keys.
    let a = vec![ChoiceValue::Float(0.0)];
    let b = vec![ChoiceValue::Float(-0.0)];
    assert_ne!(a, b);
}

// -- test_copy_choice_node ----------------------------------------------------
//
// Upstream: for a non-forced node, `copy(with_value=v) == node` iff
// `v == node.value`. Ported as explicit rows per kind. (The forced-node
// branch — upstream's `test_cannot_modify_forced_nodes`, which asserts
// that copying a forced node raises — doesn't port; native
// `ChoiceNode::with_value` propagates `was_forced` unchanged.)

fn assert_copy_choice_node(kind: ChoiceKind, value: ChoiceValue, other: ChoiceValue) {
    let node = ChoiceNode {
        kind,
        value: value.clone(),
        was_forced: false,
    };
    assert_eq!(node.with_value(value), node);
    assert_ne!(node.with_value(other), node);
}

#[test]
fn test_copy_choice_node_integer() {
    assert_copy_choice_node(
        ChoiceKind::Integer(IntegerChoice {
            min_value: -10,
            max_value: 10,
        }),
        ChoiceValue::Integer(5),
        ChoiceValue::Integer(-3),
    );
}

#[test]
fn test_copy_choice_node_boolean() {
    assert_copy_choice_node(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(true),
        ChoiceValue::Boolean(false),
    );
}

#[test]
fn test_copy_choice_node_float() {
    assert_copy_choice_node(
        ChoiceKind::Float(FloatChoice {
            min_value: -10.0,
            max_value: 10.0,
            allow_nan: false,
            allow_infinity: false,
        }),
        ChoiceValue::Float(1.5),
        ChoiceValue::Float(-2.25),
    );
}

#[test]
fn test_copy_choice_node_bytes() {
    assert_copy_choice_node(
        ChoiceKind::Bytes(BytesChoice {
            min_size: 0,
            max_size: 16,
        }),
        ChoiceValue::Bytes(b"abc".to_vec()),
        ChoiceValue::Bytes(b"xyz".to_vec()),
    );
}

#[test]
fn test_copy_choice_node_string() {
    assert_copy_choice_node(
        ChoiceKind::String(StringChoice {
            min_codepoint: b'a' as u32,
            max_codepoint: b'z' as u32,
            min_size: 0,
            max_size: 16,
        }),
        ChoiceValue::String("abc".chars().map(|c| c as u32).collect()),
        ChoiceValue::String("xyz".chars().map(|c| c as u32).collect()),
    );
}

// -- test_forced_nodes_are_trivial / test_trivial_nodes /
//    test_nontrivial_nodes / test_conservative_nontrivial_nodes -------------
//
// `ChoiceNode.trivial` is ported to native; these tests exercise the
// `.trivial` half of the upstream tests. The `minimal(values()) ==
// node.value` shrinking invariant that accompanies each row in upstream
// is NOT ported — it needs a shrinking/generator harness we don't have
// here.

fn node(kind: ChoiceKind, value: ChoiceValue, was_forced: bool) -> ChoiceNode {
    ChoiceNode {
        kind,
        value,
        was_forced,
    }
}

#[test]
fn test_forced_nodes_are_trivial_integer() {
    // Any forced node is trivial regardless of value/constraints.
    assert!(
        node(
            ChoiceKind::Integer(IntegerChoice {
                min_value: -10,
                max_value: 10,
            }),
            ChoiceValue::Integer(7),
            true,
        )
        .trivial()
    );
}

#[test]
fn test_forced_nodes_are_trivial_float_unbounded() {
    assert!(
        node(
            ChoiceKind::Float(FloatChoice {
                min_value: f64::NEG_INFINITY,
                max_value: f64::INFINITY,
                allow_nan: true,
                allow_infinity: true,
            }),
            ChoiceValue::Float(12345.0),
            true,
        )
        .trivial()
    );
}

// test_trivial_nodes rows (value is trivial). Shrink_towards-constrained
// rows dropped; boolean rows that depend on `p` dropped.

#[test]
fn test_trivial_nodes_float_integer_in_interval() {
    assert!(
        node(
            ChoiceKind::Float(FloatChoice {
                min_value: 5.0,
                max_value: 10.0,
                allow_nan: false,
                allow_infinity: false,
            }),
            ChoiceValue::Float(5.0),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_trivial_nodes_float_zero_in_interval() {
    assert!(
        node(
            ChoiceKind::Float(FloatChoice {
                min_value: -5.0,
                max_value: 5.0,
                allow_nan: false,
                allow_infinity: false,
            }),
            ChoiceValue::Float(0.0),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_trivial_nodes_float_unbounded_zero() {
    assert!(
        node(
            ChoiceKind::Float(FloatChoice {
                min_value: f64::NEG_INFINITY,
                max_value: f64::INFINITY,
                allow_nan: true,
                allow_infinity: true,
            }),
            ChoiceValue::Float(0.0),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_trivial_nodes_boolean_false() {
    assert!(
        node(
            ChoiceKind::Boolean(BooleanChoice),
            ChoiceValue::Boolean(false),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_trivial_nodes_string_empty() {
    assert!(
        node(
            ChoiceKind::String(StringChoice {
                min_codepoint: b'a' as u32,
                max_codepoint: b'd' as u32,
                min_size: 0,
                max_size: LARGE_MAX_SIZE,
            }),
            ChoiceValue::String(vec![]),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_trivial_nodes_string_all_simplest_codepoints() {
    // Alphabet b-d-a: simplest codepoint is 'a' (remapped to key 0 under
    // codepoint_key's digits/letters reorder). min_size=4 forces length 4.
    assert!(
        node(
            ChoiceKind::String(StringChoice {
                min_codepoint: b'a' as u32,
                max_codepoint: b'd' as u32,
                min_size: 4,
                max_size: LARGE_MAX_SIZE,
            }),
            ChoiceValue::String(vec![b'a' as u32; 4]),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_trivial_nodes_bytes_zeroed_fixed() {
    assert!(
        node(
            ChoiceKind::Bytes(BytesChoice {
                min_size: 8,
                max_size: 8,
            }),
            ChoiceValue::Bytes(vec![0u8; 8]),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_trivial_nodes_bytes_zeroed_range() {
    assert!(
        node(
            ChoiceKind::Bytes(BytesChoice {
                min_size: 2,
                max_size: LARGE_MAX_SIZE,
            }),
            ChoiceValue::Bytes(vec![0u8; 2]),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_trivial_nodes_integer_simplest_is_lower_bound() {
    // simplest() for [50, 100] is 50 (the endpoint closest to zero).
    assert!(
        node(
            ChoiceKind::Integer(IntegerChoice {
                min_value: 50,
                max_value: 100,
            }),
            ChoiceValue::Integer(50),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_trivial_nodes_integer_zero_in_range() {
    assert!(
        node(
            ChoiceKind::Integer(IntegerChoice {
                min_value: -10,
                max_value: 10,
            }),
            ChoiceValue::Integer(0),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_trivial_nodes_integer_full_i128_range() {
    // Native's "bounded unbounded" — the full i128 range, simplest=0.
    assert!(
        node(
            ChoiceKind::Integer(IntegerChoice {
                min_value: i128::MIN,
                max_value: i128::MAX,
            }),
            ChoiceValue::Integer(0),
            false,
        )
        .trivial()
    );
}

// test_nontrivial_nodes rows (value is NOT trivial).

#[test]
fn test_nontrivial_nodes_float_off_integer() {
    assert!(
        !node(
            ChoiceKind::Float(FloatChoice {
                min_value: 5.0,
                max_value: 10.0,
                allow_nan: false,
                allow_infinity: false,
            }),
            ChoiceValue::Float(6.0),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_nontrivial_nodes_float_off_zero() {
    assert!(
        !node(
            ChoiceKind::Float(FloatChoice {
                min_value: -5.0,
                max_value: 5.0,
                allow_nan: false,
                allow_infinity: false,
            }),
            ChoiceValue::Float(-5.0),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_nontrivial_nodes_float_unbounded_nonzero() {
    assert!(
        !node(
            ChoiceKind::Float(FloatChoice {
                min_value: f64::NEG_INFINITY,
                max_value: f64::INFINITY,
                allow_nan: true,
                allow_infinity: true,
            }),
            ChoiceValue::Float(1.0),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_nontrivial_nodes_boolean_true() {
    assert!(
        !node(
            ChoiceKind::Boolean(BooleanChoice),
            ChoiceValue::Boolean(true),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_nontrivial_nodes_string_off_simplest() {
    // Alphabet a-d with min_size=1: simplest is vec!['a']; "d" is nontrivial.
    assert!(
        !node(
            ChoiceKind::String(StringChoice {
                min_codepoint: b'a' as u32,
                max_codepoint: b'd' as u32,
                min_size: 1,
                max_size: LARGE_MAX_SIZE,
            }),
            ChoiceValue::String(vec![b'd' as u32]),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_nontrivial_nodes_bytes_nonzero_fixed() {
    assert!(
        !node(
            ChoiceKind::Bytes(BytesChoice {
                min_size: 1,
                max_size: 1,
            }),
            ChoiceValue::Bytes(vec![1u8]),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_nontrivial_nodes_bytes_nonempty_optional() {
    // min_size=0 → simplest is vec![]. vec![0] is nontrivial because it's
    // longer than the minimum.
    assert!(
        !node(
            ChoiceKind::Bytes(BytesChoice {
                min_size: 0,
                max_size: LARGE_MAX_SIZE,
            }),
            ChoiceValue::Bytes(vec![0u8]),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_nontrivial_nodes_bytes_longer_than_min() {
    // min_size=1 → simplest is b"\x00" (length 1). b"\x00\x00" is longer.
    assert!(
        !node(
            ChoiceKind::Bytes(BytesChoice {
                min_size: 1,
                max_size: 10,
            }),
            ChoiceValue::Bytes(vec![0u8, 0u8]),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_nontrivial_nodes_integer_at_bound() {
    assert!(
        !node(
            ChoiceKind::Integer(IntegerChoice {
                min_value: -10,
                max_value: 10,
            }),
            ChoiceValue::Integer(-10),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_nontrivial_nodes_integer_unbounded_nonzero() {
    assert!(
        !node(
            ChoiceKind::Integer(IntegerChoice {
                min_value: i128::MIN,
                max_value: i128::MAX,
            }),
            ChoiceValue::Integer(42),
            false,
        )
        .trivial()
    );
}

// test_conservative_nontrivial_nodes rows — these are actually trivial
// under richer analysis, but upstream's `trivial` (and our port) is
// conservative and reports them as non-trivial.

#[test]
fn test_conservative_nontrivial_nodes_float_interval_no_integer() {
    // [1.1, 1.6] contains no integer; conservative returns false.
    assert!(
        !node(
            ChoiceKind::Float(FloatChoice {
                min_value: 1.1,
                max_value: 1.6,
                allow_nan: false,
                allow_infinity: false,
            }),
            ChoiceValue::Float(1.5),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_conservative_nontrivial_nodes_float_semi_unbounded_positive() {
    // [f64::MAX - 1, +inf] — one infinite bound, conservative returns false.
    let max_f = f64::MAX;
    assert!(
        !node(
            ChoiceKind::Float(FloatChoice {
                min_value: max_f - 1.0,
                max_value: f64::INFINITY,
                allow_nan: false,
                allow_infinity: true,
            }),
            ChoiceValue::Float(max_f.floor()),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_conservative_nontrivial_nodes_float_semi_unbounded_negative() {
    let max_f = f64::MAX;
    assert!(
        !node(
            ChoiceKind::Float(FloatChoice {
                min_value: f64::NEG_INFINITY,
                max_value: -max_f + 1.0,
                allow_nan: false,
                allow_infinity: true,
            }),
            ChoiceValue::Float((-max_f).ceil()),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_conservative_nontrivial_nodes_float_only_pos_infinity() {
    assert!(
        !node(
            ChoiceKind::Float(FloatChoice {
                min_value: f64::INFINITY,
                max_value: f64::INFINITY,
                allow_nan: false,
                allow_infinity: true,
            }),
            ChoiceValue::Float(f64::INFINITY),
            false,
        )
        .trivial()
    );
}

#[test]
fn test_conservative_nontrivial_nodes_float_only_neg_infinity() {
    assert!(
        !node(
            ChoiceKind::Float(FloatChoice {
                min_value: f64::NEG_INFINITY,
                max_value: f64::NEG_INFINITY,
                allow_nan: false,
                allow_infinity: true,
            }),
            ChoiceValue::Float(f64::NEG_INFINITY),
            false,
        )
        .trivial()
    );
}

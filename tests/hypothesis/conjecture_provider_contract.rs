//! Ported from hypothesis-python/tests/conjecture/test_provider_contract.py
//!
//! Upstream exercises the `Provider` contract for three concrete
//! `ConjectureData` providers: `BytestringProvider` (drives draws from a
//! raw byte string), `URandomProvider` (reads bytes from `/dev/urandom`),
//! and `HypothesisProvider` (the default random-driven provider). The
//! single invariant under test is that each `draw_{type}(**constraints)`
//! returns a value satisfying `choice_permitted(value, constraints)`, plus
//! — for the bytestring provider — that setting `forced=choice_from_index(0, …)`
//! forces the drawn value to the simplest value of that kind.
//!
//! In `src/native/` the only "provider" is the `SmallRng` embedded in a
//! `NativeTestCase::new_random` — the equivalent of `HypothesisProvider`.
//! `BytestringProvider` and `URandomProvider` have no counterpart:
//! `NativeTestCase::for_choices` takes concrete `ChoiceValue`s, not a
//! raw byte string, and there is no `/dev/urandom`-driven constructor.
//!
//! Individually-skipped tests:
//!
//! - `test_provider_contract_bytestring` — drives a `ConjectureData` from
//!   an explicit byte string via `BytestringProvider`. No `src/native/`
//!   counterpart; native's prefix-based constructor (`for_choices`) takes
//!   `ChoiceValue`s, not bytes.
//! - `test_provider_contract[URandomProvider]` — the URandomProvider row
//!   of the parametrised `test_provider_contract`. No native counterpart.
//!
//! The `HypothesisProvider` row of `test_provider_contract` ports below
//! as one test per choice kind, iterating a handful of seeds to exercise
//! the random-draw path. The upstream PBT generates `nodes()` shapes via
//! `choice_types_constraints()`; we use explicit constraint rows in the
//! same style as `conjecture_choice.rs`.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{
    BooleanChoice, BytesChoice, ChoiceKind, ChoiceValue, FloatChoice, IntegerChoice,
    NativeTestCase, StringChoice,
};
use rand::SeedableRng;
use rand::rngs::SmallRng;

fn seeded(seed: u64) -> NativeTestCase {
    NativeTestCase::new_random(SmallRng::seed_from_u64(seed))
}

const SEEDS: &[u64] = &[0, 1, 2, 3, 17, 42, 12345, u64::MAX];

fn assert_integer_permitted(min_value: i128, max_value: i128) {
    let kind = IntegerChoice {
        min_value,
        max_value,
    };
    for &seed in SEEDS {
        let mut data = seeded(seed);
        let v = data.draw_integer(min_value, max_value).ok().unwrap();
        assert!(
            kind.validate(v),
            "draw_integer({min_value}, {max_value}) -> {v} not permitted (seed {seed})"
        );
    }
}

#[test]
fn test_provider_contract_hypothesis_integer_point() {
    assert_integer_permitted(0, 0);
    assert_integer_permitted(42, 42);
}

#[test]
fn test_provider_contract_hypothesis_integer_narrow() {
    assert_integer_permitted(0, 2);
    assert_integer_permitted(-5, 5);
    assert_integer_permitted(10, 20);
}

#[test]
fn test_provider_contract_hypothesis_integer_full_i128() {
    assert_integer_permitted(i128::MIN, i128::MAX);
}

#[test]
fn test_provider_contract_hypothesis_integer_semibounded() {
    assert_integer_permitted(0, i128::MAX);
    assert_integer_permitted(i128::MIN, 0);
}

fn assert_boolean_permitted(p: f64) {
    let kind = ChoiceKind::Boolean(BooleanChoice);
    for &seed in SEEDS {
        let mut data = seeded(seed);
        let v = data.weighted(p, None).ok().unwrap();
        assert!(
            kind.validate(&ChoiceValue::Boolean(v)),
            "weighted({p}) -> {v} not permitted (seed {seed})"
        );
    }
}

#[test]
fn test_provider_contract_hypothesis_boolean() {
    assert_boolean_permitted(0.5);
}

#[test]
fn test_provider_contract_hypothesis_boolean_degenerate() {
    // Upstream @example rows pin down the p=0/p=1 boundaries (forced to the
    // only possible value) and p near zero (forcing false is still valid).
    assert_boolean_permitted(0.0);
    assert_boolean_permitted(1.0);
    assert_boolean_permitted(1e-99);
}

fn assert_float_permitted(min_value: f64, max_value: f64, allow_nan: bool, allow_infinity: bool) {
    let kind = FloatChoice {
        min_value,
        max_value,
        allow_nan,
        allow_infinity,
    };
    for &seed in SEEDS {
        let mut data = seeded(seed);
        let v = data
            .draw_float(min_value, max_value, allow_nan, allow_infinity)
            .ok()
            .unwrap();
        assert!(
            kind.validate(v),
            "draw_float({min_value}, {max_value}, nan={allow_nan}, inf={allow_infinity}) \
             -> {v} not permitted (seed {seed})"
        );
    }
}

#[test]
fn test_provider_contract_hypothesis_float_point() {
    assert_float_permitted(0.0, 0.0, false, false);
    assert_float_permitted(1.0, 1.0, false, false);
}

#[test]
fn test_provider_contract_hypothesis_float_bounded() {
    assert_float_permitted(-10.0, 10.0, false, false);
    assert_float_permitted(-10.0, 10.0, true, false);
    assert_float_permitted(1.0, 2.0, false, false);
}

#[test]
fn test_provider_contract_hypothesis_float_unbounded() {
    assert_float_permitted(f64::NEG_INFINITY, f64::INFINITY, true, true);
    assert_float_permitted(f64::NEG_INFINITY, f64::INFINITY, false, true);
    assert_float_permitted(f64::NEG_INFINITY, f64::INFINITY, false, false);
}

fn assert_bytes_permitted(min_size: usize, max_size: usize) {
    let kind = BytesChoice { min_size, max_size };
    for &seed in SEEDS {
        let mut data = seeded(seed);
        let v = data.draw_bytes(min_size, max_size).ok().unwrap();
        assert!(
            kind.validate(&v),
            "draw_bytes({min_size}, {max_size}) -> {v:?} not permitted (seed {seed})"
        );
    }
}

#[test]
fn test_provider_contract_hypothesis_bytes() {
    assert_bytes_permitted(0, 0);
    assert_bytes_permitted(0, 10);
    assert_bytes_permitted(4, 4);
    assert_bytes_permitted(2, 16);
}

fn assert_string_permitted(
    min_codepoint: u32,
    max_codepoint: u32,
    min_size: usize,
    max_size: usize,
) {
    let kind = StringChoice {
        min_codepoint,
        max_codepoint,
        min_size,
        max_size,
    };
    for &seed in SEEDS {
        let mut data = seeded(seed);
        let s = data
            .draw_string(min_codepoint, max_codepoint, min_size, max_size)
            .ok()
            .unwrap();
        let codepoints: Vec<u32> = s.chars().map(|c| c as u32).collect();
        assert!(
            kind.validate(&codepoints),
            "draw_string([{min_codepoint}, {max_codepoint}], [{min_size}, {max_size}]) \
             -> {s:?} not permitted (seed {seed})"
        );
    }
}

#[test]
fn test_provider_contract_hypothesis_string_single_char_alphabet() {
    assert_string_permitted(b'a' as u32, b'a' as u32, 0, 10);
}

#[test]
fn test_provider_contract_hypothesis_string_ascii_range() {
    assert_string_permitted(b'a' as u32, b'z' as u32, 0, 10);
    assert_string_permitted(b'a' as u32, b'z' as u32, 3, 3);
}

#[test]
fn test_provider_contract_hypothesis_string_full_unicode() {
    // Full codepoint range; native's draw_string excludes surrogates internally.
    assert_string_permitted(0, 0x10FFFF, 0, 5);
}

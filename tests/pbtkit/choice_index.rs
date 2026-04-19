//! Ported from resources/pbtkit/tests/test_choice_index.py.
//!
//! Exercises the `to_index` / `from_index` / `sort_key` invariants for each
//! `ChoiceKind`. Native-gated because these index helpers live in
//! `src/native/core/choices.rs` and only `StringChoice` currently implements
//! them; the rest stub `todo!()`, which a later fixer-task invocation will
//! fill in.
//!
//! The Python original is parametrised over a union of five composite
//! strategies; Rust has no runtime dispatch for this without a dedicated
//! enum, so each of the six invariants is ported as five separate tests,
//! one per choice kind.
#![cfg(feature = "native")]

use hegel::__native_test_internals::{
    BigUint, BooleanChoice, BytesChoice, FloatChoice, IntegerChoice, StringChoice,
};
use hegel::generators::{self as gs};
use hegel::{Hegel, Settings, TestCase};

fn settings() -> Settings {
    Settings::new().test_cases(200).database(None)
}

// ── Composite generators ───────────────────────────────────────────────────

#[hegel::composite]
fn integer_kind_and_value(tc: TestCase) -> (IntegerChoice, i128) {
    let lo: i64 = tc.draw(
        gs::integers::<i64>()
            .min_value(-(1_i64 << 16))
            .max_value(1_i64 << 16),
    );
    let extra: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1_i64 << 16));
    let hi = lo + extra;
    let kind = IntegerChoice {
        min_value: i128::from(lo),
        max_value: i128::from(hi),
    };
    let value: i64 = tc.draw(gs::integers::<i64>().min_value(lo).max_value(hi));
    (kind, i128::from(value))
}

#[hegel::composite]
fn boolean_kind_and_value(tc: TestCase) -> (BooleanChoice, bool) {
    let value: bool = tc.draw(gs::booleans());
    (BooleanChoice, value)
}

#[hegel::composite]
fn bytes_kind_and_value(tc: TestCase) -> (BytesChoice, Vec<u8>) {
    let min_size: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(4));
    let extra: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(4));
    let max_size = min_size + extra;
    let kind = BytesChoice { min_size, max_size };
    let length: usize = tc.draw(
        gs::integers::<usize>()
            .min_value(kind.min_size)
            .max_value(kind.max_size),
    );
    let value: Vec<u8> = tc.draw(gs::binary().min_size(length).max_size(length));
    (kind, value)
}

#[hegel::composite]
fn string_kind_and_value(tc: TestCase) -> (StringChoice, Vec<u32>) {
    let min_cp: u32 = tc.draw(gs::integers::<u32>().min_value(32).max_value(126));
    let max_cp: u32 = tc.draw(
        gs::integers::<u32>()
            .min_value(min_cp)
            .max_value((min_cp + 20).min(126)),
    );
    let min_size: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(3));
    let extra: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(3));
    let max_size = min_size + extra;
    let kind = StringChoice {
        min_codepoint: min_cp,
        max_codepoint: max_cp,
        min_size,
        max_size,
    };
    let length: usize = tc.draw(
        gs::integers::<usize>()
            .min_value(kind.min_size)
            .max_value(kind.max_size),
    );
    let value: String = tc.draw(
        gs::text()
            .min_codepoint(kind.min_codepoint)
            .max_codepoint(kind.max_codepoint)
            .min_size(length)
            .max_size(length),
    );
    let cps: Vec<u32> = value.chars().map(|c| c as u32).collect();
    (kind, cps)
}

#[hegel::composite]
fn float_kind_and_value(tc: TestCase) -> (FloatChoice, f64) {
    let lo: f64 = tc.draw(gs::floats::<f64>().min_value(-1e6).max_value(1e6));
    let hi: f64 = tc.draw(gs::floats::<f64>().min_value(lo).max_value(lo + 1e6));
    let kind = FloatChoice {
        min_value: lo,
        max_value: hi,
        allow_nan: false,
        allow_infinity: false,
    };
    let value: f64 = tc.draw(gs::floats::<f64>().min_value(lo).max_value(hi));
    (kind, value)
}

// ── test_from_index_zero_is_simplest ───────────────────────────────────────

#[test]
fn test_from_index_zero_is_simplest_integer() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(integer_kind_and_value());
        assert_eq!(kind.from_index(BigUint::from(0u32)), Some(kind.simplest()));
    })
    .settings(settings())
    .run();
}

#[test]
fn test_from_index_zero_is_simplest_boolean() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(boolean_kind_and_value());
        assert_eq!(kind.from_index(BigUint::from(0u32)), Some(kind.simplest()));
    })
    .settings(settings())
    .run();
}

#[test]
fn test_from_index_zero_is_simplest_bytes() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(bytes_kind_and_value());
        assert_eq!(kind.from_index(BigUint::from(0u32)), Some(kind.simplest()));
    })
    .settings(settings())
    .run();
}

#[test]
fn test_from_index_zero_is_simplest_string() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(string_kind_and_value());
        assert_eq!(kind.from_index(BigUint::from(0u32)), Some(kind.simplest()));
    })
    .settings(settings())
    .run();
}

#[test]
fn test_from_index_zero_is_simplest_float() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(float_kind_and_value());
        assert_eq!(
            kind.from_index(BigUint::from(0u32)).map(f64::to_bits),
            Some(kind.simplest().to_bits())
        );
    })
    .settings(settings())
    .run();
}

// ── test_from_index_one_is_second_simplest ─────────────────────────────────

#[test]
fn test_from_index_one_is_second_simplest_integer() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(integer_kind_and_value());
        if let Some(v) = kind.from_index(BigUint::from(1u32)) {
            assert!(kind.sort_key(v) > kind.sort_key(kind.simplest()));
        }
    })
    .settings(settings())
    .run();
}

#[test]
fn test_from_index_one_is_second_simplest_boolean() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(boolean_kind_and_value());
        if let Some(v) = kind.from_index(BigUint::from(1u32)) {
            assert!(kind.sort_key(v) > kind.sort_key(kind.simplest()));
        }
    })
    .settings(settings())
    .run();
}

#[test]
fn test_from_index_one_is_second_simplest_bytes() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(bytes_kind_and_value());
        if let Some(v) = kind.from_index(BigUint::from(1u32)) {
            assert!(kind.sort_key(&v) > kind.sort_key(&kind.simplest()));
        }
    })
    .settings(settings())
    .run();
}

#[test]
fn test_from_index_one_is_second_simplest_string() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(string_kind_and_value());
        if let Some(v) = kind.from_index(BigUint::from(1u32)) {
            assert!(kind.sort_key(&v) > kind.sort_key(&kind.simplest()));
        }
    })
    .settings(settings())
    .run();
}

#[test]
fn test_from_index_one_is_second_simplest_float() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(float_kind_and_value());
        if let Some(v) = kind.from_index(BigUint::from(1u32)) {
            assert!(kind.sort_key(v) > kind.sort_key(kind.simplest()));
        }
    })
    .settings(settings())
    .run();
}

// ── test_roundtrip_value ───────────────────────────────────────────────────

#[test]
fn test_roundtrip_value_integer() {
    Hegel::new(|tc| {
        let (kind, value) = tc.draw(integer_kind_and_value());
        let index = kind.to_index(value);
        assert_eq!(kind.from_index(index), Some(value));
    })
    .settings(settings())
    .run();
}

#[test]
fn test_roundtrip_value_boolean() {
    Hegel::new(|tc| {
        let (kind, value) = tc.draw(boolean_kind_and_value());
        let index = kind.to_index(value);
        assert_eq!(kind.from_index(index), Some(value));
    })
    .settings(settings())
    .run();
}

#[test]
fn test_roundtrip_value_bytes() {
    Hegel::new(|tc| {
        let (kind, value) = tc.draw(bytes_kind_and_value());
        let index = kind.to_index(&value);
        assert_eq!(kind.from_index(index), Some(value));
    })
    .settings(settings())
    .run();
}

#[test]
fn test_roundtrip_value_string() {
    Hegel::new(|tc| {
        let (kind, value) = tc.draw(string_kind_and_value());
        let index = kind.to_index(&value);
        assert_eq!(kind.from_index(index), Some(value));
    })
    .settings(settings())
    .run();
}

#[test]
fn test_roundtrip_value_float() {
    Hegel::new(|tc| {
        let (kind, value) = tc.draw(float_kind_and_value());
        let index = kind.to_index(value);
        let back = kind.from_index(index);
        // Compare by bit pattern so -0.0 round-trips are flagged as
        // sign-preserving (matches pbtkit's NaN-bitwise check for floats).
        assert_eq!(back.map(f64::to_bits), Some(value.to_bits()));
    })
    .settings(settings())
    .run();
}

// ── test_to_index_non_negative ─────────────────────────────────────────────
//
// BigUint is unsigned by construction, so the invariant is that the call
// succeeds (i.e. `to_index` returns rather than panicking with `todo!()`).

#[test]
fn test_to_index_non_negative_integer() {
    Hegel::new(|tc| {
        let (kind, value) = tc.draw(integer_kind_and_value());
        let _ = kind.to_index(value);
    })
    .settings(settings())
    .run();
}

#[test]
fn test_to_index_non_negative_boolean() {
    Hegel::new(|tc| {
        let (kind, value) = tc.draw(boolean_kind_and_value());
        let _ = kind.to_index(value);
    })
    .settings(settings())
    .run();
}

#[test]
fn test_to_index_non_negative_bytes() {
    Hegel::new(|tc| {
        let (kind, value) = tc.draw(bytes_kind_and_value());
        let _ = kind.to_index(&value);
    })
    .settings(settings())
    .run();
}

#[test]
fn test_to_index_non_negative_string() {
    Hegel::new(|tc| {
        let (kind, value) = tc.draw(string_kind_and_value());
        let _ = kind.to_index(&value);
    })
    .settings(settings())
    .run();
}

#[test]
fn test_to_index_non_negative_float() {
    Hegel::new(|tc| {
        let (kind, value) = tc.draw(float_kind_and_value());
        let _ = kind.to_index(value);
    })
    .settings(settings())
    .run();
}

// ── test_from_index_then_to_index_le ───────────────────────────────────────

#[test]
fn test_from_index_then_to_index_le_integer() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(integer_kind_and_value());
        let extra: u64 = tc.draw(gs::integers::<u64>().min_value(0).max_value(100));
        let index = BigUint::from(extra);
        if let Some(value) = kind.from_index(index.clone()) {
            assert!(kind.to_index(value) <= index);
        }
    })
    .settings(settings())
    .run();
}

#[test]
fn test_from_index_then_to_index_le_boolean() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(boolean_kind_and_value());
        let extra: u64 = tc.draw(gs::integers::<u64>().min_value(0).max_value(100));
        let index = BigUint::from(extra);
        if let Some(value) = kind.from_index(index.clone()) {
            assert!(kind.to_index(value) <= index);
        }
    })
    .settings(settings())
    .run();
}

#[test]
fn test_from_index_then_to_index_le_bytes() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(bytes_kind_and_value());
        let extra: u64 = tc.draw(gs::integers::<u64>().min_value(0).max_value(100));
        let index = BigUint::from(extra);
        if let Some(value) = kind.from_index(index.clone()) {
            assert!(kind.to_index(&value) <= index);
        }
    })
    .settings(settings())
    .run();
}

#[test]
fn test_from_index_then_to_index_le_string() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(string_kind_and_value());
        let extra: u64 = tc.draw(gs::integers::<u64>().min_value(0).max_value(100));
        let index = BigUint::from(extra);
        if let Some(value) = kind.from_index(index.clone()) {
            assert!(kind.to_index(&value) <= index);
        }
    })
    .settings(settings())
    .run();
}

#[test]
fn test_from_index_then_to_index_le_float() {
    Hegel::new(|tc| {
        let (kind, _) = tc.draw(float_kind_and_value());
        let extra: u64 = tc.draw(gs::integers::<u64>().min_value(0).max_value(100));
        let index = BigUint::from(extra);
        if let Some(value) = kind.from_index(index.clone()) {
            assert!(kind.to_index(value) <= index);
        }
    })
    .settings(settings())
    .run();
}

// ── test_order_preserving ──────────────────────────────────────────────────

#[test]
fn test_order_preserving_integer() {
    Hegel::new(|tc| {
        let (kind, x) = tc.draw(integer_kind_and_value());
        let idx_x = kind.to_index(x);
        if let Some(y) = kind.from_index(idx_x + BigUint::from(1u32)) {
            assert!(kind.sort_key(x) <= kind.sort_key(y));
            assert!(kind.to_index(x) <= kind.to_index(y));
        }
    })
    .settings(settings())
    .run();
}

#[test]
fn test_order_preserving_boolean() {
    Hegel::new(|tc| {
        let (kind, x) = tc.draw(boolean_kind_and_value());
        let idx_x = kind.to_index(x);
        if let Some(y) = kind.from_index(idx_x + BigUint::from(1u32)) {
            assert!(kind.sort_key(x) <= kind.sort_key(y));
            assert!(kind.to_index(x) <= kind.to_index(y));
        }
    })
    .settings(settings())
    .run();
}

#[test]
fn test_order_preserving_bytes() {
    Hegel::new(|tc| {
        let (kind, x) = tc.draw(bytes_kind_and_value());
        let idx_x = kind.to_index(&x);
        if let Some(y) = kind.from_index(idx_x + BigUint::from(1u32)) {
            assert!(kind.sort_key(&x) <= kind.sort_key(&y));
            assert!(kind.to_index(&x) <= kind.to_index(&y));
        }
    })
    .settings(settings())
    .run();
}

#[test]
fn test_order_preserving_string() {
    Hegel::new(|tc| {
        let (kind, x) = tc.draw(string_kind_and_value());
        let idx_x = kind.to_index(&x);
        if let Some(y) = kind.from_index(idx_x + BigUint::from(1u32)) {
            assert!(kind.sort_key(&x) <= kind.sort_key(&y));
            assert!(kind.to_index(&x) <= kind.to_index(&y));
        }
    })
    .settings(settings())
    .run();
}

#[test]
fn test_order_preserving_float() {
    Hegel::new(|tc| {
        let (kind, x) = tc.draw(float_kind_and_value());
        let idx_x = kind.to_index(x);
        if let Some(y) = kind.from_index(idx_x + BigUint::from(1u32)) {
            assert!(kind.sort_key(x) <= kind.sort_key(y));
            assert!(kind.to_index(x) <= kind.to_index(y));
        }
    })
    .settings(settings())
    .run();
}

// clippy is rightfully complaining about a < n < b when that range is actually
// guaranteed by the types. Nevertheless I want these tests here as a foundational
// guardrail and for my sanity.
#![allow(clippy::absurd_extreme_comparisons)]
#![allow(clippy::manual_range_contains)]

mod common;

use common::utils::{assert_all_examples, find_any};
use hegel::generators;

#[test]
fn test_i8() {
    assert_all_examples(generators::integers::<i8>(), |&n| {
        n >= i8::MIN && n <= i8::MAX
    });
    find_any(generators::integers::<i8>(), |&n| n < i8::MIN / 2);
    find_any(generators::integers::<i8>(), |&n| n > i8::MAX / 2);
    find_any(generators::integers::<i8>(), |&n| n == i8::MIN);
    find_any(generators::integers::<i8>(), |&n| n == i8::MAX);
}

#[test]
fn test_i16() {
    assert_all_examples(generators::integers::<i16>(), |&n| {
        n >= i16::MIN && n <= i16::MAX
    });
    find_any(generators::integers::<i16>(), |&n| n < i16::MIN / 2);
    find_any(generators::integers::<i16>(), |&n| n > i16::MAX / 2);
    find_any(generators::integers::<i16>(), |&n| n == i16::MIN);
    find_any(generators::integers::<i16>(), |&n| n == i16::MAX);
}

#[test]
fn test_i32() {
    assert_all_examples(generators::integers::<i32>(), |&n| {
        n >= i32::MIN && n <= i32::MAX
    });
    find_any(generators::integers::<i32>(), |&n| n < i32::MIN / 2);
    find_any(generators::integers::<i32>(), |&n| n > i32::MAX / 2);
    find_any(generators::integers::<i32>(), |&n| n == i32::MIN);
    find_any(generators::integers::<i32>(), |&n| n == i32::MAX);
}

#[test]
fn test_i64() {
    assert_all_examples(generators::integers::<i64>(), |&n| {
        n >= i64::MIN && n <= i64::MAX
    });
    find_any(generators::integers::<i64>(), |&n| n < i64::MIN / 2);
    find_any(generators::integers::<i64>(), |&n| n > i64::MAX / 2);
    find_any(generators::integers::<i64>(), |&n| n == i64::MIN);
    find_any(generators::integers::<i64>(), |&n| n == i64::MAX);
}

#[test]
fn test_u8() {
    assert_all_examples(generators::integers::<u8>(), |&n| {
        n >= u8::MIN && n <= u8::MAX
    });
    find_any(generators::integers::<u8>(), |&n| n > u8::MAX / 2);
    find_any(generators::integers::<u8>(), |&n| n == u8::MIN);
    find_any(generators::integers::<u8>(), |&n| n == u8::MAX);
}

#[test]
fn test_u16() {
    assert_all_examples(generators::integers::<u16>(), |&n| {
        n >= u16::MIN && n <= u16::MAX
    });
    find_any(generators::integers::<u16>(), |&n| n > u16::MAX / 2);
    find_any(generators::integers::<u16>(), |&n| n == u16::MIN);
    find_any(generators::integers::<u16>(), |&n| n == u16::MAX);
}

#[test]
fn test_u32() {
    assert_all_examples(generators::integers::<u32>(), |&n| {
        n >= u32::MIN && n <= u32::MAX
    });
    find_any(generators::integers::<u32>(), |&n| n > u32::MAX / 2);
    find_any(generators::integers::<u32>(), |&n| n == u32::MIN);
    find_any(generators::integers::<u32>(), |&n| n == u32::MAX);
}

#[test]
fn test_u64() {
    assert_all_examples(generators::integers::<u64>(), |&n| {
        n >= u64::MIN && n <= u64::MAX
    });
    find_any(generators::integers::<u64>(), |&n| n > u64::MAX / 2);
    find_any(generators::integers::<u64>(), |&n| n == u64::MIN);
    find_any(generators::integers::<u64>(), |&n| n == u64::MAX);
}

#[test]
#[ignore]
fn test_i128() {
    assert_all_examples(generators::integers::<i128>(), |&n| {
        n >= i128::MIN && n <= i128::MAX
    });
    find_any(generators::integers::<i128>(), |&n| n < i128::MIN / 2);
    find_any(generators::integers::<i128>(), |&n| n > i128::MAX / 2);
    find_any(generators::integers::<i128>(), |&n| n == i128::MIN);
    find_any(generators::integers::<i128>(), |&n| n == i128::MAX);
}

#[test]
#[ignore]
fn test_u128() {
    assert_all_examples(generators::integers::<u128>(), |&n| {
        n >= u128::MIN && n <= u128::MAX
    });
    find_any(generators::integers::<u128>(), |&n| n > u128::MAX / 2);
    find_any(generators::integers::<u128>(), |&n| n == u128::MIN);
    find_any(generators::integers::<u128>(), |&n| n == u128::MAX);
}

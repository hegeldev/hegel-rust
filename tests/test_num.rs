#![cfg(feature = "num")]
// clippy is rightfully complaining about a < n < b when that range is actually
// guaranteed by the types. Nevertheless I want these tests here as a foundational
// guardrail and for my sanity.
#![allow(clippy::absurd_extreme_comparisons)]
#![allow(clippy::manual_range_contains)]

mod common;

use common::utils::{assert_all_examples, find_any};
use hegel::generators as gs;
use num_bigint::{BigInt, BigUint};
use num_traits::{One, Zero};

// ---------------------------------------------------------------------------
// BigInt
// ---------------------------------------------------------------------------

#[test]
fn test_big_integers_default_range() {
    let lower = -(BigInt::one() << 128u32);
    let upper = (BigInt::one() << 128u32) - BigInt::one();
    assert_all_examples(gs::big_integers(), move |n| *n >= lower && *n <= upper);
}

#[test]
fn test_big_integers_finds_zero() {
    find_any(gs::big_integers(), |n| n.is_zero());
}

#[test]
fn test_big_integers_finds_positive() {
    find_any(gs::big_integers(), |n| *n > BigInt::from(1_000_000));
}

#[test]
fn test_big_integers_finds_negative() {
    find_any(gs::big_integers(), |n| *n < BigInt::from(-1_000_000));
}

#[test]
fn test_big_integers_constrained() {
    let min = BigInt::from(-100);
    let max = BigInt::from(100);
    let generator = gs::big_integers()
        .min_value(BigInt::from(-100))
        .max_value(BigInt::from(100));
    assert_all_examples(generator, move |n| *n >= min && *n <= max);
}

#[test]
fn test_big_integers_finds_min_boundary() {
    let generator = gs::big_integers()
        .min_value(BigInt::from(-50))
        .max_value(BigInt::from(50));
    find_any(generator, |n| *n == BigInt::from(-50));
}

#[test]
fn test_big_integers_finds_max_boundary() {
    let generator = gs::big_integers()
        .min_value(BigInt::from(-50))
        .max_value(BigInt::from(50));
    find_any(generator, |n| *n == BigInt::from(50));
}

// ---------------------------------------------------------------------------
// BigUint
// ---------------------------------------------------------------------------

#[test]
fn test_big_uintegers_default_range() {
    let upper = (BigUint::one() << 128u32) - BigUint::one();
    assert_all_examples(gs::big_uintegers(), move |n| *n <= upper);
}

#[test]
fn test_big_uintegers_finds_zero() {
    find_any(gs::big_uintegers(), |n| n.is_zero());
}

#[test]
fn test_big_uintegers_finds_large() {
    find_any(gs::big_uintegers(), |n| *n > BigUint::from(1_000_000u64));
}

#[test]
fn test_big_uintegers_constrained() {
    let min = BigUint::from(10u32);
    let max = BigUint::from(200u32);
    let generator = gs::big_uintegers()
        .min_value(BigUint::from(10u32))
        .max_value(BigUint::from(200u32));
    assert_all_examples(generator, move |n| *n >= min && *n <= max);
}

#[test]
fn test_big_uintegers_finds_min_boundary() {
    let generator = gs::big_uintegers()
        .min_value(BigUint::from(5u32))
        .max_value(BigUint::from(100u32));
    find_any(generator, |n| *n == BigUint::from(5u32));
}

#[test]
fn test_big_uintegers_finds_max_boundary() {
    let generator = gs::big_uintegers()
        .min_value(BigUint::from(5u32))
        .max_value(BigUint::from(100u32));
    find_any(generator, |n| *n == BigUint::from(100u32));
}

// ---------------------------------------------------------------------------
// Rationals
// ---------------------------------------------------------------------------

#[test]
fn test_rationals_denom_positive() {
    assert_all_examples(gs::rationals(), |r| *r.denom() > 0);
}

#[test]
fn test_rationals_reduced() {
    assert_all_examples(gs::rationals(), |r| {
        // Ratio::new always reduces to lowest terms
        use num_integer::Integer;
        r.numer().gcd(r.denom()) == 1
    });
}

#[test]
fn test_rationals_finds_zero() {
    find_any(gs::rationals(), |r| r.is_zero());
}

#[test]
fn test_rationals_finds_negative() {
    find_any(gs::rationals(), |r| *r.numer() < 0);
}

#[test]
fn test_rationals_finds_whole_number() {
    find_any(gs::rationals(), |r| *r.denom() == 1 && *r.numer() != 0);
}

#[test]
fn test_rationals_custom_numerator_denominator() {
    let generator = gs::rationals()
        .numerator(gs::integers::<i64>().min_value(0).max_value(100))
        .denominator(gs::integers::<i64>().min_value(1).max_value(10));
    assert_all_examples(generator, |r| *r.numer() >= 0 && *r.denom() >= 1);
}

// ---------------------------------------------------------------------------
// Big Rationals
// ---------------------------------------------------------------------------

#[test]
fn test_big_rationals_denom_positive() {
    assert_all_examples(gs::big_rationals(), |r| *r.denom() > BigInt::zero());
}

#[test]
fn test_big_rationals_finds_zero() {
    find_any(gs::big_rationals(), |r| r.is_zero());
}

#[test]
fn test_big_rationals_finds_negative() {
    find_any(gs::big_rationals(), |r| *r.numer() < BigInt::zero());
}

// ---------------------------------------------------------------------------
// Complex
// ---------------------------------------------------------------------------

#[test]
fn test_complex_i64() {
    let generator = gs::complex(
        gs::integers::<i64>().min_value(-100).max_value(100),
        gs::integers::<i64>().min_value(-100).max_value(100),
    );
    assert_all_examples(generator, |c| {
        c.re >= -100 && c.re <= 100 && c.im >= -100 && c.im <= 100
    });
}

#[test]
fn test_complex_finds_zero() {
    let generator = gs::complex(
        gs::integers::<i64>().min_value(-100).max_value(100),
        gs::integers::<i64>().min_value(-100).max_value(100),
    );
    find_any(generator, |c| c.re == 0 && c.im == 0);
}

#[test]
fn test_complex_finds_purely_real() {
    let generator = gs::complex(
        gs::integers::<i64>().min_value(-100).max_value(100),
        gs::integers::<i64>().min_value(-100).max_value(100),
    );
    find_any(generator, |c| c.re != 0 && c.im == 0);
}

#[test]
fn test_complex_finds_purely_imaginary() {
    let generator = gs::complex(
        gs::integers::<i64>().min_value(-100).max_value(100),
        gs::integers::<i64>().min_value(-100).max_value(100),
    );
    find_any(generator, |c| c.re == 0 && c.im != 0);
}

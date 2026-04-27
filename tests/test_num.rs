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
    assert_all_examples(gs::integers::<BigInt>(), move |n| {
        *n >= lower && *n <= upper
    });
}

#[test]
fn test_big_integers_finds_zero() {
    find_any(gs::integers::<BigInt>(), |n| n.is_zero());
}

#[test]
fn test_big_integers_finds_positive() {
    find_any(gs::integers::<BigInt>(), |n| *n > BigInt::from(1_000_000));
}

#[test]
fn test_big_integers_finds_negative() {
    find_any(
        gs::integers::<BigInt>().min_value(BigInt::from(-BigInt::one() << 64u32)),
        |n| *n < BigInt::from(-1_000_000),
    );
}

#[test]
fn test_big_integers_constrained() {
    let min = BigInt::from(-100);
    let max = BigInt::from(100);
    let generator = gs::integers::<BigInt>()
        .min_value(BigInt::from(-100))
        .max_value(BigInt::from(100));
    assert_all_examples(generator, move |n| *n >= min && *n <= max);
}

#[test]
fn test_big_integers_finds_min_boundary() {
    let generator = gs::integers::<BigInt>()
        .min_value(BigInt::from(-50))
        .max_value(BigInt::from(50));
    find_any(generator, |n| *n == BigInt::from(-50));
}

#[test]
fn test_big_integers_finds_max_boundary() {
    let generator = gs::integers::<BigInt>()
        .min_value(BigInt::from(-50))
        .max_value(BigInt::from(50));
    find_any(generator, |n| *n == BigInt::from(50));
}

#[test]
fn test_big_integers_big() {
    let generator = gs::integers::<BigInt>()
        .min_value(BigInt::one() << 129u32)
        .max_value(BigInt::one() << 130u32);
    find_any(generator, |n| *n >= BigInt::one() << 129u32);
}

#[test]
fn test_big_integers_small() {
    let generator = gs::integers::<BigInt>()
        .min_value(-(BigInt::one() << 130u32))
        .max_value(-(BigInt::one() << 129u32));
    find_any(generator, |n| *n >= -(BigInt::one() << 130u32));
}
// ---------------------------------------------------------------------------
// BigUint
// ---------------------------------------------------------------------------

#[test]
fn test_big_uintegers_default_range() {
    let upper = (BigUint::one() << 128u32) - BigUint::one();
    assert_all_examples(gs::integers::<BigUint>(), move |n| *n <= upper);
}

#[test]
fn test_big_uintegers_finds_zero() {
    find_any(gs::integers::<BigUint>(), |n| n.is_zero());
}

#[test]
fn test_big_uintegers_finds_large() {
    find_any(gs::integers::<BigUint>(), |n| {
        *n > BigUint::from(1_000_000u64)
    });
}

#[test]
fn test_big_uintegers_constrained() {
    let min = BigUint::from(10u32);
    let max = BigUint::from(200u32);
    let generator = gs::integers::<BigUint>()
        .min_value(BigUint::from(10u32))
        .max_value(BigUint::from(200u32));
    assert_all_examples(generator, move |n| *n >= min && *n <= max);
}

#[test]
fn test_big_uintegers_finds_min_boundary() {
    let generator = gs::integers::<BigUint>()
        .min_value(BigUint::from(5u32))
        .max_value(BigUint::from(100u32));
    find_any(generator, |n| *n == BigUint::from(5u32));
}

#[test]
fn test_big_uintegers_finds_max_boundary() {
    let generator = gs::integers::<BigUint>()
        .min_value(BigUint::from(5u32))
        .max_value(BigUint::from(100u32));
    find_any(generator, |n| *n == BigUint::from(100u32));
}

// ---------------------------------------------------------------------------
// Rationals
// ---------------------------------------------------------------------------

#[test]
fn test_rationals_denom_positive() {
    assert_all_examples(gs::rationals::<i32>(), |r| *r.denom() > 0);
}

#[test]
fn test_rationals_reduced() {
    assert_all_examples(gs::rationals::<i32>(), |r| {
        // Ratio::new always reduces to lowest terms
        use num_integer::Integer;
        r.numer().gcd(r.denom()) == 1
    });
}

#[test]
fn test_rationals_finds_zero() {
    find_any(gs::rationals::<i32>(), |r| r.is_zero());
}

#[test]
fn test_rationals_finds_negative() {
    find_any(gs::rationals::<i32>(), |r| *r.numer() < 0);
}

#[test]
fn test_rationals_finds_whole_number() {
    find_any(gs::rationals::<i32>(), |r| {
        *r.denom() == 1 && *r.numer() != 0
    });
}

#[test]
fn test_rationals_custom_numerator_denominator() {
    let generator = gs::rationals::<BigInt>()
        .min_value(BigInt::from(0))
        .max_value(BigInt::from(100));
    assert_all_examples(generator, |r| {
        *r.numer() >= BigInt::zero() && *r.denom() >= BigInt::one()
    });
}

#[test]
#[should_panic(expected = "max_value * max_denominator overflows the numerator type")]
fn test_rationals_overflow() {
    use hegel::generators::Generator;
    gs::rationals::<i32>()
        .min_value(1230123)
        .max_value(2230123)
        .as_basic();
}

#[test]
fn test_rationals_biguint_denom_positive() {
    assert_all_examples(gs::rationals::<BigUint>(), |r| *r.denom() > BigUint::zero());
}

// ---------------------------------------------------------------------------
// BigInt as_basic (via map combinator)
// ---------------------------------------------------------------------------

#[test]
fn test_big_integers_map_uses_basic() {
    use hegel::generators::Generator;
    let generator = gs::integers::<BigInt>()
        .min_value(BigInt::from(0))
        .max_value(BigInt::from(100))
        .map(|n| n + BigInt::one());
    assert_all_examples(generator, move |n| {
        *n >= BigInt::one() && *n <= BigInt::from(101)
    });
}

// ---------------------------------------------------------------------------
// BigUint as_basic (via map combinator)
// ---------------------------------------------------------------------------

#[test]
fn test_big_uintegers_map_uses_basic() {
    use hegel::generators::Generator;
    let generator = gs::integers::<BigUint>()
        .min_value(BigUint::from(0u32))
        .max_value(BigUint::from(100u32))
        .map(|n| n + BigUint::one());
    assert_all_examples(generator, move |n| {
        *n >= BigUint::one() && *n <= BigUint::from(101u32)
    });
}

// ---------------------------------------------------------------------------
// DefaultGenerator for BigInt / BigUint
// ---------------------------------------------------------------------------

#[test]
fn test_default_bigint() {
    let lower = -(BigInt::one() << 128u32);
    let upper = (BigInt::one() << 128u32) - BigInt::one();
    assert_all_examples(gs::default::<BigInt>(), move |n| *n >= lower && *n <= upper);
}

#[test]
fn test_default_biguint() {
    let upper = (BigUint::one() << 128u32) - BigUint::one();
    assert_all_examples(gs::default::<BigUint>(), move |n| *n <= upper);
}

// ---------------------------------------------------------------------------
// Complex
// ---------------------------------------------------------------------------

#[test]
fn test_complex_f64_bounded_magnitude() {
    let generator = gs::complex::<f64>().max_magnitude(100.0);
    assert_all_examples(generator, |c| {
        c.re * c.re + c.im * c.im <= 100.0 * 100.0 + 1e-9
    });
}

#[test]
fn test_complex_f64_finite() {
    assert_all_examples(gs::complex::<f64>(), |c| {
        c.re.is_finite() && c.im.is_finite()
    });
}

#[test]
fn test_complex_f64_finds_zero() {
    find_any(gs::complex::<f64>(), |c| c.re == 0.0 && c.im == 0.0);
}

#[test]
fn test_complex_f64_finds_purely_real() {
    find_any(gs::complex::<f64>(), |c| c.re != 0.0 && c.im == 0.0);
}

#[test]
fn test_complex_f64_finds_purely_imaginary() {
    find_any(gs::complex::<f64>(), |c| c.re == 0.0 && c.im != 0.0);
}

#[test]
fn test_complex_f32_bounded_magnitude() {
    let generator = gs::complex::<f32>().max_magnitude(100.0);
    assert_all_examples(generator, |c| {
        c.re * c.re + c.im * c.im <= 100.0 * 100.0 + 1e-3
    });
}

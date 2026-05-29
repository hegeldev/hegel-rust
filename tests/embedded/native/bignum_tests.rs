// Embedded tests for src/native/bignum.rs — exercise the small-value
// optimisation: every operation must take the allocation-free `Small` path
// when both operands fit `i128`, transparently widen to `Big` on overflow, and
// re-normalise `Big` results that fall back into `i128` range.

use super::*;
use num_bigint::BigInt as RawBigInt;

/// A `Big` value just past `i128::MAX` (`i128::MAX + 1`).
fn big_pos() -> BigInt {
    let b = RawBigInt::from(i128::MAX) + RawBigInt::from(1);
    let v = BigInt::from(b);
    assert!(matches!(v, BigInt::Big(_)), "expected Big variant");
    v
}

/// A `Big` value just below `i128::MIN` (`i128::MIN - 1`).
fn big_neg() -> BigInt {
    let b = RawBigInt::from(i128::MIN) - RawBigInt::from(1);
    let v = BigInt::from(b);
    assert!(matches!(v, BigInt::Big(_)), "expected Big variant");
    v
}

#[test]
fn zero_one_is_zero() {
    assert_eq!(BigInt::zero(), BigInt::Small(0));
    assert_eq!(BigInt::one(), BigInt::Small(1));
    assert!(BigInt::zero().is_zero());
    assert!(!BigInt::Small(5).is_zero());
    assert!(!big_pos().is_zero());
}

#[test]
fn from_raw_normalises_into_small() {
    // A raw bignum that fits i128 collapses to Small.
    let r = RawBigInt::from(42);
    assert_eq!(BigInt::from(r), BigInt::Small(42));
    // A raw bignum past i128 stays Big.
    assert!(matches!(big_pos(), BigInt::Big(_)));
}

#[test]
fn from_primitives() {
    assert_eq!(BigInt::from(1i8), BigInt::Small(1));
    assert_eq!(BigInt::from(2i16), BigInt::Small(2));
    assert_eq!(BigInt::from(3i32), BigInt::Small(3));
    assert_eq!(BigInt::from(4i64), BigInt::Small(4));
    assert_eq!(BigInt::from(-5i128), BigInt::Small(-5));
    assert_eq!(BigInt::from(6u8), BigInt::Small(6));
    assert_eq!(BigInt::from(7u16), BigInt::Small(7));
    assert_eq!(BigInt::from(8u32), BigInt::Small(8));
    assert_eq!(BigInt::from(9u64), BigInt::Small(9));
    assert_eq!(BigInt::from(10usize), BigInt::Small(10));
    assert_eq!(BigInt::from(true), BigInt::Small(1));
    assert_eq!(BigInt::from(false), BigInt::Small(0));
    // u128 within i128 range → Small; beyond → Big.
    assert_eq!(BigInt::from(11u128), BigInt::Small(11));
    let big = BigInt::from(u128::MAX);
    assert!(matches!(big, BigInt::Big(_)));
}

#[test]
fn try_into_i128() {
    assert_eq!(i128::try_from(&BigInt::Small(-7)), Ok(-7));
    // A Big never fits i128 by invariant.
    assert_eq!(i128::try_from(&big_pos()), Err(()));
}

#[test]
fn try_into_unsigned() {
    // Small, in range.
    assert_eq!(u8::try_from(&BigInt::Small(200)), Ok(200u8));
    assert_eq!(u32::try_from(&BigInt::Small(70000)), Ok(70000u32));
    assert_eq!(u64::try_from(&BigInt::Small(1 << 40)), Ok(1u64 << 40));
    assert_eq!(usize::try_from(&BigInt::Small(123)), Ok(123usize));
    assert_eq!(u128::try_from(&BigInt::Small(123)), Ok(123u128));
    // Small, out of range (negative) → Err, exercising the map_err arm.
    assert_eq!(u8::try_from(&BigInt::Small(-1)), Err(()));
    assert_eq!(u32::try_from(&BigInt::Small(-1)), Err(()));
    assert_eq!(u64::try_from(&BigInt::Small(-1)), Err(()));
    assert_eq!(usize::try_from(&BigInt::Small(-1)), Err(()));
    assert_eq!(u128::try_from(&BigInt::Small(-1)), Err(()));
    // Small in range for u128 specifically (the widest).
    // Big: a positive Big fits u128 here (i128::MAX + 1), but not u64/u8.
    assert_eq!(u128::try_from(&big_pos()), Ok((i128::MAX as u128) + 1));
    assert_eq!(u64::try_from(&big_pos()), Err(()));
    assert_eq!(u8::try_from(&big_pos()), Err(()));
    assert_eq!(u32::try_from(&big_pos()), Err(()));
    assert_eq!(usize::try_from(&big_pos()), Err(()));
}

#[test]
fn ordering() {
    assert!(BigInt::Small(1) < BigInt::Small(2));
    assert_eq!(
        BigInt::Small(2).cmp(&BigInt::Small(2)),
        std::cmp::Ordering::Equal
    );
    // Small vs Big.
    assert!(BigInt::Small(5) < big_pos());
    assert!(BigInt::Small(5) > big_neg());
    // Big vs Small.
    assert!(big_pos() > BigInt::Small(i128::MAX));
    assert!(big_neg() < BigInt::Small(i128::MIN));
    // Big vs Big.
    assert!(big_neg() < big_pos());
    assert_eq!(big_pos().cmp(&big_pos()), std::cmp::Ordering::Equal);
    // partial_cmp delegates to cmp.
    assert_eq!(
        BigInt::Small(1).partial_cmp(&BigInt::Small(3)),
        Some(std::cmp::Ordering::Less)
    );
}

#[test]
fn display() {
    assert_eq!(format!("{}", BigInt::Small(-42)), "-42");
    assert_eq!(
        format!("{}", big_pos()),
        ((i128::MAX as u128) + 1).to_string()
    );
}

#[test]
fn add_all_ref_combos() {
    let a = BigInt::Small(2);
    let b = BigInt::Small(3);
    assert_eq!(a.clone() + b.clone(), BigInt::Small(5));
    assert_eq!(a.clone() + &b, BigInt::Small(5));
    assert_eq!(&a + b.clone(), BigInt::Small(5));
    assert_eq!(&a + &b, BigInt::Small(5));
}

#[test]
fn sub_all_ref_combos() {
    let a = BigInt::Small(10);
    let b = BigInt::Small(4);
    assert_eq!(a.clone() - b.clone(), BigInt::Small(6));
    assert_eq!(a.clone() - &b, BigInt::Small(6));
    assert_eq!(&a - b.clone(), BigInt::Small(6));
    assert_eq!(&a - &b, BigInt::Small(6));
}

#[test]
fn mul_all_ref_combos() {
    let a = BigInt::Small(6);
    let b = BigInt::Small(7);
    assert_eq!(a.clone() * b.clone(), BigInt::Small(42));
    assert_eq!(a.clone() * &b, BigInt::Small(42));
    assert_eq!(&a * b.clone(), BigInt::Small(42));
    assert_eq!(&a * &b, BigInt::Small(42));
}

#[test]
fn div_all_ref_combos() {
    let a = BigInt::Small(20);
    let b = BigInt::Small(6);
    assert_eq!(a.clone() / b.clone(), BigInt::Small(3));
    assert_eq!(a.clone() / &b, BigInt::Small(3));
    assert_eq!(&a / b.clone(), BigInt::Small(3));
    assert_eq!(&a / &b, BigInt::Small(3));
}

#[test]
fn rem_all_ref_combos() {
    let a = BigInt::Small(20);
    let b = BigInt::Small(6);
    assert_eq!(a.clone() % b.clone(), BigInt::Small(2));
    assert_eq!(a.clone() % &b, BigInt::Small(2));
    assert_eq!(&a % b.clone(), BigInt::Small(2));
    assert_eq!(&a % &b, BigInt::Small(2));
}

#[test]
fn assign_ops() {
    let mut x = BigInt::Small(10);
    x += BigInt::Small(5);
    assert_eq!(x, BigInt::Small(15));
    x += &BigInt::Small(5);
    assert_eq!(x, BigInt::Small(20));
    x -= BigInt::Small(1);
    x -= &BigInt::Small(1);
    assert_eq!(x, BigInt::Small(18));
    x *= BigInt::Small(2);
    x *= &BigInt::Small(2);
    assert_eq!(x, BigInt::Small(72));
    x /= BigInt::Small(2);
    x /= &BigInt::Small(3);
    assert_eq!(x, BigInt::Small(12));
    x %= BigInt::Small(7);
    assert_eq!(x, BigInt::Small(5));
    x %= &BigInt::Small(3);
    assert_eq!(x, BigInt::Small(2));
}

#[test]
fn neg() {
    assert_eq!(-BigInt::Small(5), BigInt::Small(-5));
    assert_eq!(-&BigInt::Small(5), BigInt::Small(-5));
    // i128::MIN cannot be negated in i128 → widens to Big.
    let n = -BigInt::Small(i128::MIN);
    assert!(matches!(n, BigInt::Big(_)));
    assert_eq!(u128::try_from(&n), Ok((i128::MAX as u128) + 1));
    // Negating a Big.
    assert_eq!(-big_pos(), big_neg() + BigInt::Small(1));
}

#[test]
fn arithmetic_overflow_widens_then_renormalises() {
    // Adding past i128::MAX widens to Big...
    let over = BigInt::Small(i128::MAX) + BigInt::Small(1);
    assert!(matches!(over, BigInt::Big(_)));
    // ...and subtracting back re-normalises to Small (mixed-operand path).
    assert_eq!(&over - &BigInt::Small(1), BigInt::Small(i128::MAX));
    // Underflow past i128::MIN.
    let under = BigInt::Small(i128::MIN) - BigInt::Small(1);
    assert!(matches!(under, BigInt::Big(_)));
    // Multiply overflow (Small/Small fast path widening).
    let big_mul = BigInt::Small(i128::MAX) * BigInt::Small(2);
    assert!(matches!(big_mul, BigInt::Big(_)));
    // Mixed-operand multiply (Big * Small) takes the promote-both arm.
    assert_eq!(big_pos() * BigInt::Small(2), &big_pos() + &big_pos());
    // Mixed-operand div / rem (Big / Small).
    assert_eq!(&big_mul / &BigInt::Small(2), BigInt::Small(i128::MAX));
    assert_eq!(&big_mul % &BigInt::Small(2), BigInt::Small(0));
    // Division and remainder that overflow i128 (MIN op -1) widen / renormalise.
    let div_over = BigInt::Small(i128::MIN) / BigInt::Small(-1);
    assert!(matches!(div_over, BigInt::Big(_)));
    assert_eq!(
        BigInt::Small(i128::MIN) % BigInt::Small(-1),
        BigInt::Small(0)
    );
}

#[test]
fn pow_small_and_big() {
    assert_eq!(BigInt::Small(2).pow(10), BigInt::Small(1024));
    // Overflowing i128 widens.
    let big = BigInt::Small(2).pow(200);
    assert!(matches!(big, BigInt::Big(_)));
    // pow on an already-Big base.
    let bigger = big.pow(2);
    assert!(matches!(bigger, BigInt::Big(_)));
    assert_eq!(bigger, BigInt::Small(2).pow(400));
}

#[test]
fn abs_and_abs_diff() {
    assert_eq!(BigInt::Small(-7).abs(), BigInt::Small(7));
    assert_eq!(BigInt::Small(7).abs(), BigInt::Small(7));
    // abs of i128::MIN widens.
    let m = BigInt::Small(i128::MIN).abs();
    assert!(matches!(m, BigInt::Big(_)));
    // abs of a negative Big and a positive Big.
    assert_eq!(big_neg().abs(), -big_neg());
    assert_eq!(big_pos().abs(), big_pos());
    // abs_diff is symmetric and non-negative.
    assert_eq!(
        BigInt::Small(3).abs_diff(&BigInt::Small(10)),
        BigInt::Small(7)
    );
    assert_eq!(
        BigInt::Small(10).abs_diff(&BigInt::Small(3)),
        BigInt::Small(7)
    );
}

#[test]
fn shr_small_and_big() {
    assert_eq!(BigInt::Small(1024).shr(3), BigInt::Small(128));
    // Shift count clamped at 127 for Small (non-negative → 0 at the limit).
    assert_eq!(BigInt::Small(1024).shr(200), BigInt::Small(0));
    // Shifting a Big back into i128 range re-normalises.
    let shifted = big_pos().shr(1);
    assert_eq!(shifted, BigInt::Small((i128::MAX / 2) + 1));
}

#[test]
fn clamp_to_bounds() {
    let lo = BigInt::Small(-5);
    let hi = BigInt::Small(5);
    assert_eq!(BigInt::Small(-10).clamp_to(&lo, &hi), lo.clone());
    assert_eq!(BigInt::Small(10).clamp_to(&lo, &hi), hi.clone());
    assert_eq!(BigInt::Small(3).clamp_to(&lo, &hi), BigInt::Small(3));
}

#[test]
fn to_f64_conversion() {
    assert_eq!(BigInt::Small(42).to_f64(), 42.0);
    // A Big that fits f64 converts to a large finite value.
    let f = big_pos().to_f64();
    assert!(f.is_finite() && f > 0.0);
    // A Big past f64::MAX saturates to +inf.
    let huge = BigInt::Small(2).pow(2000);
    assert!(huge.to_f64().is_infinite() && huge.to_f64() > 0.0);
    // Negative huge saturates to -inf.
    let huge_neg = -BigInt::Small(2).pow(2000);
    assert!(huge_neg.to_f64().is_infinite() && huge_neg.to_f64() < 0.0);
}

#[test]
fn hash_and_eq_are_consistent_across_variants() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(BigInt::Small(1));
    set.insert(big_pos());
    set.insert(big_pos());
    assert_eq!(set.len(), 2);
    assert!(set.contains(&BigInt::Small(1)));
    assert!(set.contains(&big_pos()));
}

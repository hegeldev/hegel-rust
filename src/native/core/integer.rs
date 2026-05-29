//! The [`Integer`] trait: the parametrised integer type underlying
//! [`IntegerChoice<T>`](super::choices::IntegerChoice).
//!
//! `IntegerChoice` used to hardcode `i128`, which both excluded genuine big
//! integers and forced 128-bit / `BigUint` arithmetic onto every small draw.
//! It is now generic over `T: Integer`, implemented for every native width
//! (`i8`..`i128`, `u8`..`u128`) and for [`BigInt`] (unbounded).
//!
//! The key observation that keeps the native path allocation-free: for *every*
//! native width the unsigned span `max - min` and any shrink distance fit in
//! `u128` (the full `i128`/`u128` span is exactly `2^128 - 1 = u128::MAX`, and
//! the index algorithm guarantees `count + 1 <= span`). So native `T` reuses the
//! original `u128` algorithm verbatim — converting `T <-> u128` with wrapping
//! `as` casts — and only allocates a [`BigUint`] at the index-space boundary the
//! [`ChoiceKind`](super::choices::ChoiceKind) interface already demands. Only
//! `BigInt` needs `BigUint` throughout.

use super::choices::{AnyInteger, AnyIntegerChoice, IntMagnitude, IntegerChoice};
use crate::native::bignum::{BigInt, BigUint, ToPrimitive, Zero};

/// The unsigned-distance type associated with an [`Integer`]: the magnitudes
/// the shrink-index arithmetic works in. `u128` for every native width,
/// [`BigUint`] for [`BigInt`]. Every operation here is exercised by the
/// `to_index` / `from_index` algorithms in
/// [`IntegerChoice`](super::choices::IntegerChoice).
pub trait UnsignedDistance: Clone + Ord {
    fn one() -> Self;
    /// `self + other`. Callers guarantee the result stays within the choice's
    /// span, which fits the type (see module docs), so this never overflows.
    /// (Named `plus`/`minus`/`min_with`/`max_with` to avoid colliding with
    /// `std::ops` / `Ord` methods of the same name on the concrete types.)
    fn plus(&self, other: &Self) -> Self;
    /// `self - other`. Callers guarantee `self >= other`.
    fn minus(&self, other: &Self) -> Self;
    fn min_with(&self, other: &Self) -> Self;
    fn max_with(&self, other: &Self) -> Self;
    /// `self / 2`, used for the `from_index` binary search midpoint.
    fn halve(&self) -> Self;
    fn to_biguint(&self) -> BigUint;
    /// `None` when `v` exceeds this type's range (an out-of-range index).
    fn try_from_biguint(v: &BigUint) -> Option<Self>;
    fn to_magnitude(self) -> IntMagnitude;
}

impl UnsignedDistance for u128 {
    fn one() -> Self {
        1
    }
    fn plus(&self, other: &Self) -> Self {
        self + other
    }
    fn minus(&self, other: &Self) -> Self {
        self - other
    }
    fn min_with(&self, other: &Self) -> Self {
        (*self).min(*other)
    }
    fn max_with(&self, other: &Self) -> Self {
        (*self).max(*other)
    }
    fn halve(&self) -> Self {
        self >> 1
    }
    fn to_biguint(&self) -> BigUint {
        BigUint::from(*self)
    }
    fn try_from_biguint(v: &BigUint) -> Option<Self> {
        v.to_u128()
    }
    fn to_magnitude(self) -> IntMagnitude {
        IntMagnitude::Small(self)
    }
}

impl UnsignedDistance for BigUint {
    fn one() -> Self {
        BigUint::from(1u32)
    }
    fn plus(&self, other: &Self) -> Self {
        self + other
    }
    fn minus(&self, other: &Self) -> Self {
        self - other
    }
    fn min_with(&self, other: &Self) -> Self {
        std::cmp::min(self, other).clone()
    }
    fn max_with(&self, other: &Self) -> Self {
        std::cmp::max(self, other).clone()
    }
    fn halve(&self) -> Self {
        self >> 1u32
    }
    fn to_biguint(&self) -> BigUint {
        self.clone()
    }
    fn try_from_biguint(v: &BigUint) -> Option<Self> {
        Some(v.clone())
    }
    fn to_magnitude(self) -> IntMagnitude {
        IntMagnitude::from_biguint(self)
    }
}

/// An integer type usable as the value of an [`IntegerChoice`].
///
/// Implemented for `i8`..`i128`, `u8`..`u128`, and [`BigInt`]. The associated
/// [`Unsigned`](Integer::Unsigned) type is `u128` for native widths and
/// [`BigUint`] for `BigInt`.
pub trait Integer: Clone + Eq + Ord + std::fmt::Debug + Send + Sync + 'static {
    type Unsigned: UnsignedDistance;

    /// The default shrink target.
    fn zero() -> Self;

    /// `|self - other|` as an unsigned distance. Order-independent.
    fn abs_diff(&self, other: &Self) -> Self::Unsigned;

    /// `self + delta`. Callers guarantee the result is in `[MIN, MAX]`.
    fn add_unsigned(&self, delta: &Self::Unsigned) -> Self;
    /// `self - delta`. Callers guarantee the result is in `[MIN, MAX]`.
    fn sub_unsigned(&self, delta: &Self::Unsigned) -> Self;

    /// `self + 1`, or `None` on overflow at this type's maximum.
    fn checked_succ(&self) -> Option<Self>;
    /// `self - 1`, or `None` on overflow at this type's minimum.
    fn checked_pred(&self) -> Option<Self>;

    fn to_bigint(&self) -> BigInt;
    /// Convert from a [`BigInt`], or `None` if it falls outside this type.
    fn from_bigint(v: &BigInt) -> Option<Self>;

    /// Type-erasure glue: wrap a typed choice / value into the homogeneous
    /// [`AnyIntegerChoice`] / [`AnyInteger`] stored in choice nodes, and the
    /// inverse downcast.
    fn wrap_choice(choice: IntegerChoice<Self>) -> AnyIntegerChoice;
    fn wrap_value(self) -> AnyInteger;
    fn unwrap_value(value: &AnyInteger) -> Option<Self>;
}

macro_rules! impl_integer_native {
    ($t:ty, $variant:ident, $to_prim:ident) => {
        impl Integer for $t {
            type Unsigned = u128;

            fn zero() -> Self {
                0
            }

            fn abs_diff(&self, other: &Self) -> u128 {
                let (hi, lo) = if self >= other {
                    (*self, *other)
                } else {
                    (*other, *self)
                };
                // Sign-extend both to u128 then wrapping-subtract: the true
                // difference is non-negative and fits u128, so the result is
                // exact (mirrors the original i128 `wrapping_sub` trick).
                (hi as u128).wrapping_sub(lo as u128)
            }

            fn add_unsigned(&self, delta: &u128) -> Self {
                (*self as u128).wrapping_add(*delta) as $t
            }

            fn sub_unsigned(&self, delta: &u128) -> Self {
                (*self as u128).wrapping_sub(*delta) as $t
            }

            fn checked_succ(&self) -> Option<Self> {
                self.checked_add(1)
            }

            fn checked_pred(&self) -> Option<Self> {
                self.checked_sub(1)
            }

            fn to_bigint(&self) -> BigInt {
                BigInt::from(*self)
            }

            fn from_bigint(v: &BigInt) -> Option<Self> {
                v.$to_prim()
            }

            fn wrap_choice(choice: IntegerChoice<Self>) -> AnyIntegerChoice {
                AnyIntegerChoice::$variant(choice)
            }

            fn wrap_value(self) -> AnyInteger {
                AnyInteger::$variant(self)
            }

            fn unwrap_value(value: &AnyInteger) -> Option<Self> {
                if let AnyInteger::$variant(x) = value {
                    Some(*x)
                } else {
                    None
                }
            }
        }
    };
}

impl_integer_native!(i8, I8, to_i8);
impl_integer_native!(i16, I16, to_i16);
impl_integer_native!(i32, I32, to_i32);
impl_integer_native!(i64, I64, to_i64);
impl_integer_native!(i128, I128, to_i128);
impl_integer_native!(u8, U8, to_u8);
impl_integer_native!(u16, U16, to_u16);
impl_integer_native!(u32, U32, to_u32);
impl_integer_native!(u64, U64, to_u64);
impl_integer_native!(u128, U128, to_u128);

impl Integer for BigInt {
    type Unsigned = BigUint;

    fn zero() -> Self {
        <BigInt as Zero>::zero()
    }

    fn abs_diff(&self, other: &Self) -> BigUint {
        (self - other).magnitude().clone()
    }

    fn add_unsigned(&self, delta: &BigUint) -> Self {
        self + BigInt::from(delta.clone())
    }

    fn sub_unsigned(&self, delta: &BigUint) -> Self {
        self - BigInt::from(delta.clone())
    }

    fn checked_succ(&self) -> Option<Self> {
        Some(self + 1)
    }

    fn checked_pred(&self) -> Option<Self> {
        Some(self - 1)
    }

    fn to_bigint(&self) -> BigInt {
        self.clone()
    }

    fn from_bigint(v: &BigInt) -> Option<Self> {
        Some(v.clone())
    }

    fn wrap_choice(choice: IntegerChoice<Self>) -> AnyIntegerChoice {
        AnyIntegerChoice::Big(choice)
    }

    fn wrap_value(self) -> AnyInteger {
        AnyInteger::Big(self)
    }

    fn unwrap_value(value: &AnyInteger) -> Option<Self> {
        if let AnyInteger::Big(x) = value {
            Some(x.clone())
        } else {
            None
        }
    }
}

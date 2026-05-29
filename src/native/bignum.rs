//! Arbitrary-precision integer for the native engine.
//!
//! The shortlex index arithmetic on the choice kinds
//! (`to_index` / `from_index` / `max_index`) accumulates beyond
//! fixed-width arithmetic over a wide index space, and an
//! [`IntegerChoice`](crate::native::core::choices::IntegerChoice) must be able
//! to represent genuinely arbitrary bounds (the schema protocol ships CBOR
//! bignums). Hypothesis (which uses Python `int` throughout) needs a Rust
//! analogue.
//!
//! [`BigInt`] is that analogue, with one crucial property: it is a small-value
//! optimisation over [`num_bigint::BigInt`]. Values that fit in `i128` are held
//! inline in the [`BigInt::Small`] variant and every operation on two `Small`
//! operands runs in native `i128` arithmetic with **no heap allocation**; only
//! when a result genuinely overflows `i128` do we fall back to the heap-backed
//! [`BigInt::Big`] variant. This matters because the index/sort-key machinery
//! is on the hot path of generation and shrinking, and the overwhelming
//! majority of integers a test draws fit comfortably in `i128`.
//!
//! Routing all big-integer arithmetic through this one type keeps the backend
//! choice localised: swapping `num-bigint` for e.g. `malachite` later would
//! only touch this file.

use std::cmp::Ordering;
use std::fmt;

use num_bigint::{BigInt as RawBigInt, Sign};
use num_traits::{Pow, ToPrimitive};

/// A signed integer that is allocation-free for values that fit in `i128` and
/// transparently widens to an arbitrary-precision representation otherwise.
///
/// # Invariant
///
/// A [`BigInt::Big`] **never** holds a value that would fit in `i128`: every
/// constructor and arithmetic result is normalised (see [`BigInt::from_raw`])
/// so that any `i128`-representable value lives in [`BigInt::Small`]. This
/// canonical form is what makes the derived [`PartialEq`] / [`Eq`] / [`Hash`]
/// correct (equal values always share a variant) and what guarantees the
/// `Small` fast path is taken whenever it possibly can be.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BigInt {
    /// A value that fits in `i128`. The common case; allocation-free.
    Small(i128),
    /// A value outside the `i128` range. Never holds an `i128`-representable
    /// value (the type invariant).
    Big(RawBigInt),
}

impl BigInt {
    /// The additive identity. Always a [`BigInt::Small`].
    pub fn zero() -> BigInt {
        BigInt::Small(0)
    }

    /// The multiplicative identity. Always a [`BigInt::Small`].
    pub fn one() -> BigInt {
        BigInt::Small(1)
    }

    /// True iff this is zero. Cheap: zero is always `Small(0)` by the
    /// normalisation invariant.
    pub fn is_zero(&self) -> bool {
        matches!(self, BigInt::Small(0))
    }

    /// Materialise a heap `num_bigint::BigInt` regardless of variant. Clones
    /// for the `Big` case; constructs from the inline `i128` for `Small`.
    fn to_raw(&self) -> RawBigInt {
        match self {
            BigInt::Small(x) => RawBigInt::from(*x),
            BigInt::Big(b) => b.clone(),
        }
    }

    /// Normalise a raw `num_bigint::BigInt` into a [`BigInt`], collapsing to
    /// [`BigInt::Small`] whenever the value fits `i128`. The single choke point
    /// that upholds the type invariant.
    fn from_raw(r: RawBigInt) -> BigInt {
        match r.to_i128() {
            Some(x) => BigInt::Small(x),
            None => BigInt::Big(r),
        }
    }

    /// `self` raised to the power `exp`.
    pub fn pow(&self, exp: u32) -> BigInt {
        match self {
            BigInt::Small(x) => match x.checked_pow(exp) {
                Some(v) => BigInt::Small(v),
                None => BigInt::from_raw(Pow::pow(RawBigInt::from(*x), exp)),
            },
            BigInt::Big(b) => BigInt::from_raw(Pow::pow(b.clone(), exp)),
        }
    }

    /// Absolute value (always non-negative).
    pub fn abs(&self) -> BigInt {
        match self {
            BigInt::Small(x) => match x.checked_abs() {
                Some(v) => BigInt::Small(v),
                // Only `i128::MIN` lacks a representable negation.
                None => BigInt::from_raw(-RawBigInt::from(*x)),
            },
            BigInt::Big(b) => BigInt::from_raw(if b.sign() == Sign::Minus {
                -b.clone()
            } else {
                b.clone()
            }),
        }
    }

    /// The non-negative distance `|self - other|`.
    pub fn abs_diff(&self, other: &BigInt) -> BigInt {
        BigInt::sub_core(self, other).abs()
    }

    /// Arithmetic right shift by `k` bits (floor-divide by `2^k`). Intended for
    /// the shrinker's shift-right descent over non-negative distances; for a
    /// `Small` operand the shift count is clamped to `127` so it never trips
    /// the native shift-overflow guard (a non-negative `i128 >> 127` is `0`,
    /// which is the desired limit).
    pub fn shr(&self, k: u32) -> BigInt {
        match self {
            BigInt::Small(x) => BigInt::Small(x >> k.min(127)),
            BigInt::Big(b) => BigInt::from_raw(b >> (k as usize)),
        }
    }

    /// Clamp into `[min, max]`. `min <= max` is assumed.
    pub fn clamp_to(self, min: &BigInt, max: &BigInt) -> BigInt {
        if &self < min {
            min.clone()
        } else if &self > max {
            max.clone()
        } else {
            self
        }
    }

    /// Number of bits in the magnitude `|self|`; `0` for zero.
    pub fn bit_length(&self) -> u64 {
        match self {
            BigInt::Small(x) => u64::from(128 - x.unsigned_abs().leading_zeros()),
            BigInt::Big(b) => b.bits(),
        }
    }

    /// Construct a non-negative `BigInt` from little-endian unsigned bytes.
    /// Used by the arbitrary-precision uniform sampler to turn random bytes
    /// into a magnitude.
    pub fn from_unsigned_le_bytes(bytes: &[u8]) -> BigInt {
        BigInt::from_raw(RawBigInt::from_bytes_le(Sign::Plus, bytes))
    }

    /// Minimal two's-complement little-endian byte encoding, for persistence.
    /// Round-trips through [`Self::from_signed_le_bytes`].
    pub fn to_signed_le_bytes(&self) -> Vec<u8> {
        self.to_raw().to_signed_bytes_le()
    }

    /// Inverse of [`Self::to_signed_le_bytes`].
    pub fn from_signed_le_bytes(bytes: &[u8]) -> BigInt {
        BigInt::from_raw(RawBigInt::from_signed_bytes_le(bytes))
    }

    /// Lossy conversion to `f64`. `Small` casts directly; a `Big` goes through
    /// `num_bigint`, which already saturates an out-of-range magnitude to the
    /// signed infinity (so the `unwrap_or` default is only a belt-and-braces
    /// fallback that never fires in practice).
    pub fn to_f64(&self) -> f64 {
        match self {
            BigInt::Small(x) => *x as f64,
            BigInt::Big(b) => b.to_f64().unwrap_or(f64::INFINITY),
        }
    }

    fn add_core(a: &BigInt, b: &BigInt) -> BigInt {
        match (a, b) {
            (BigInt::Small(x), BigInt::Small(y)) => match x.checked_add(*y) {
                Some(v) => BigInt::Small(v),
                None => BigInt::from_raw(RawBigInt::from(*x) + RawBigInt::from(*y)),
            },
            _ => BigInt::from_raw(a.to_raw() + b.to_raw()),
        }
    }

    fn sub_core(a: &BigInt, b: &BigInt) -> BigInt {
        match (a, b) {
            (BigInt::Small(x), BigInt::Small(y)) => match x.checked_sub(*y) {
                Some(v) => BigInt::Small(v),
                None => BigInt::from_raw(RawBigInt::from(*x) - RawBigInt::from(*y)),
            },
            _ => BigInt::from_raw(a.to_raw() - b.to_raw()),
        }
    }

    fn mul_core(a: &BigInt, b: &BigInt) -> BigInt {
        match (a, b) {
            (BigInt::Small(x), BigInt::Small(y)) => match x.checked_mul(*y) {
                Some(v) => BigInt::Small(v),
                None => BigInt::from_raw(RawBigInt::from(*x) * RawBigInt::from(*y)),
            },
            _ => BigInt::from_raw(a.to_raw() * b.to_raw()),
        }
    }

    fn div_core(a: &BigInt, b: &BigInt) -> BigInt {
        match (a, b) {
            (BigInt::Small(x), BigInt::Small(y)) => match x.checked_div(*y) {
                Some(v) => BigInt::Small(v),
                // Only `i128::MIN / -1` overflows; promote and retry.
                None => BigInt::from_raw(RawBigInt::from(*x) / RawBigInt::from(*y)),
            },
            _ => BigInt::from_raw(a.to_raw() / b.to_raw()),
        }
    }

    fn rem_core(a: &BigInt, b: &BigInt) -> BigInt {
        match (a, b) {
            (BigInt::Small(x), BigInt::Small(y)) => match x.checked_rem(*y) {
                Some(v) => BigInt::Small(v),
                None => BigInt::from_raw(RawBigInt::from(*x) % RawBigInt::from(*y)),
            },
            _ => BigInt::from_raw(a.to_raw() % b.to_raw()),
        }
    }

    fn neg_core(a: &BigInt) -> BigInt {
        match a {
            BigInt::Small(x) => match x.checked_neg() {
                Some(v) => BigInt::Small(v),
                None => BigInt::from_raw(-RawBigInt::from(*x)),
            },
            BigInt::Big(b) => BigInt::from_raw(-b.clone()),
        }
    }
}

impl Ord for BigInt {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (BigInt::Small(a), BigInt::Small(b)) => a.cmp(b),
            // A `Big` is always outside `i128`, so its sign alone decides the
            // comparison against any `Small` — no allocation needed.
            (BigInt::Small(_), BigInt::Big(b)) => match b.sign() {
                Sign::Minus => Ordering::Greater,
                _ => Ordering::Less,
            },
            (BigInt::Big(a), BigInt::Small(_)) => match a.sign() {
                Sign::Minus => Ordering::Less,
                _ => Ordering::Greater,
            },
            (BigInt::Big(a), BigInt::Big(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for BigInt {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Ergonomic comparison against `i128` (an `i128` always lands in the `Small`
/// variant, so these are allocation-free).
impl PartialEq<i128> for BigInt {
    fn eq(&self, other: &i128) -> bool {
        *self == BigInt::Small(*other)
    }
}

impl PartialEq<BigInt> for i128 {
    fn eq(&self, other: &BigInt) -> bool {
        BigInt::Small(*self) == *other
    }
}

impl PartialOrd<i128> for BigInt {
    fn partial_cmp(&self, other: &i128) -> Option<Ordering> {
        Some(self.cmp(&BigInt::Small(*other)))
    }
}

impl PartialOrd<BigInt> for i128 {
    fn partial_cmp(&self, other: &BigInt) -> Option<Ordering> {
        Some(BigInt::Small(*self).cmp(other))
    }
}

impl fmt::Display for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BigInt::Small(x) => write!(f, "{x}"),
            BigInt::Big(b) => write!(f, "{b}"),
        }
    }
}

/// `From` for every primitive that fits losslessly in `i128`.
macro_rules! impl_from_small {
    ($($t:ty),*) => {$(
        impl From<$t> for BigInt {
            fn from(v: $t) -> BigInt {
                BigInt::Small(v as i128)
            }
        }
    )*};
}
impl_from_small!(i8, i16, i32, i64, u8, u16, u32, u64, usize);

impl From<i128> for BigInt {
    fn from(v: i128) -> BigInt {
        BigInt::Small(v)
    }
}

impl From<u128> for BigInt {
    fn from(v: u128) -> BigInt {
        match i128::try_from(v) {
            Ok(x) => BigInt::Small(x),
            Err(_) => BigInt::Big(RawBigInt::from(v)),
        }
    }
}

impl From<bool> for BigInt {
    fn from(v: bool) -> BigInt {
        BigInt::Small(v as i128)
    }
}

impl From<RawBigInt> for BigInt {
    fn from(v: RawBigInt) -> BigInt {
        BigInt::from_raw(v)
    }
}

/// Cloning conversion from a borrow, so APIs taking `impl Into<BigInt>` accept
/// `&BigInt` without the caller writing `.clone()`.
impl From<&BigInt> for BigInt {
    fn from(v: &BigInt) -> BigInt {
        v.clone()
    }
}

/// `TryFrom<&BigInt>` for `i128`: infallible-looking but `Err` for any `Big`
/// (which by invariant never fits `i128`).
impl TryFrom<&BigInt> for i128 {
    type Error = ();
    fn try_from(v: &BigInt) -> Result<i128, ()> {
        match v {
            BigInt::Small(x) => Ok(*x),
            BigInt::Big(_) => Err(()),
        }
    }
}

/// `TryFrom<&BigInt>` for unsigned primitives, via `ToPrimitive` (`None` for
/// negative or out-of-range values).
macro_rules! impl_try_into_unsigned {
    ($($t:ty => $m:ident),*) => {$(
        impl TryFrom<&BigInt> for $t {
            type Error = ();
            fn try_from(v: &BigInt) -> Result<$t, ()> {
                match v {
                    BigInt::Small(x) => (*x).try_into().map_err(|_| ()),
                    BigInt::Big(b) => b.$m().ok_or(()),
                }
            }
        }
    )*};
}
impl_try_into_unsigned!(u8 => to_u8, u32 => to_u32, u64 => to_u64, usize => to_usize, u128 => to_u128);

/// Owned `TryFrom<BigInt>` for each primitive, delegating to the borrowed impl
/// (call sites frequently `.try_into()` an owned arithmetic temporary).
macro_rules! impl_try_from_owned {
    ($($t:ty),*) => {$(
        impl TryFrom<BigInt> for $t {
            type Error = ();
            fn try_from(v: BigInt) -> Result<$t, ()> {
                <$t>::try_from(&v)
            }
        }
    )*};
}
impl_try_from_owned!(i128, u8, u32, u64, usize, u128);

/// Generate the four owned/borrowed combinations of a binary operator from a
/// single `(&BigInt, &BigInt) -> BigInt` core.
macro_rules! impl_binop {
    ($trait:ident, $method:ident, $core:ident) => {
        impl std::ops::$trait<BigInt> for BigInt {
            type Output = BigInt;
            fn $method(self, rhs: BigInt) -> BigInt {
                BigInt::$core(&self, &rhs)
            }
        }
        impl std::ops::$trait<&BigInt> for BigInt {
            type Output = BigInt;
            fn $method(self, rhs: &BigInt) -> BigInt {
                BigInt::$core(&self, rhs)
            }
        }
        impl std::ops::$trait<BigInt> for &BigInt {
            type Output = BigInt;
            fn $method(self, rhs: BigInt) -> BigInt {
                BigInt::$core(self, &rhs)
            }
        }
        impl<'a, 'b> std::ops::$trait<&'b BigInt> for &'a BigInt {
            type Output = BigInt;
            fn $method(self, rhs: &'b BigInt) -> BigInt {
                BigInt::$core(self, rhs)
            }
        }
    };
}
impl_binop!(Add, add, add_core);
impl_binop!(Sub, sub, sub_core);
impl_binop!(Mul, mul, mul_core);
impl_binop!(Div, div, div_core);
impl_binop!(Rem, rem, rem_core);

/// Generate `OpAssign<BigInt>` and `OpAssign<&BigInt>` from the value operator.
macro_rules! impl_assign {
    ($trait:ident, $method:ident, $op:tt) => {
        impl std::ops::$trait<BigInt> for BigInt {
            fn $method(&mut self, rhs: BigInt) {
                *self = &*self $op &rhs;
            }
        }
        impl std::ops::$trait<&BigInt> for BigInt {
            fn $method(&mut self, rhs: &BigInt) {
                *self = &*self $op rhs;
            }
        }
    };
}
impl_assign!(AddAssign, add_assign, +);
impl_assign!(SubAssign, sub_assign, -);
impl_assign!(MulAssign, mul_assign, *);
impl_assign!(DivAssign, div_assign, /);
impl_assign!(RemAssign, rem_assign, %);

impl std::ops::Neg for BigInt {
    type Output = BigInt;
    fn neg(self) -> BigInt {
        BigInt::neg_core(&self)
    }
}
impl std::ops::Neg for &BigInt {
    type Output = BigInt;
    fn neg(self) -> BigInt {
        BigInt::neg_core(self)
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/bignum_tests.rs"]
mod tests;

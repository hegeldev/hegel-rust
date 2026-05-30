//! Arbitrary-precision integer types for the native engine.
//!
//! The shortlex index arithmetic on `IntegerChoice`
//! (`to_index` / `from_index` / `max_index`) accumulates beyond
//! fixed-width arithmetic over the full `u128`-wide index space, so
//! Hypothesis (which uses Python `int` throughout) needs a Rust
//! analogue.  Routing all big-integer arithmetic through this module
//! keeps the backend choice localised.
//!
//! The backend is [`dashu_int`], whose `IBig` / `UBig` keep small values
//! inline rather than forcing a heap allocation on every value the way
//! `num-bigint` did.  [`BigInt`] / [`BigUint`] are thin newtypes over those
//! that re-expose the slice of the old `num-bigint` API the rest of the engine
//! relies on (a three-valued [`Sign`], [`ToPrimitive`], [`Zero`], [`Signed`],
//! and the usual arithmetic operators), so swapping the backend stays confined
//! to this file.

use dashu_int::ops::BitTest;
use dashu_int::{IBig, Sign as DashuSign, UBig};

/// Three-valued sign, matching the old `num-bigint` API: zero is [`Sign::NoSign`]
/// (distinct from both [`Sign::Plus`] and [`Sign::Minus`]). Callers rely on this:
/// `value.sign() == Sign::Plus` means *strictly positive*.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// Variant names deliberately mirror the old `num_bigint::Sign` API.
#[allow(clippy::enum_variant_names)]
pub enum Sign {
    Minus,
    NoSign,
    Plus,
}

/// A signed arbitrary-precision integer.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BigInt(IBig);

/// An unsigned arbitrary-precision integer (a magnitude).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BigUint(UBig);

/// `num-traits`-style narrowing conversions. Each returns `None` when the value
/// falls outside the target type's range.
pub trait ToPrimitive {
    fn to_i8(&self) -> Option<i8>;
    fn to_i16(&self) -> Option<i16>;
    fn to_i32(&self) -> Option<i32>;
    fn to_i64(&self) -> Option<i64>;
    fn to_i128(&self) -> Option<i128>;
    fn to_u8(&self) -> Option<u8>;
    fn to_u16(&self) -> Option<u16>;
    fn to_u32(&self) -> Option<u32>;
    fn to_u64(&self) -> Option<u64>;
    fn to_u128(&self) -> Option<u128>;
    fn to_f64(&self) -> Option<f64>;
}

/// `num-traits`-style additive identity.
pub trait Zero {
    fn zero() -> Self;
    fn is_zero(&self) -> bool;
}

/// `num-traits`-style `abs` for signed values.
pub trait Signed {
    fn abs(&self) -> Self;
}

impl ::core::fmt::Display for BigInt {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        ::core::fmt::Display::fmt(&self.0, f)
    }
}

impl ::core::fmt::Display for BigUint {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        ::core::fmt::Display::fmt(&self.0, f)
    }
}

fn to_dashu_sign(sign: Sign) -> DashuSign {
    match sign {
        Sign::Minus => DashuSign::Negative,
        // A zero magnitude normalises to a non-negative `IBig` regardless, so
        // mapping `NoSign` to `Positive` is faithful.
        Sign::NoSign | Sign::Plus => DashuSign::Positive,
    }
}

impl BigInt {
    /// Build from a sign and big-endian magnitude bytes (mirrors
    /// `num_bigint::BigInt::from_bytes_be`).
    pub fn from_bytes_be(sign: Sign, bytes: &[u8]) -> BigInt {
        BigInt(IBig::from_parts(
            to_dashu_sign(sign),
            UBig::from_be_bytes(bytes),
        ))
    }

    /// Big-endian magnitude bytes, paired with the value's [`Sign`].
    pub fn to_bytes_be(&self) -> (Sign, Vec<u8>) {
        let (sign, mag) = self.0.clone().into_parts();
        (
            from_dashu_sign(sign, mag.is_zero()),
            mag.to_be_bytes().into_vec(),
        )
    }

    /// The value's [`Sign`] (three-valued: zero is [`Sign::NoSign`]).
    pub fn sign(&self) -> Sign {
        from_dashu_sign(self.0.sign(), self.0.is_zero())
    }

    /// The value's magnitude as a [`BigUint`].
    pub fn magnitude(&self) -> BigUint {
        BigUint(self.0.clone().into_parts().1)
    }

    /// Minimal two's-complement little-endian bytes (mirrors
    /// `num_bigint::BigInt::to_signed_bytes_le`). Compatible with
    /// [`Self::from_signed_bytes_le`].
    pub fn to_signed_bytes_le(&self) -> Vec<u8> {
        let (sign, mag) = self.0.clone().into_parts();
        if mag.is_zero() {
            return vec![0];
        }
        let mut bytes = mag.to_le_bytes().into_vec();
        // The most-significant byte's top bit is the sign bit. A positive value
        // whose top bit is set needs a leading `0x00`; a negative value is then
        // two's-complemented in place (and likewise needs the spare byte so the
        // sign bit lands clear-of-magnitude before negation).
        if bytes.last().unwrap() & 0x80 != 0 {
            bytes.push(0);
        }
        if sign == DashuSign::Negative {
            twos_complement_le(&mut bytes);
        }
        bytes
    }

    /// Decode minimal two's-complement little-endian bytes (mirrors
    /// `num_bigint::BigInt::from_signed_bytes_le`).
    pub fn from_signed_bytes_le(bytes: &[u8]) -> BigInt {
        match bytes.last() {
            None => BigInt::zero(),
            Some(&last) if last & 0x80 == 0 => BigInt(IBig::from(UBig::from_le_bytes(bytes))),
            Some(_) => {
                let mut buf = bytes.to_vec();
                twos_complement_le(&mut buf);
                BigInt(IBig::from_parts(
                    DashuSign::Negative,
                    UBig::from_le_bytes(&buf),
                ))
            }
        }
    }

    /// The number of bits in this value's magnitude (0 for zero).
    pub fn bits(&self) -> u64 {
        self.magnitude().bits()
    }
}

impl BigUint {
    /// Build from little-endian magnitude bytes.
    pub fn from_bytes_le(bytes: &[u8]) -> BigUint {
        BigUint(UBig::from_le_bytes(bytes))
    }

    /// The number of bits needed to represent this value (0 for zero).
    pub fn bits(&self) -> u64 {
        self.0.bit_len() as u64
    }

    /// `self` raised to the power `exp`.
    pub fn pow(&self, exp: u32) -> BigUint {
        BigUint(self.0.pow(exp as usize))
    }
}

/// Negate `bytes` in place as a little-endian two's-complement integer
/// (bitwise NOT then add one). Shared by signed (de)serialisation.
fn twos_complement_le(bytes: &mut [u8]) {
    let mut carry = true;
    for b in bytes.iter_mut() {
        let (sum, overflow) = (!*b).overflowing_add(carry as u8);
        *b = sum;
        carry = overflow;
    }
}

fn from_dashu_sign(sign: DashuSign, is_zero: bool) -> Sign {
    if is_zero {
        Sign::NoSign
    } else {
        match sign {
            DashuSign::Positive => Sign::Plus,
            DashuSign::Negative => Sign::Minus,
        }
    }
}

impl Zero for BigInt {
    fn zero() -> Self {
        BigInt(IBig::ZERO)
    }
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl Zero for BigUint {
    fn zero() -> Self {
        BigUint(UBig::ZERO)
    }
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl Signed for BigInt {
    fn abs(&self) -> Self {
        BigInt(IBig::from(self.0.clone().into_parts().1))
    }
}

macro_rules! impl_to_primitive {
    ($wrapper:ident) => {
        impl ToPrimitive for $wrapper {
            fn to_i8(&self) -> Option<i8> {
                i8::try_from(&self.0).ok()
            }
            fn to_i16(&self) -> Option<i16> {
                i16::try_from(&self.0).ok()
            }
            fn to_i32(&self) -> Option<i32> {
                i32::try_from(&self.0).ok()
            }
            fn to_i64(&self) -> Option<i64> {
                i64::try_from(&self.0).ok()
            }
            fn to_i128(&self) -> Option<i128> {
                i128::try_from(&self.0).ok()
            }
            fn to_u8(&self) -> Option<u8> {
                u8::try_from(&self.0).ok()
            }
            fn to_u16(&self) -> Option<u16> {
                u16::try_from(&self.0).ok()
            }
            fn to_u32(&self) -> Option<u32> {
                u32::try_from(&self.0).ok()
            }
            fn to_u64(&self) -> Option<u64> {
                u64::try_from(&self.0).ok()
            }
            fn to_u128(&self) -> Option<u128> {
                u128::try_from(&self.0).ok()
            }
            fn to_f64(&self) -> Option<f64> {
                Some(self.0.to_f64().value())
            }
        }
    };
}
impl_to_primitive!(BigInt);
impl_to_primitive!(BigUint);

macro_rules! impl_from_int {
    ($wrapper:ident, $inner:ident, $($t:ty),*) => {
        $(
            impl From<$t> for $wrapper {
                fn from(v: $t) -> Self {
                    $wrapper($inner::from(v))
                }
            }
        )*
    };
}
impl_from_int!(
    BigInt, IBig, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);
impl_from_int!(BigUint, UBig, u8, u16, u32, u64, u128);

impl From<BigUint> for BigInt {
    fn from(v: BigUint) -> Self {
        BigInt(IBig::from(v.0))
    }
}

macro_rules! impl_try_into_native {
    ($($t:ty),*) => {
        $(
            impl TryFrom<&BigInt> for $t {
                type Error = ();
                fn try_from(v: &BigInt) -> Result<$t, ()> {
                    <$t>::try_from(&v.0).map_err(|_| ())
                }
            }
        )*
    };
}
impl_try_into_native!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

macro_rules! impl_try_from_owned_bigint {
    ($($t:ty),*) => {
        $(
            impl TryFrom<BigInt> for $t {
                type Error = ();
                fn try_from(v: BigInt) -> Result<$t, ()> {
                    <$t>::try_from(&v)
                }
            }
        )*
    };
}
impl_try_from_owned_bigint!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

macro_rules! impl_try_into_native_uint {
    ($($t:ty),*) => {
        $(
            impl TryFrom<BigUint> for $t {
                type Error = ();
                fn try_from(v: BigUint) -> Result<$t, ()> {
                    <$t>::try_from(v.0).map_err(|_| ())
                }
            }
            impl TryFrom<&BigUint> for $t {
                type Error = ();
                fn try_from(v: &BigUint) -> Result<$t, ()> {
                    <$t>::try_from(&v.0).map_err(|_| ())
                }
            }
        )*
    };
}
impl_try_into_native_uint!(u8, u16, u32, u64, u128);

// --- Arithmetic operators -------------------------------------------------
//
// Each binary op is forwarded for all four owned/borrowed operand combinations
// (cloning into the owned `dashu` op). `BigInt`/`BigUint` only appear on cold
// paths — every native-width draw stays in `u128` — so the clones never touch
// the generation hot loop.

macro_rules! impl_binop {
    ($wrapper:ident, $trait:ident, $method:ident) => {
        impl ::core::ops::$trait for $wrapper {
            type Output = $wrapper;
            fn $method(self, rhs: $wrapper) -> $wrapper {
                $wrapper(::core::ops::$trait::$method(self.0, rhs.0))
            }
        }
        impl ::core::ops::$trait<&$wrapper> for $wrapper {
            type Output = $wrapper;
            fn $method(self, rhs: &$wrapper) -> $wrapper {
                $wrapper(::core::ops::$trait::$method(self.0, rhs.0.clone()))
            }
        }
        impl ::core::ops::$trait<$wrapper> for &$wrapper {
            type Output = $wrapper;
            fn $method(self, rhs: $wrapper) -> $wrapper {
                $wrapper(::core::ops::$trait::$method(self.0.clone(), rhs.0))
            }
        }
        impl ::core::ops::$trait<&$wrapper> for &$wrapper {
            type Output = $wrapper;
            fn $method(self, rhs: &$wrapper) -> $wrapper {
                $wrapper(::core::ops::$trait::$method(self.0.clone(), rhs.0.clone()))
            }
        }
    };
}

impl_binop!(BigInt, Add, add);
impl_binop!(BigInt, Sub, sub);
impl_binop!(BigInt, Mul, mul);
impl_binop!(BigUint, Add, add);
impl_binop!(BigUint, Sub, sub);
impl_binop!(BigUint, Mul, mul);
impl_binop!(BigUint, Div, div);
impl_binop!(BigUint, Rem, rem);

// In-place assignment forms, each reusing the borrowing binary op above.
impl ::core::ops::AddAssign<i32> for BigInt {
    fn add_assign(&mut self, rhs: i32) {
        *self = &*self + rhs;
    }
}
impl ::core::ops::AddAssign for BigUint {
    fn add_assign(&mut self, rhs: BigUint) {
        *self = &*self + rhs;
    }
}
impl ::core::ops::SubAssign for BigUint {
    fn sub_assign(&mut self, rhs: BigUint) {
        *self = &*self - rhs;
    }
}
impl ::core::ops::MulAssign for BigUint {
    fn mul_assign(&mut self, rhs: BigUint) {
        *self = &*self * rhs;
    }
}
impl ::core::ops::DivAssign<&BigUint> for BigUint {
    fn div_assign(&mut self, rhs: &BigUint) {
        *self = &*self / rhs;
    }
}

// `BigInt ± i32`, for the `value ± 1` steps the choice arithmetic performs.
macro_rules! impl_binop_i32 {
    ($trait:ident, $method:ident) => {
        impl ::core::ops::$trait<i32> for BigInt {
            type Output = BigInt;
            fn $method(self, rhs: i32) -> BigInt {
                BigInt(::core::ops::$trait::$method(self.0, IBig::from(rhs)))
            }
        }
        impl ::core::ops::$trait<i32> for &BigInt {
            type Output = BigInt;
            fn $method(self, rhs: i32) -> BigInt {
                BigInt(::core::ops::$trait::$method(
                    self.0.clone(),
                    IBig::from(rhs),
                ))
            }
        }
    };
}
impl_binop_i32!(Add, add);
impl_binop_i32!(Sub, sub);
impl_binop_i32!(Div, div);

impl ::core::ops::Neg for BigInt {
    type Output = BigInt;
    fn neg(self) -> BigInt {
        BigInt(-self.0)
    }
}
impl ::core::ops::Neg for &BigInt {
    type Output = BigInt;
    fn neg(self) -> BigInt {
        BigInt(-self.0.clone())
    }
}

macro_rules! impl_shift {
    ($wrapper:ident, $trait:ident, $method:ident, $rhs:ty) => {
        impl ::core::ops::$trait<$rhs> for $wrapper {
            type Output = $wrapper;
            fn $method(self, rhs: $rhs) -> $wrapper {
                $wrapper(::core::ops::$trait::$method(self.0, rhs as usize))
            }
        }
        impl ::core::ops::$trait<$rhs> for &$wrapper {
            type Output = $wrapper;
            fn $method(self, rhs: $rhs) -> $wrapper {
                $wrapper(::core::ops::$trait::$method(self.0.clone(), rhs as usize))
            }
        }
    };
}
impl_shift!(BigInt, Shr, shr, usize);
impl_shift!(BigUint, Shr, shr, u32);
impl_shift!(BigUint, Shl, shl, usize);

#[cfg(test)]
#[path = "../../tests/embedded/native/bignum_tests.rs"]
mod tests;

use super::{Generator, TestCase};
use crate::test_case::invalid_argument;
use std::marker::PhantomData;
use std::sync::OnceLock;

/// Trait bound for integer types usable with [`integers()`].
pub trait Integer: Copy + Ord {
    /// The minimum value of this type.
    const MIN: Self;
    /// The maximum value of this type.
    const MAX: Self;
    /// This value as an `i64`, when it fits.
    #[doc(hidden)]
    fn to_i64_checked(self) -> Option<i64>;
    /// This value's two's-complement little-endian encoding, sign-extended
    /// to 17 bytes (wide enough for any `i128` or `u128`).
    #[doc(hidden)]
    fn to_le17(self) -> [u8; 17];
    /// Decode a 17-byte sign-extended two's-complement little-endian
    /// encoding. The value is guaranteed by the caller to be in this type's
    /// range.
    #[doc(hidden)]
    fn from_le17(bytes: [u8; 17]) -> Self;
    /// Convert from an `i64` guaranteed by the caller to be in this type's
    /// range.
    #[doc(hidden)]
    fn from_i64(v: i64) -> Self;
}

macro_rules! impl_signed_integer_type {
    ($($t:ty),*) => { $(
        impl Integer for $t {
            const MIN: Self = <$t>::MIN;
            const MAX: Self = <$t>::MAX;
            fn to_i64_checked(self) -> Option<i64> {
                i64::try_from(self).ok()
            }
            fn to_le17(self) -> [u8; 17] {
                let v = self as i128;
                let fill = if v < 0 { 0xFF } else { 0x00 };
                let mut bytes = [fill; 17];
                bytes[..16].copy_from_slice(&v.to_le_bytes());
                bytes
            }
            fn from_le17(bytes: [u8; 17]) -> Self {
                i128::from_le_bytes(bytes[..16].try_into().unwrap()) as $t
            }
            fn from_i64(v: i64) -> Self {
                v as $t
            }
        }
    )* };
}

macro_rules! impl_unsigned_integer_type {
    ($($t:ty),*) => { $(
        impl Integer for $t {
            const MIN: Self = <$t>::MIN;
            const MAX: Self = <$t>::MAX;
            fn to_i64_checked(self) -> Option<i64> {
                i64::try_from(self).ok()
            }
            fn to_le17(self) -> [u8; 17] {
                let v = self as u128;
                let mut bytes = [0u8; 17];
                bytes[..16].copy_from_slice(&v.to_le_bytes());
                bytes
            }
            fn from_le17(bytes: [u8; 17]) -> Self {
                u128::from_le_bytes(bytes[..16].try_into().unwrap()) as $t
            }
            fn from_i64(v: i64) -> Self {
                v as $t
            }
        }
    )* };
}

impl_signed_integer_type!(i8, i16, i32, i64, i128, isize);
impl_unsigned_integer_type!(u8, u16, u32, u64, u128, usize);

/// Trait bound for float types usable with [`floats()`].
pub trait Float: Copy + PartialOrd {
    /// The minimum value of this type.
    const MIN: Self;
    /// The maximum value of this type.
    const MAX: Self;
    /// Widen to f64 for cross-width comparisons (bound validation).
    fn to_f64(self) -> f64;
    /// Narrow from the engine's f64 result. The value is guaranteed to be
    /// exactly representable at this type's width.
    #[doc(hidden)]
    fn from_f64(v: f64) -> Self;
}

impl Float for f32 {
    const MIN: Self = f32::MIN;
    const MAX: Self = f32::MAX;
    fn to_f64(self) -> f64 {
        self as f64
    }
    fn from_f64(v: f64) -> Self {
        v as f32
    }
}

impl Float for f64 {
    const MIN: Self = f64::MIN;
    const MAX: Self = f64::MAX;
    fn to_f64(self) -> f64 {
        self
    }
    fn from_f64(v: f64) -> Self {
        v
    }
}

/// Less-than-or-equal under sign-aware ordering, where `-0.0 < +0.0`.
///
/// IEEE 754 considers `+0.0` and `-0.0` equal under `<=`, but Hypothesis and
/// the native backend treat `-0.0` as strictly less than `+0.0`. Mirrors
/// `sign_aware_lte` in hypothesis (`strategies/_internal/numbers.py`) and
/// `native/core/choices.rs`.
pub(crate) fn sign_aware_lte<T: Float>(a: T, b: T) -> bool {
    let a = a.to_f64();
    let b = b.to_f64();
    if a == 0.0 && b == 0.0 {
        a.is_sign_negative() || b.is_sign_positive()
    } else {
        a <= b
    }
}

/// Generator for integer values. Created by [`integers()`].
///
/// Bounds default to the type's full range.
pub struct IntegerGenerator<T> {
    min: Option<T>,
    max: Option<T>,
    _phantom: PhantomData<T>,
}

impl<T> IntegerGenerator<T> {
    /// Set the minimum value (inclusive).
    pub fn min_value(mut self, min_value: T) -> Self {
        self.min = Some(min_value);
        self
    }

    /// Set the maximum value (inclusive).
    pub fn max_value(mut self, max_value: T) -> Self {
        self.max = Some(max_value);
        self
    }
}

impl<T: Integer> Generator<T> for IntegerGenerator<T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        let min = self.min.unwrap_or(T::MIN);
        let max = self.max.unwrap_or(T::MAX);
        if min > max {
            invalid_argument!("Cannot have max_value < min_value");
        }
        match (min.to_i64_checked(), max.to_i64_checked()) {
            (Some(lo), Some(hi)) => T::from_i64(tc.generate_integer_i64(lo, hi)),
            _ => T::from_le17(tc.generate_integer_le17(&min.to_le17(), &max.to_le17())),
        }
    }
}

/// Generate integers of type `T`.
///
/// Bounds default to the full range of `T`. Use the builder methods `min_value`
/// and `max_value` to constrain the range. See [`IntegerGenerator`] for more
/// details.
pub fn integers<T: Integer>() -> IntegerGenerator<T> {
    IntegerGenerator {
        min: None,
        max: None,
        _phantom: PhantomData,
    }
}

/// Generator for floating-point values. Created by [`floats()`].
///
/// By default, may produce NaN and infinity when no bounds are set.
/// Setting bounds automatically disables these unless re-enabled.
pub struct FloatGenerator<T> {
    min: Option<T>,
    max: Option<T>,
    exclude_min: bool,
    exclude_max: bool,
    allow_nan: Option<bool>,
    allow_infinity: Option<bool>,
    allow_subnormal: Option<bool>,
    params: OnceLock<FloatDrawParams>,
}

impl<T> FloatGenerator<T> {
    /// Set the minimum value (inclusive by default).
    pub fn min_value(mut self, min_value: T) -> Self {
        self.min = Some(min_value);
        self.params = OnceLock::new();
        self
    }

    /// Set the maximum value (inclusive by default).
    pub fn max_value(mut self, max_value: T) -> Self {
        self.max = Some(max_value);
        self.params = OnceLock::new();
        self
    }

    /// Set whether to exclude the minimum value from the range.
    pub fn exclude_min(mut self, exclude_min: bool) -> Self {
        self.exclude_min = exclude_min;
        self.params = OnceLock::new();
        self
    }

    /// Set whether to exclude the maximum value from the range.
    pub fn exclude_max(mut self, exclude_max: bool) -> Self {
        self.exclude_max = exclude_max;
        self.params = OnceLock::new();
        self
    }

    /// Whether NaN values are allowed. Cannot be used with bounds.
    pub fn allow_nan(mut self, allow: bool) -> Self {
        self.allow_nan = Some(allow);
        self.params = OnceLock::new();
        self
    }

    /// Whether infinite values are allowed. Cannot be used with both bounds set.
    pub fn allow_infinity(mut self, allow: bool) -> Self {
        self.allow_infinity = Some(allow);
        self.params = OnceLock::new();
        self
    }

    /// Whether subnormal ("denormalised") values are allowed. Defaults to
    /// allowing them whenever the bounds admit any; set to `false` when the
    /// code under test may run with flush-to-zero floating point (e.g.
    /// compiled with `-ffast-math`), where subnormal inputs silently become
    /// zero.
    pub fn allow_subnormal(mut self, allow: bool) -> Self {
        self.allow_subnormal = Some(allow);
        self.params = OnceLock::new();
        self
    }
}

/// The validated parameters of a float draw, in the form
/// `TestCase::generate_float` accepts.
struct FloatDrawParams {
    width: u32,
    min_value: f64,
    max_value: f64,
    allow_nan: bool,
    allow_infinity: bool,
    smallest_nonzero_magnitude: f64,
}

impl<T: Float> FloatGenerator<T> {
    fn draw_params(&self) -> FloatDrawParams {
        let width = (std::mem::size_of::<T>() * 8) as u32;
        let has_min = self.min.is_some();
        let has_max = self.max.is_some();

        if let (Some(min), Some(max)) = (self.min, self.max) {
            if !matches!(
                min.partial_cmp(&max),
                Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
            ) {
                invalid_argument!("Cannot have max_value < min_value");
            }
            if !sign_aware_lte(min, max) {
                invalid_argument!(
                    "InvalidArgument: There are no {width}-bit floating-point \
                     values between min_value=0.0 and max_value=-0.0"
                );
            }
            let min_f = min.to_f64();
            let max_f = max.to_f64();
            let zero_pair = min_f == 0.0 && max_f == 0.0;
            if (min_f == max_f || zero_pair) && (self.exclude_min || self.exclude_max) {
                invalid_argument!(
                    "InvalidArgument: exclude_min/exclude_max leave no \
                     {width}-bit floating-point values in [{min_f}, {max_f}]"
                );
            }
        }

        if self.exclude_min && !has_min {
            invalid_argument!("InvalidArgument: Cannot have exclude_min=true without min_value");
        }
        if self.exclude_max && !has_max {
            invalid_argument!("InvalidArgument: Cannot have exclude_max=true without max_value");
        }

        if self.exclude_min && self.min.is_some_and(|v| v.to_f64() == f64::INFINITY) {
            invalid_argument!(
                "InvalidArgument: exclude_min=true with min_value=+inf leaves \
                 no {width}-bit floating-point values"
            );
        }
        if self.exclude_max && self.max.is_some_and(|v| v.to_f64() == f64::NEG_INFINITY) {
            invalid_argument!(
                "InvalidArgument: exclude_max=true with max_value=-inf leaves \
                 no {width}-bit floating-point values"
            );
        }

        let allow_nan = self.allow_nan.unwrap_or(!has_min && !has_max);
        let allow_infinity = self.allow_infinity.unwrap_or(!has_min || !has_max);

        if allow_nan && (has_min || has_max) {
            invalid_argument!("Cannot have allow_nan=true with min_value or max_value");
        }
        if allow_infinity && has_min && has_max {
            invalid_argument!("Cannot have allow_infinity=true with both min_value and max_value");
        }

        let smallest_normal = if width == 32 {
            f32::MIN_POSITIVE as f64
        } else {
            f64::MIN_POSITIVE
        };
        let min_f = self.min.map(|v| v.to_f64());
        let max_f = self.max.map(|v| v.to_f64());
        let allow_subnormal = self.allow_subnormal.unwrap_or(match (min_f, max_f) {
            (Some(lo), Some(hi)) if lo == hi => -smallest_normal < lo && lo < smallest_normal,
            (Some(lo), Some(hi)) => lo < smallest_normal && hi > -smallest_normal,
            (Some(lo), None) => lo < smallest_normal,
            (None, Some(hi)) => hi > -smallest_normal,
            (None, None) => true,
        });
        if allow_subnormal {
            if min_f.is_some_and(|lo| lo >= smallest_normal) {
                invalid_argument!(
                    "InvalidArgument: allow_subnormal=true, but min_value excludes \
                     all values below the smallest positive normal {smallest_normal}"
                );
            }
            if max_f.is_some_and(|hi| hi <= -smallest_normal) {
                invalid_argument!(
                    "InvalidArgument: allow_subnormal=true, but max_value excludes \
                     all values above the smallest negative normal -{smallest_normal}"
                );
            }
        } else if let (Some(lo), Some(hi)) = (min_f, max_f) {
            let contains_zero = lo <= 0.0 && hi >= 0.0;
            if !contains_zero && hi < smallest_normal && lo > -smallest_normal {
                invalid_argument!(
                    "InvalidArgument: allow_subnormal=false leaves no {width}-bit \
                     floating-point values in [{lo}, {hi}]"
                );
            }
        }

        let bounded_default = !allow_nan && !allow_infinity;
        let min_value = match min_f {
            Some(lo) => lo,
            None if bounded_default => T::MIN.to_f64(),
            None => f64::NEG_INFINITY,
        };
        let max_value = match max_f {
            Some(hi) => hi,
            None if bounded_default => T::MAX.to_f64(),
            None => f64::INFINITY,
        };

        let smallest_nonzero_magnitude = if allow_subnormal {
            if width == 32 {
                f64::from(f32::from_bits(1))
            } else {
                f64::from_bits(1)
            }
        } else {
            smallest_normal
        };

        FloatDrawParams {
            width,
            min_value,
            max_value,
            allow_nan,
            allow_infinity,
            smallest_nonzero_magnitude,
        }
    }
}

impl<T: Float> Generator<T> for FloatGenerator<T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        let params = self.params.get_or_init(|| self.draw_params());
        let v = tc.generate_float(
            params.width,
            params.min_value,
            params.max_value,
            params.allow_nan,
            params.allow_infinity,
            self.exclude_min,
            self.exclude_max,
            params.smallest_nonzero_magnitude,
        );
        T::from_f64(v)
    }
}

/// Generate floating-point values of type `T`.
/// Use the builder methods `min_value`, `max_value`, `allow_nan`, and
/// `allow_infinity` to constrain the output. By default, may produce NaN and
/// infinity. See [`FloatGenerator`] for more details.
///
/// # Example
///
/// ```no_run
/// use hegel::generators as gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let x: f64 = tc.draw(gs::floats()
///         .min_value(0.0)
///         .max_value(1.0));
///     assert!((0.0..=1.0).contains(&x));
/// }
/// ```
pub fn floats<T: Float>() -> FloatGenerator<T> {
    FloatGenerator {
        min: None,
        max: None,
        exclude_min: false,
        exclude_max: false,
        allow_nan: None,
        allow_infinity: None,
        allow_subnormal: None,
        params: OnceLock::new(),
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/generators/numeric_tests.rs"]
mod tests;

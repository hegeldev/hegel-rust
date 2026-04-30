use super::{BasicGenerator, Generator, TestCase};
use crate::utils::cbor_utils::{cbor_map, cbor_serialize};
use crate::utils::num::{cbor_to_bigint, cbor_to_biguint, int_to_cbor};
use ciborium::Value;
use num_bigint::{BigInt, BigUint};
use num_complex::Complex;
use num_integer::Integer as NumInteger;
use num_rational::Ratio;
use num_traits::{CheckedMul, One, Zero};

// ---------------------------------------------------------------------------
// Integer impls for BigInt/BigUint
// ---------------------------------------------------------------------------

impl super::Integer for BigInt {
    fn default_min() -> Self {
        -(<BigInt as One>::one() << 128u32)
    }
    fn default_max() -> Self {
        (<BigInt as One>::one() << 128u32) - <BigInt as One>::one()
    }
    fn one() -> Self {
        <BigInt as One>::one()
    }
    fn to_cbor(&self) -> Value {
        int_to_cbor(self.clone())
    }
    fn from_cbor(v: Value) -> Self {
        cbor_to_bigint(v)
    }
}

impl super::Integer for BigUint {
    fn default_min() -> Self {
        BigUint::zero()
    }
    fn default_max() -> Self {
        (<BigUint as One>::one() << 128u32) - <BigUint as One>::one()
    }
    fn one() -> Self {
        <BigUint as One>::one()
    }
    fn to_cbor(&self) -> Value {
        int_to_cbor(BigInt::from(self.clone()))
    }
    fn from_cbor(v: Value) -> Self {
        cbor_to_biguint(v)
    }
}

// ---------------------------------------------------------------------------
// RationalGenerator
// ---------------------------------------------------------------------------

/// Generator for [`Ratio<T>`] values. Created by [`rationals()`].
///
/// The server draws a fraction within the configured value range and with a
/// denominator up to `max_denominator`, then returns it reduced to lowest terms.
/// Use `.min_value()`, `.max_value()`, and `.max_denominator()` to constrain the output.
pub struct RationalGenerator<T> {
    min: Option<T>,
    max: Option<T>,
    max_denom: Option<T>,
}

impl<T> RationalGenerator<T> {
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

    /// Set the maximum allowed denominator (inclusive).
    pub fn max_denominator(mut self, max_denom: T) -> Self {
        self.max_denom = Some(max_denom);
        self
    }
}

impl<T: NumInteger + super::Integer + CheckedMul> RationalGenerator<T> {
    fn build_schema(&self) -> Value {
        let min = self.min.clone().unwrap_or_else(T::rational_default_min);
        let max = self.max.clone().unwrap_or_else(T::rational_default_max);
        let max_denom = self
            .max_denom
            .clone()
            .unwrap_or_else(T::rational_default_max);

        assert!(
            max_denom >= <T as super::Integer>::one(),
            "max_denominator must be >= 1"
        );
        assert!(
            max.checked_mul(&max_denom).is_some(),
            "max_value * max_denominator overflows the numerator type"
        );
        assert!(
            min.checked_mul(&max_denom).is_some(),
            "min_value * max_denominator overflows the numerator type"
        );

        cbor_map! {
            "type" => "fraction",
            "min_value" => min.to_cbor(),
            "max_value" => max.to_cbor(),
            "max_denominator" => max_denom.to_cbor()
        }
    }
}

fn parse_ratio<T: super::Integer + NumInteger>(v: Value) -> Ratio<T> {
    let Value::Array(items) = v else {
        panic!("expected Array for rational, got {v:?}"); // nocov
    };
    let mut iter = items.into_iter();
    let numer = T::from_cbor(iter.next().unwrap());
    let denom = T::from_cbor(iter.next().unwrap());
    Ratio::new(numer, denom)
}

impl<T> Generator<Ratio<T>> for RationalGenerator<T>
where
    T: super::Integer + NumInteger + CheckedMul,
{
    fn do_draw(&self, tc: &TestCase) -> Ratio<T> {
        parse_ratio::<T>(super::generate_raw(tc, &self.build_schema()))
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Ratio<T>>> {
        Some(BasicGenerator::new(self.build_schema(), parse_ratio::<T>))
    }
}

/// Generate [`Ratio<T>`] values.
///
/// Bounds default to a safe range derived from `T` so the numerator
/// (= value × denominator) cannot overflow. Use `.min_value()`,
/// `.max_value()`, and `.max_denominator()` to constrain further.
///
/// # Examples
///
/// ```no_run
/// use num_rational::Ratio;
/// use hegel::generators::{self as gs, Generator};
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let r: Ratio<i64> = tc.draw(gs::rationals::<i64>());
///     assert!(*r.denom() > 0);
///
///     // Constrain to a non-negative range with small denominators
///     let r: Ratio<i64> = tc.draw(gs::rationals::<i64>()
///         .min_value(0)
///         .max_value(100)
///         .max_denominator(10));
///     assert!(*r.numer() >= 0 && *r.denom() >= 1 && *r.denom() <= 10);
/// }
/// ```
pub fn rationals<T: super::Integer>() -> RationalGenerator<T> {
    RationalGenerator {
        min: None,
        max: None,
        max_denom: None,
    }
}

// ---------------------------------------------------------------------------
// ComplexGenerator
// ---------------------------------------------------------------------------

/// Generator for [`Complex<T>`] values. Created by [`complex()`].
///
/// The server returns complex numbers as a CBOR tag wrapping `[re, im]`.
/// The schema sends `width = 2 × bits(T)`, so f32 components use width=64
/// and f64 use 128. By default, NaN and infinity are disallowed.
pub struct ComplexGenerator<T> {
    min_magnitude: Option<T>,
    max_magnitude: Option<T>,
    allow_nan: bool,
    allow_infinity: bool,
    allow_subnormal: bool,
}

impl<T> ComplexGenerator<T> {
    /// Set the minimum magnitude (inclusive).
    pub fn min_magnitude(mut self, min: T) -> Self {
        self.min_magnitude = Some(min);
        self
    }

    /// Set the maximum magnitude (inclusive).
    pub fn max_magnitude(mut self, max: T) -> Self {
        self.max_magnitude = Some(max);
        self
    }

    /// Whether NaN values are allowed in either component.
    pub fn allow_nan(mut self, allow: bool) -> Self {
        self.allow_nan = allow;
        self
    }

    /// Whether infinite values are allowed in either component.
    pub fn allow_infinity(mut self, allow: bool) -> Self {
        self.allow_infinity = allow;
        self
    }

    /// Whether subnormal values are allowed in either component.
    pub fn allow_subnormal(mut self, allow: bool) -> Self {
        self.allow_subnormal = allow;
        self
    }
}

impl<T: super::Float + serde::Serialize> ComplexGenerator<T> {
    fn build_schema(&self) -> Value {
        let min = self.min_magnitude.unwrap_or(T::ZERO);
        let has_max = self.max_magnitude.is_some();

        if min < T::ZERO || self.max_magnitude.is_some_and(|m| m < T::ZERO) {
            panic!("min_magnitude and max_magnitude must be non-negative");
        }
        if self.allow_nan && (min != T::ZERO || has_max) {
            panic!("Cannot have allow_nan=true with min_magnitude > 0 or max_magnitude set");
        }
        if self.allow_infinity && has_max {
            panic!("Cannot have allow_infinity=true with max_magnitude set");
        }

        let max = self.max_magnitude.unwrap_or(if self.allow_infinity {
            T::INFINITY
        } else {
            T::MAX
        });

        cbor_map! {
            "type" => "complex",
            "min_magnitude" => cbor_serialize(&min),
            "max_magnitude" => cbor_serialize(&max),
            "allow_nan" => self.allow_nan,
            "allow_infinity" => self.allow_infinity,
            "allow_subnormal" => self.allow_subnormal,
            "width" => u64::from(2 * T::WIDTH),
        }
    }
}
// jelo :)
fn parse_complex<T: serde::de::DeserializeOwned>(v: Value) -> Complex<T> {
    let Value::Array(items) = v else {
        panic!("expected Array for complex, got {v:?}"); // nocov
    };
    let mut iter = items.into_iter();
    let real: T = super::deserialize_value(iter.next().unwrap());
    let imaginary: T = super::deserialize_value(iter.next().unwrap());
    Complex::new(real, imaginary)
}

impl<T> Generator<Complex<T>> for ComplexGenerator<T>
where
    T: super::Float + serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    fn do_draw(&self, tc: &TestCase) -> Complex<T> {
        parse_complex::<T>(super::generate_raw(tc, &self.build_schema()))
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Complex<T>>> {
        Some(BasicGenerator::new(self.build_schema(), parse_complex::<T>))
    }
}

/// Generate [`Complex<T>`] values for `T = f32` or `T = f64`.
///
/// # Example
///
/// ```no_run
/// use num_complex::Complex;
/// use hegel::generators::{self as gs, Generator};
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let c: Complex<f64> = tc.draw(gs::complex::<f64>().max_magnitude(100.0));
///     assert!(c.re * c.re + c.im * c.im <= 100.0 * 100.0);
/// }
/// ```
pub fn complex<T: super::Float>() -> ComplexGenerator<T> {
    ComplexGenerator {
        min_magnitude: None,
        max_magnitude: None,
        allow_nan: false,
        allow_infinity: false,
        allow_subnormal: true,
    }
}

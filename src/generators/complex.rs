use super::{BasicGenerator, Generator, TestCase};
use crate::utils::cbor_utils::{cbor_map, cbor_serialize};
use ciborium::Value;
use num_complex::Complex;

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

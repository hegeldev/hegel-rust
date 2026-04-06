use super::{BasicGenerator, Generator, TestCase};
use crate::utils::cbor_utils::cbor_map;
use crate::utils::num::{cbor_to_bigint, cbor_to_biguint, int_to_cbor};
use ciborium::Value;
use num_bigint::{BigInt, BigUint};
use num_complex::Complex;
use num_integer::Integer;
use num_rational::Ratio;
use num_traits::{Num, One, Zero};

// ---------------------------------------------------------------------------
// BigIntGenerator
// ---------------------------------------------------------------------------

/// Generator for [`BigInt`] values. Created by [`big_integers()`].
///
/// Defaults to the range \[-2^128, 2^128).
/// Use `min_value` and `max_value` to constrain the range.
pub struct BigIntGenerator {
    min: BigInt,
    max: BigInt,
}

impl BigIntGenerator {
    /// Set the minimum value (inclusive).
    pub fn min_value(mut self, min: BigInt) -> Self {
        self.min = min;
        self
    }

    /// Set the maximum value (inclusive).
    pub fn max_value(mut self, max: BigInt) -> Self {
        self.max = max;
        self
    }

    fn build_schema(&self) -> Value {
        assert!(self.min <= self.max, "Cannot have max_value < min_value");
        cbor_map! {
            "type" => "integer",
            "min_value" => int_to_cbor(self.min.clone()),
            "max_value" => int_to_cbor(self.max.clone())
        }
    }
}

impl Generator<BigInt> for BigIntGenerator {
    fn do_draw(&self, tc: &TestCase) -> BigInt {
        let raw = super::generate_raw(tc, &self.build_schema());
        cbor_to_bigint(raw)
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, BigInt>> {
        Some(BasicGenerator::new(self.build_schema(), cbor_to_bigint))
    }
}

/// Generate [`BigInt`] values.
///
/// By default, generates values in \[-2^128, 2^128).
/// Use `min_value` and `max_value` to constrain the range.
///
/// # Example
///
/// ```no_run
/// use num_bigint::BigInt;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let n = tc.draw(hegel::generators::big_integers()
///         .max_value(BigInt::from(1000)));
///     assert!(n <= BigInt::from(1000));
/// }
/// ```
pub fn big_integers() -> BigIntGenerator {
    BigIntGenerator {
        min: -(BigInt::one() << 128u32),
        max: (BigInt::one() << 128u32) - BigInt::one(),
    }
}

// ---------------------------------------------------------------------------
// BigUintGenerator
// ---------------------------------------------------------------------------

/// Generator for [`BigUint`] values. Created by [`big_uintegers()`].
///
/// Defaults to the range \[0, 2^128).
/// Use `min_value` and `max_value` to constrain the range.
pub struct BigUintGenerator {
    min: BigUint,
    max: BigUint,
}

impl BigUintGenerator {
    /// Set the minimum value (inclusive).
    pub fn min_value(mut self, min: BigUint) -> Self {
        self.min = min;
        self
    }

    /// Set the maximum value (inclusive).
    pub fn max_value(mut self, max: BigUint) -> Self {
        self.max = max;
        self
    }

    fn build_schema(&self) -> Value {
        assert!(self.min <= self.max, "Cannot have max_value < min_value");
        cbor_map! {
            "type" => "integer",
            "min_value" => int_to_cbor(self.min.clone()),
            "max_value" => int_to_cbor(self.max.clone())
        }
    }
}

impl Generator<BigUint> for BigUintGenerator {
    fn do_draw(&self, tc: &TestCase) -> BigUint {
        let raw = super::generate_raw(tc, &self.build_schema());
        cbor_to_biguint(raw)
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, BigUint>> {
        Some(BasicGenerator::new(self.build_schema(), cbor_to_biguint))
    }
}

/// Generate [`BigUint`] values.
///
/// By default, generates values in \[0, 2^128).
/// Use `min_value` and `max_value` to constrain the range.
///
/// # Example
///
/// ```no_run
/// use num_bigint::BigUint;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let n = tc.draw(hegel::generators::big_uintegers()
///         .max_value(BigUint::from(1000u32)));
///     assert!(n <= BigUint::from(1000u32));
/// }
/// ```
pub fn big_uintegers() -> BigUintGenerator {
    BigUintGenerator {
        min: BigUint::zero(),
        max: (BigUint::one() << 128u32) - BigUint::one(),
    }
}

// ---------------------------------------------------------------------------
// RationalGenerator
// ---------------------------------------------------------------------------

/// Generator for [`Ratio<T>`] values. Created by [`rationals()`].
///
/// Generates a numerator and denominator independently, with the denominator
/// constrained to be non-zero. The resulting `Ratio` is automatically reduced
/// to lowest terms by `Ratio::new()`.
pub struct RationalGenerator<NG, DG> {
    numer_gen: NG,
    denom_gen: DG,
}

impl<NG, DG> RationalGenerator<NG, DG> {
    /// Set the numerator generator.
    pub fn numerator<NG2>(self, numer_gen: NG2) -> RationalGenerator<NG2, DG> {
        RationalGenerator {
            numer_gen,
            denom_gen: self.denom_gen,
        }
    }

    /// Set the denominator generator. Must not produce zero.
    pub fn denominator<DG2>(self, denom_gen: DG2) -> RationalGenerator<NG, DG2> {
        RationalGenerator {
            numer_gen: self.numer_gen,
            denom_gen,
        }
    }
}

impl<T, NG, DG> Generator<Ratio<T>> for RationalGenerator<NG, DG>
where
    NG: Generator<T>,
    DG: Generator<T>,
    T: Clone + Integer,
{
    fn do_draw(&self, tc: &TestCase) -> Ratio<T> {
        let numer = self.numer_gen.do_draw(tc);
        let denom = self.denom_gen.do_draw(tc);
        Ratio::new(numer, denom)
    }
}

/// Generate [`Ratio<i64>`] values.
///
/// By default, uses `integers::<i64>()` for the numerator and
/// `integers::<i64>().min_value(1)` for the denominator.
/// Use `.numerator()` and `.denominator()` to customize.
///
/// # Examples
///
/// ```no_run
/// use num_rational::Ratio;
/// use hegel::generators::{self as gs, Generator};
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     // Use defaults
///     let r: Ratio<i64> = tc.draw(gs::rationals());
///     assert!(*r.denom() > 0);
///
///     // Customize numerator and denominator ranges
///     let r: Ratio<i64> = tc.draw(gs::rationals()
///         .numerator(gs::integers::<i64>().min_value(0).max_value(100))
///         .denominator(gs::integers::<i64>().min_value(1).max_value(10)));
///     assert!(*r.numer() >= 0 && *r.denom() >= 1);
/// }
/// ```
pub fn rationals() -> RationalGenerator<super::IntegerGenerator<i64>, super::IntegerGenerator<i64>>
{
    RationalGenerator {
        numer_gen: super::integers::<i64>(),
        denom_gen: super::integers::<i64>().min_value(1),
    }
}

/// Generate [`Ratio<BigInt>`] values.
///
/// Uses [`big_integers()`] for the numerator and a strictly-positive
/// range for the denominator.
///
/// # Example
///
/// ```no_run
/// use num_bigint::BigInt;
/// use num_rational::Ratio;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let r: Ratio<BigInt> = tc.draw(hegel::generators::big_rationals());
///     assert!(!r.denom().is_zero());
/// }
/// ```
pub fn big_rationals() -> RationalGenerator<BigIntGenerator, BigIntGenerator> {
    RationalGenerator {
        numer_gen: big_integers(),
        denom_gen: big_integers().min_value(BigInt::one()),
    }
}

// ---------------------------------------------------------------------------
// ComplexGenerator
// ---------------------------------------------------------------------------

/// Generator for [`Complex<T>`] values. Created by [`complex()`].
///
/// Draws the real and imaginary parts from separate generators.
pub struct ComplexGenerator<G> {
    real_gen: G,
    imag_gen: G,
}

impl<T, G> Generator<Complex<T>> for ComplexGenerator<G>
where
    G: Generator<T>,
    T: Clone + Num,
{
    fn do_draw(&self, tc: &TestCase) -> Complex<T> {
        let re = self.real_gen.do_draw(tc);
        let im = self.imag_gen.do_draw(tc);
        Complex::new(re, im)
    }
}

/// Generate [`Complex<T>`] values from the given real and imaginary generators.
///
/// # Example
///
/// ```no_run
/// use num_complex::Complex;
/// use hegel::generators::{self as gs, Generator};
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let c: Complex<f64> = tc.draw(gs::complex(
///         gs::floats::<f64>().min_value(-100.0).max_value(100.0),
///         gs::floats::<f64>().min_value(-100.0).max_value(100.0),
///     ));
///     assert!(c.re >= -100.0 && c.re <= 100.0);
/// }
/// ```
pub fn complex<T, G>(real_gen: G, imag_gen: G) -> ComplexGenerator<G>
where
    G: Generator<T>,
    T: Clone + Num,
{
    ComplexGenerator { real_gen, imag_gen }
}

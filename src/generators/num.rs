use super::{Generator, TestCase};
use crate::utils::num::{cbor_to_bigint, cbor_to_biguint, int_to_cbor};
use ciborium::Value;
use num_bigint::{BigInt, BigUint};
use num_complex::Complex;
use num_integer::Integer as NumInteger;
use num_rational::Ratio;
use num_traits::{Num, One, Zero};

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
    T: Clone + NumInteger,
{
    fn do_draw(&self, tc: &TestCase) -> Ratio<T> {
        let numer = self.numer_gen.do_draw(tc);
        let denom = self.denom_gen.do_draw(tc);
        Ratio::new(numer, denom)
    }
}

/// Generate [`Ratio<T>`] values.
///
/// By default, uses `integers::<T>()` for the numerator and
/// `integers::<T>().min_value(T::one())` for the denominator.
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
///     let r: Ratio<i64> = tc.draw(gs::rationals::<i64>());
///     assert!(*r.denom() > 0);
///
///     // Customize numerator and denominator ranges
///     let r: Ratio<i64> = tc.draw(gs::rationals::<i64>()
///         .numerator(gs::integers::<i64>().min_value(0).max_value(100))
///         .denominator(gs::integers::<i64>().min_value(1).max_value(10)));
///     assert!(*r.numer() >= 0 && *r.denom() >= 1);
/// }
/// ```
pub fn rationals<T: super::Integer>()
-> RationalGenerator<super::IntegerGenerator<T>, super::IntegerGenerator<T>> {
    RationalGenerator {
        numer_gen: super::integers::<T>(),
        denom_gen: super::integers::<T>().min_value(<T as super::Integer>::one()),
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

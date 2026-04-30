use super::{BasicGenerator, Generator, TestCase};
use crate::utils::cbor_utils::cbor_map;
use ciborium::Value;
use num_integer::Integer as NumInteger;
use num_rational::Ratio;
use num_traits::CheckedMul;

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

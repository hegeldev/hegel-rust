use super::{BasicGenerator, Generator, TestCase};
use crate::cbor_utils::{cbor_map, cbor_serialize, map_insert};
use ciborium::Value;
use std::marker::PhantomData;

/// Trait bound for integer types usable with [`integers()`].
pub trait Integer: Copy + Ord {
    /// The minimum value of this type.
    const MIN: Self;
    /// The maximum value of this type.
    const MAX: Self;
}

macro_rules! impl_integer_type {
    ($($t:ty),*) => { $(
        impl Integer for $t {
            const MIN: Self = <$t>::MIN;
            const MAX: Self = <$t>::MAX;
        }
    )* };
}

impl_integer_type!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

/// Trait bound for float types usable with [`floats()`].
pub trait Float: Copy + PartialOrd {
    /// The minimum value of this type.
    const MIN: Self;
    /// The maximum value of this type.
    const MAX: Self;
}

impl Float for f32 {
    const MIN: Self = f32::MIN;
    const MAX: Self = f32::MAX;
}

impl Float for f64 {
    const MIN: Self = f64::MIN;
    const MAX: Self = f64::MAX;
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

impl<T: Integer + serde::Serialize> IntegerGenerator<T> {
    fn build_schema(&self) -> Value {
        let min = self.min.unwrap_or(T::MIN);
        let max = self.max.unwrap_or(T::MAX);
        assert!(min <= max, "Cannot have max_value < min_value");

        cbor_map! {
            "type" => "integer",
            "min_value" => cbor_serialize(&min),
            "max_value" => cbor_serialize(&max)
        }
    }
}

impl<T: Integer + serde::de::DeserializeOwned + serde::Serialize + Send + Sync + 'static>
    Generator<T> for IntegerGenerator<T>
{
    fn do_draw(&self, tc: &TestCase) -> T {
        super::generate_from_schema(tc, &self.build_schema())
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            super::deserialize_value(raw)
        }))
    }
}

/// Generate integers of type `T`.
///
/// Bounds default to the full range of `T`. See [`IntegerGenerator`] for
/// builder methods.
pub fn integers<
    T: Integer + serde::de::DeserializeOwned + serde::Serialize + Send + Sync + 'static,
>() -> IntegerGenerator<T> {
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
}

impl<T> FloatGenerator<T> {
    /// Set the minimum value (inclusive by default).
    pub fn min_value(mut self, min_value: T) -> Self {
        self.min = Some(min_value);
        self
    }

    /// Set the maximum value (inclusive by default).
    pub fn max_value(mut self, max_value: T) -> Self {
        self.max = Some(max_value);
        self
    }

    /// Set whether to exclude the minimum value from the range.
    pub fn exclude_min(mut self, exclude_min: bool) -> Self {
        self.exclude_min = exclude_min;
        self
    }

    /// Set whether to exclude the maximum value from the range.
    pub fn exclude_max(mut self, exclude_max: bool) -> Self {
        self.exclude_max = exclude_max;
        self
    }

    /// Whether NaN values are allowed. Cannot be used with bounds.
    pub fn allow_nan(mut self, allow: bool) -> Self {
        self.allow_nan = Some(allow);
        self
    }

    /// Whether infinite values are allowed. Cannot be used with both bounds set.
    pub fn allow_infinity(mut self, allow: bool) -> Self {
        self.allow_infinity = Some(allow);
        self
    }
}

impl<T: Float + serde::Serialize> FloatGenerator<T> {
    fn build_schema(&self) -> Value {
        let width = (std::mem::size_of::<T>() * 8) as u64;
        let has_min = self.min.is_some();
        let has_max = self.max.is_some();

        if let (Some(min), Some(max)) = (self.min, self.max) {
            assert!(min <= max, "Cannot have max_value < min_value");
        }

        let allow_nan = self.allow_nan.unwrap_or(!has_min && !has_max);
        let allow_infinity = self.allow_infinity.unwrap_or(!has_min || !has_max);

        if allow_nan && (has_min || has_max) {
            panic!("Cannot have allow_nan=true with min_value or max_value");
        }
        if allow_infinity && has_min && has_max {
            panic!("Cannot have allow_infinity=true with both min_value and max_value");
        }

        let mut schema = cbor_map! {
            "type" => "float",
            "exclude_min" => self.exclude_min,
            "exclude_max" => self.exclude_max,
            "allow_nan" => allow_nan,
            "allow_infinity" => allow_infinity,
            "width" => width
        };

        if let Some(ref min) = self.min {
            map_insert(&mut schema, "min_value", cbor_serialize(min));
        }
        if let Some(ref max) = self.max {
            map_insert(&mut schema, "max_value", cbor_serialize(max));
        }

        // When generating finite values without explicit bounds, add type
        // bounds to prevent overflow during deserialization (the protocol
        // uses f64, so f32 values near MAX can overflow when round-tripped)
        if !allow_nan && !allow_infinity {
            if self.min.is_none() {
                map_insert(&mut schema, "min_value", cbor_serialize(&T::MIN));
            }
            if self.max.is_none() {
                map_insert(&mut schema, "max_value", cbor_serialize(&T::MAX));
            }
        }

        schema
    }
}

impl<T: Float + serde::de::DeserializeOwned + serde::Serialize + Send + Sync + 'static> Generator<T>
    for FloatGenerator<T>
{
    fn do_draw(&self, tc: &TestCase) -> T {
        super::generate_from_schema(tc, &self.build_schema())
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            super::deserialize_value(raw)
        }))
    }
}

/// Generate floating-point values of type `T`.
///
/// By default, may produce NaN and infinity. See [`FloatGenerator`] for
/// builder methods.
pub fn floats<T: Float + serde::de::DeserializeOwned + serde::Serialize + Send + Sync + 'static>()
-> FloatGenerator<T> {
    FloatGenerator {
        min: None,
        max: None,
        exclude_min: false,
        exclude_max: false,
        allow_nan: None,
        allow_infinity: None,
    }
}

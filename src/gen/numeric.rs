use super::{generate_from_schema, BasicGenerator, Generate};
use crate::cbor_helpers::{cbor_map, cbor_serialize, map_insert};
use ciborium::Value;
use num::{Bounded, Float as NumFloat, Integer as NumInteger};
use std::marker::PhantomData;

pub struct IntegerGenerator<T> {
    min: Option<T>,
    max: Option<T>,
    _phantom: PhantomData<T>,
}

impl<T> IntegerGenerator<T> {
    /// Set the minimum value (inclusive).
    pub fn with_min(mut self, min: T) -> Self {
        self.min = Some(min);
        self
    }

    /// Set the maximum value (inclusive).
    pub fn with_max(mut self, max: T) -> Self {
        self.max = Some(max);
        self
    }
}

impl<T> IntegerGenerator<T>
where
    T: serde::Serialize + Bounded + Copy,
{
    fn build_schema(&self) -> Value {
        let min = self.min.unwrap_or_else(T::min_value);
        let max = self.max.unwrap_or_else(T::max_value);

        cbor_map! {
            "type" => "integer",
            "min_value" => cbor_serialize(&min),
            "max_value" => cbor_serialize(&max)
        }
    }
}

impl<T> Generate<T> for IntegerGenerator<T>
where
    T: serde::de::DeserializeOwned + serde::Serialize + Bounded + NumInteger + Send + Sync + Copy,
{
    fn generate(&self) -> T {
        generate_from_schema(&self.build_schema())
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            super::deserialize_value(raw)
        }))
    }
}

/// Generate integer values.
///
/// The type parameter determines the integer type. Bounds are automatically
/// derived from the type (e.g., `u8` uses 0-255). Use `with_min()` and
/// `with_max()` to constrain the range further.
///
/// # Example
///
/// ```no_run
/// use hegel::gen::{self, Generate};
///
/// // Generate any i32 (uses i32::MIN to i32::MAX)
/// let gen = gen::integers::<i32>();
///
/// // Generate u8 in range 0-100
/// let gen = gen::integers::<u8>().with_min(0).with_max(100);
/// ```
pub fn integers<T>() -> IntegerGenerator<T>
where
    T: serde::de::DeserializeOwned + serde::Serialize + Bounded + NumInteger + Send + Sync + Copy,
{
    IntegerGenerator {
        min: None,
        max: None,
        _phantom: PhantomData,
    }
}

// ============================================================================
// Float Generator
// ============================================================================

/// Generator for floating-point values.
pub struct FloatGenerator<T> {
    min: Option<T>,
    max: Option<T>,
    exclude_min: bool,
    exclude_max: bool,
    allow_nan: Option<bool>,
    allow_infinity: Option<bool>,
}

impl<T> FloatGenerator<T> {
    /// Set the minimum value.
    pub fn with_min(mut self, min: T) -> Self {
        self.min = Some(min);
        self
    }

    /// Set the maximum value.
    pub fn with_max(mut self, max: T) -> Self {
        self.max = Some(max);
        self
    }

    /// Exclude the minimum value from the range.
    pub fn exclude_min(mut self) -> Self {
        self.exclude_min = true;
        self
    }

    /// Exclude the maximum value from the range.
    pub fn exclude_max(mut self) -> Self {
        self.exclude_max = true;
        self
    }

    /// Whether NaN values can be generated.
    pub fn allow_nan(mut self, allow: bool) -> Self {
        self.allow_nan = Some(allow);
        self
    }

    /// Whether infinity values can be generated.
    pub fn allow_infinity(mut self, allow: bool) -> Self {
        self.allow_infinity = Some(allow);
        self
    }
}

impl<T> FloatGenerator<T>
where
    T: serde::Serialize + NumFloat,
{
    fn build_schema(&self) -> Value {
        let width = (std::mem::size_of::<T>() * 8) as u64;
        let has_min = self.min.is_some();
        let has_max = self.max.is_some();

        let allow_nan = self.allow_nan.unwrap_or(!has_min && !has_max);
        let allow_infinity = self.allow_infinity.unwrap_or(!has_min || !has_max);

        let mut schema = cbor_map! {
            "type" => "number",
            "exclude_min" => self.exclude_min,
            "exclude_max" => self.exclude_max,
            "allow_nan" => allow_nan,
            "allow_infinity" => allow_infinity,
            "width" => width
        };

        // Include user-specified bounds
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
                map_insert(&mut schema, "min_value", cbor_serialize(&T::min_value()));
            }
            if self.max.is_none() {
                map_insert(&mut schema, "max_value", cbor_serialize(&T::max_value()));
            }
        }

        schema
    }
}

impl<T> Generate<T> for FloatGenerator<T>
where
    T: serde::de::DeserializeOwned + serde::Serialize + NumFloat + Send + Sync,
{
    fn generate(&self) -> T {
        generate_from_schema(&self.build_schema())
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            super::deserialize_value(raw)
        }))
    }
}

/// Generate floating-point values.
///
/// By default, allows NaN and infinity values. Use `.allow_nan(false)` and
/// `.allow_infinity(false)` to restrict to finite values.
pub fn floats<T>() -> FloatGenerator<T>
where
    T: NumFloat,
{
    FloatGenerator {
        min: None,
        max: None,
        exclude_min: false,
        exclude_max: false,
        allow_nan: None,
        allow_infinity: None,
    }
}

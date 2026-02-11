use super::{BasicGenerator, Generate};
use crate::cbor_helpers::{cbor_map, cbor_serialize, map_insert};
use num::{Bounded, Float as NumFloat, Integer as NumInteger};
use std::marker::PhantomData;
use std::sync::OnceLock;

pub struct IntegerGenerator<T> {
    min: Option<T>,
    max: Option<T>,
    _phantom: PhantomData<T>,
    cached_basic: OnceLock<Option<BasicGenerator<T>>>,
}

impl<T> IntegerGenerator<T> {
    /// Set the minimum value (inclusive).
    pub fn with_min(mut self, min: T) -> Self {
        self.min = Some(min);
        self.cached_basic = OnceLock::new();
        self
    }

    /// Set the maximum value (inclusive).
    pub fn with_max(mut self, max: T) -> Self {
        self.max = Some(max);
        self.cached_basic = OnceLock::new();
        self
    }
}

impl<T> Generate<T> for IntegerGenerator<T>
where
    T: serde::de::DeserializeOwned
        + serde::Serialize
        + Bounded
        + NumInteger
        + Send
        + Sync
        + Copy
        + 'static,
{
    fn generate(&self) -> T {
        self.as_basic().unwrap().generate()
    }

    fn as_basic(&self) -> Option<BasicGenerator<T>> {
        self.cached_basic
            .get_or_init(|| {
                // Always include bounds - use type's min/max as defaults since Hegel
                // generates arbitrary precision integers without bounds
                let min = self.min.unwrap_or_else(T::min_value);
                let max = self.max.unwrap_or_else(T::max_value);

                Some(BasicGenerator::new(cbor_map! {
                    "type" => "integer",
                    "minimum" => cbor_serialize(&min),
                    "maximum" => cbor_serialize(&max)
                }))
            })
            .clone()
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
        cached_basic: OnceLock::new(),
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
    allow_nan: bool,
    allow_infinity: bool,
    cached_basic: OnceLock<Option<BasicGenerator<T>>>,
}

impl<T> FloatGenerator<T> {
    /// Set the minimum value.
    pub fn with_min(mut self, min: T) -> Self {
        self.min = Some(min);
        self.cached_basic = OnceLock::new();
        self
    }

    /// Set the maximum value.
    pub fn with_max(mut self, max: T) -> Self {
        self.max = Some(max);
        self.cached_basic = OnceLock::new();
        self
    }

    /// Exclude the minimum value from the range.
    pub fn exclude_min(mut self) -> Self {
        self.exclude_min = true;
        self.cached_basic = OnceLock::new();
        self
    }

    /// Exclude the maximum value from the range.
    pub fn exclude_max(mut self) -> Self {
        self.exclude_max = true;
        self.cached_basic = OnceLock::new();
        self
    }

    /// Set whether NaN values can be generated.
    pub fn allow_nan(mut self, allow: bool) -> Self {
        self.allow_nan = allow;
        self.cached_basic = OnceLock::new();
        self
    }

    /// Set whether infinity values can be generated.
    pub fn allow_infinity(mut self, allow: bool) -> Self {
        self.allow_infinity = allow;
        self.cached_basic = OnceLock::new();
        self
    }
}

impl<T> Generate<T> for FloatGenerator<T>
where
    T: serde::de::DeserializeOwned + serde::Serialize + NumFloat + Send + Sync + 'static,
{
    fn generate(&self) -> T {
        self.as_basic().unwrap().generate()
    }

    fn as_basic(&self) -> Option<BasicGenerator<T>> {
        self.cached_basic
            .get_or_init(|| {
                let width = (std::mem::size_of::<T>() * 8) as u64;

                let mut schema = cbor_map! {
                    "type" => "number",
                    "exclude_minimum" => self.exclude_min,
                    "exclude_maximum" => self.exclude_max,
                    "allow_nan" => self.allow_nan,
                    "allow_infinity" => self.allow_infinity,
                    "width" => width
                };

                // Include user-specified bounds
                if let Some(ref min) = self.min {
                    map_insert(&mut schema, "minimum", cbor_serialize(min));
                }
                if let Some(ref max) = self.max {
                    map_insert(&mut schema, "maximum", cbor_serialize(max));
                }

                // When generating finite values without explicit bounds, add type
                // bounds to prevent overflow during deserialization (the protocol
                // uses f64, so f32 values near MAX can overflow when round-tripped)
                if !self.allow_nan && !self.allow_infinity {
                    if self.min.is_none() {
                        map_insert(&mut schema, "minimum", cbor_serialize(&T::min_value()));
                    }
                    if self.max.is_none() {
                        map_insert(&mut schema, "maximum", cbor_serialize(&T::max_value()));
                    }
                }

                Some(BasicGenerator::new(schema))
            })
            .clone()
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
        allow_nan: true,
        allow_infinity: true,
        cached_basic: OnceLock::new(),
    }
}

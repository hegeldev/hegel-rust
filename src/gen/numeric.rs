use super::{generate_from_schema, Generate};
use num::{Bounded, Float as NumFloat, Integer as NumInteger};
use serde_json::{json, Value};
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

impl<T> Generate<T> for IntegerGenerator<T>
where
    T: serde::de::DeserializeOwned + serde::Serialize + Bounded + NumInteger + Send + Sync + Copy,
{
    fn generate(&self) -> T {
        generate_from_schema(&self.schema().unwrap())
    }

    fn schema(&self) -> Option<Value> {
        // Always include bounds - use type's min/max as defaults since Hegel
        // generates arbitrary precision integers without bounds
        let min = self.min.unwrap_or_else(T::min_value);
        let max = self.max.unwrap_or_else(T::max_value);

        Some(json!({
            "type": "integer",
            "minimum": min,
            "maximum": max
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
}

impl<T> Generate<T> for FloatGenerator<T>
where
    T: serde::de::DeserializeOwned + serde::Serialize + NumFloat + Send + Sync,
{
    fn generate(&self) -> T {
        generate_from_schema(&self.schema().unwrap())
    }

    fn schema(&self) -> Option<Value> {
        let mut schema = json!({"type": "number"});

        if let Some(ref min) = self.min {
            if self.exclude_min {
                schema["exclusiveMinimum"] = json!(min);
            } else {
                schema["minimum"] = json!(min);
            }
        }

        if let Some(ref max) = self.max {
            if self.exclude_max {
                schema["exclusiveMaximum"] = json!(max);
            } else {
                schema["maximum"] = json!(max);
            }
        }

        Some(schema)
    }
}

/// Generate floating-point values.
pub fn floats<T>() -> FloatGenerator<T>
where
    T: NumFloat,
{
    FloatGenerator {
        min: None,
        max: None,
        exclude_min: false,
        exclude_max: false,
    }
}

//! Typed draw parameter handling for the `hegel_generate_*` C ABI.

use crate::native::core::{EngineError, NativeTestCase};

/// Parameters of a float draw as accepted at the `hegel_generate_float` API
/// surface. Width-32 handling (bound clamping, result rounding) and the
/// exclusive-bound adjustments happen inside [`generate_float`], so callers
/// pass their request verbatim.
pub struct FloatSpec {
    pub width: u32,
    pub min_value: f64,
    pub max_value: f64,
    pub allow_nan: bool,
    pub allow_infinity: bool,
    pub exclude_min: bool,
    pub exclude_max: bool,
    pub smallest_nonzero_magnitude: f64,
}

/// Draw a float according to `spec`, validating the spec first.
///
/// Mirrors Hypothesis's float strategy handling: exclusive bounds step to
/// the next representable value at the requested width, width-32 draws clamp
/// finite bounds into the `f32` range when infinities are disallowed, and a
/// finite width-32 result is rounded through `f32`.
pub fn generate_float(ntc: &mut NativeTestCase, spec: &FloatSpec) -> Result<f64, EngineError> {
    if spec.width != 32 && spec.width != 64 {
        return Err(EngineError::InvalidArgument(format!(
            "unsupported float width: {} — Hegel supports widths 32 and 64",
            spec.width
        )));
    }
    let snm = spec.smallest_nonzero_magnitude;
    if !(snm.is_finite() && snm > 0.0) {
        return Err(EngineError::InvalidArgument(format!(
            "smallest_nonzero_magnitude must be a positive finite float, got {snm}"
        )));
    }
    if spec.min_value.is_nan() || spec.max_value.is_nan() {
        return Err(EngineError::InvalidArgument(
            "float bounds must not be NaN".to_string(),
        ));
    }
    let mut min_value = spec.min_value;
    let mut max_value = spec.max_value;
    if spec.exclude_min {
        min_value = if spec.width == 32 {
            f64::from((min_value as f32).next_up())
        } else {
            min_value.next_up()
        };
    }
    if spec.exclude_max {
        max_value = if spec.width == 32 {
            f64::from((max_value as f32).next_down())
        } else {
            max_value.next_down()
        };
    }
    if spec.width == 32 && !spec.allow_infinity {
        min_value = min_value.max(f64::from(f32::MIN));
        max_value = max_value.min(f64::from(f32::MAX));
    }
    if min_value > max_value {
        return Err(EngineError::InvalidArgument(format!(
            "min_value ({min_value}) must be <= max_value ({max_value}) \
             after exclusive-bound adjustment"
        )));
    }
    let v = ntc.draw_float(
        min_value,
        max_value,
        spec.allow_nan,
        spec.allow_infinity,
        snm,
    )?;
    Ok(if spec.width == 32 && v.is_finite() {
        f64::from(v as f32)
    } else {
        v
    })
}

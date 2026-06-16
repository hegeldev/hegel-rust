// Float schema interpreter.

use crate::cbor_utils::{as_bool, as_u64, map_get};
use crate::native::core::{EngineError, NativeTestCase};
use ciborium::Value;

pub(super) fn interpret_float(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    let width: u64 = map_get(schema, "width").and_then(as_u64).unwrap_or(64);
    // `width=64` is backed by `f64` and `width=32` by `f64` rounded to
    // `f32` precision (no `f16` Rust type yet). Any other value means the
    // caller's schema is invalid.
    if width != 32 && width != 64 {
        return Err(EngineError::InvalidArgument(format!(
            "unsupported float width: {width} — Hegel supports widths 32 and 64"
        )));
    }
    let min_value = map_get(schema, "min_value")
        .map(cbor_to_f64)
        .transpose()?
        .unwrap_or(f64::NEG_INFINITY);
    let max_value = map_get(schema, "max_value")
        .map(cbor_to_f64)
        .transpose()?
        .unwrap_or(f64::INFINITY);
    let allow_nan = map_get(schema, "allow_nan")
        .and_then(as_bool)
        .unwrap_or(true);
    let allow_infinity = map_get(schema, "allow_infinity")
        .and_then(as_bool)
        .unwrap_or(true);
    let exclude_min = map_get(schema, "exclude_min")
        .and_then(as_bool)
        .unwrap_or(false);
    let exclude_max = map_get(schema, "exclude_max")
        .and_then(as_bool)
        .unwrap_or(false);
    // Smallest magnitude (other than zero) the draw may produce —
    // Hypothesis's `smallest_nonzero_magnitude` constraint, which
    // `allow_subnormal=false` sets to the width's smallest normal. Defaults
    // to the width's smallest subnormal, i.e. no restriction, so schemas
    // without the field keep their old behaviour.
    let default_snm = if width == 32 {
        f32::from_bits(1) as f64
    } else {
        f64::from_bits(1)
    };
    let smallest_nonzero_magnitude = map_get(schema, "smallest_nonzero_magnitude")
        .map(cbor_to_f64)
        .transpose()?
        .unwrap_or(default_snm);
    // The `>` comparison (not `<= 0.0`) also rejects NaN.
    let snm_valid = smallest_nonzero_magnitude.is_finite() && smallest_nonzero_magnitude > 0.0;
    if !snm_valid {
        return Err(EngineError::InvalidArgument(format!(
            "smallest_nonzero_magnitude must be a positive finite float, \
             got {smallest_nonzero_magnitude}"
        )));
    }

    // Adjust bounds by one ULP for exclusive boundaries. This deliberately
    // applies to non-finite bounds too: `min_value=-inf, exclude_min=true` is
    // the Hypothesis idiom for "everything except -inf", and steps the bound
    // to `-MAX` (std's `next_up(-inf)`; `next_up(+inf)` is a fixed point, and
    // the generator rejects that combination as empty). Note the signed-zero
    // semantics also follow Hypothesis's `floats()`: excluding a `±0.0` bound
    // excludes *both* zeros (Hypothesis steps `next_up_normal` a second time
    // when the first step lands on the other zero; std's `next_up` skips it
    // in one step).
    // For f32 schemas (width=32), use f32-precision next_up/next_down so that
    // the adjusted bound is representable as f32 (preventing round-to-boundary bugs).
    let min_value = if exclude_min {
        if width == 32 {
            (min_value as f32).next_up() as f64
        } else {
            min_value.next_up()
        }
    } else {
        min_value
    };
    let max_value = if exclude_max {
        if width == 32 {
            (max_value as f32).next_down() as f64
        } else {
            max_value.next_down()
        }
    } else {
        max_value
    };

    // For f32 schemas with infinity disallowed, clamp the unbounded end(s) to
    // the f32 finite range. Otherwise `draw_float` can produce large f64 values
    // that round to ±f32::INFINITY when the client deserializes as f32 — and
    // since the draw used f64 bounds, the engine would not recognise the result
    // as violating `allow_infinity=false`. Mirrors Hypothesis, which applies
    // `float_of(x, width)` post-generation and rejects f32 overflows
    // (`strategies/_internal/numbers.py::floats`).
    let (min_value, max_value) = if width == 32 && !allow_infinity {
        (
            min_value.max(f32::MIN as f64),
            max_value.min(f32::MAX as f64),
        )
    } else {
        (min_value, max_value)
    };

    let v = ntc.draw_float(
        min_value,
        max_value,
        allow_nan,
        allow_infinity,
        smallest_nonzero_magnitude,
    )?;
    // Round the drawn f64 to f32 precision when the user asked for f32.
    // This matches what the client's deserializer will do and keeps
    // shrinking / replay stable (the same bit pattern round-trips).
    let v = if width == 32 && v.is_finite() {
        (v as f32) as f64
    } else {
        v
    };
    Ok(Value::Float(v))
}

/// Extract an f64 from a CBOR value (Float or Integer). Returns
/// [`EngineError::InvalidArgument`] for any other value, since a float
/// bound that is neither a float nor an integer means the caller's schema
/// is invalid.
fn cbor_to_f64(value: &Value) -> Result<f64, EngineError> {
    match value {
        Value::Float(f) => Ok(*f),
        Value::Integer(i) => Ok(i128::from(*i) as f64),
        _ => Err(EngineError::InvalidArgument(format!(
            "expected a CBOR float or integer, got {value:?}"
        ))),
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/float_tests.rs"]
mod tests;

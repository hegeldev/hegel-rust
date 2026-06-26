use crate::cbor_utils::{as_bool, as_u64, map_get};
use crate::native::core::{EngineError, NativeTestCase};
use ciborium::Value;

pub(super) fn interpret_float(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    let width: u64 = map_get(schema, "width").and_then(as_u64).unwrap_or(64);
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
    let default_snm = if width == 32 {
        f32::from_bits(1) as f64
    } else {
        f64::from_bits(1)
    };
    let smallest_nonzero_magnitude = map_get(schema, "smallest_nonzero_magnitude")
        .map(cbor_to_f64)
        .transpose()?
        .unwrap_or(default_snm);
    let snm_valid = smallest_nonzero_magnitude.is_finite() && smallest_nonzero_magnitude > 0.0;
    if !snm_valid {
        return Err(EngineError::InvalidArgument(format!(
            "smallest_nonzero_magnitude must be a positive finite float, \
             got {smallest_nonzero_magnitude}"
        )));
    }

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

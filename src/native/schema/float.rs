// Float schema interpreter.

use crate::cbor_utils::{as_bool, as_u64, map_get};
use crate::native::core::{NativeTestCase, StopTest};
use ciborium::Value;

pub(super) fn interpret_float(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let width: u64 = map_get(schema, "width").and_then(as_u64).unwrap_or(64);
    let min_value = map_get(schema, "min_value")
        .map(cbor_to_f64)
        .unwrap_or(f64::NEG_INFINITY);
    let max_value = map_get(schema, "max_value")
        .map(cbor_to_f64)
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

    // Adjust bounds by one ULP for exclusive boundaries.
    // For f32 schemas (width=32), use f32-precision next_up/next_down so that
    // the adjusted bound is representable as f32 (preventing round-to-boundary bugs).
    let min_value = if exclude_min && min_value.is_finite() {
        if width == 32 {
            (min_value as f32).next_up() as f64
        } else {
            min_value.next_up()
        }
    } else {
        min_value
    };
    let max_value = if exclude_max && max_value.is_finite() {
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

    let v = ntc.draw_float(min_value, max_value, allow_nan, allow_infinity)?;
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

/// Extract an f64 from a CBOR value (Float or Integer).
fn cbor_to_f64(value: &Value) -> f64 {
    match value {
        Value::Float(f) => *f,
        Value::Integer(i) => i128::from(*i) as f64,
        _ => panic!("Expected CBOR float/integer, got {:?}", value),
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/float_tests.rs"]
mod tests;

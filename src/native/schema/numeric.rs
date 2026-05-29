// Numeric schema interpreters: integer, boolean, constant.

use crate::native::core::{NativeTestCase, StopTest};
use ciborium::Value;

use super::{bigint_to_cbor, cbor_to_bigint};
use crate::cbor_utils::map_get;

pub(super) fn interpret_integer(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
    let min_cbor = map_get(schema, "min_value").expect("integer schema must have min_value");
    let max_cbor = map_get(schema, "max_value").expect("integer schema must have max_value");
    // Bounds are arbitrary-precision: a `u128::MAX` (or wider) bound is now a
    // single integer choice rather than the old selector + two-64-bit-halves hack.
    let min_value = cbor_to_bigint(min_cbor);
    let max_value = cbor_to_bigint(max_cbor);
    let v = ntc.draw_integer_big(&min_value, &max_value)?;
    Ok(bigint_to_cbor(&v))
}

pub(super) fn interpret_boolean(ntc: &mut NativeTestCase) -> Result<Value, StopTest> {
    let v = ntc.weighted(0.5, None)?;
    Ok(Value::Bool(v))
}

pub(super) fn interpret_constant(schema: &Value) -> Result<Value, StopTest> {
    let value = map_get(schema, "value").expect("constant schema must have value");
    Ok(value.clone())
}

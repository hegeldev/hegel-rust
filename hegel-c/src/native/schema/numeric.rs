use crate::native::core::{EngineError, NativeTestCase};
use ciborium::Value;

use super::{bigint_to_cbor, cbor_to_bigint, require};

pub(super) fn interpret_integer(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    let min_cbor = require(schema, "min_value")?;
    let max_cbor = require(schema, "max_value")?;
    let min = cbor_to_bigint(min_cbor)?;
    let max = cbor_to_bigint(max_cbor)?;
    let value = ntc.draw_integer(min, max)?;
    Ok(bigint_to_cbor(&value))
}

pub(super) fn interpret_boolean(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    let v = ntc.weighted(0.5, None)?;
    Ok(Value::Bool(v))
}

pub(super) fn interpret_constant(schema: &Value) -> Result<Value, EngineError> {
    let value = require(schema, "value")?;
    Ok(value.clone())
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/numeric_tests.rs"]
mod tests;

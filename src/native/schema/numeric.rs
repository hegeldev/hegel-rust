// Numeric schema interpreters: integer, boolean, constant.

use crate::native::core::{NativeTestCase, StopTest};
use ciborium::Value;

use super::{bignum_overflows_i128, cbor_to_i128, i128_to_cbor, u128_to_cbor};
use crate::cbor_utils::map_get;

pub(super) fn interpret_integer(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
    let min_cbor = map_get(schema, "min_value").expect("integer schema must have min_value");
    let max_cbor = map_get(schema, "max_value").expect("integer schema must have max_value");
    let min_value = cbor_to_i128(min_cbor);
    let max_value = cbor_to_i128(max_cbor);

    // If max saturated because it exceeded i128::MAX (e.g. u128::MAX), draw using
    // a selector + two 64-bit halves to cover the full u128 range.
    if bignum_overflows_i128(max_cbor) {
        // Selector: 0 = u128::MIN, 1 = u128::MAX, else = random two-halves.
        // Edge case boosting on the selector naturally produces the min (0) often.
        // Selector = 1 gives u128::MAX with ~1% probability.
        let selector = ntc.draw_integer(0, 99)?;
        match selector {
            0 => return Ok(u128_to_cbor(0u128)),
            1 => return Ok(u128_to_cbor(u128::MAX)),
            _ => {}
        }
        let hi = ntc.draw_integer(0, u64::MAX as i128)?;
        let lo = ntc.draw_integer(0, u64::MAX as i128)?;
        let v = ((hi as u128) << 64) | (lo as u128);
        return Ok(u128_to_cbor(v));
    }

    let v = ntc.draw_integer(min_value, max_value)?;
    Ok(i128_to_cbor(v))
}

pub(super) fn interpret_boolean(ntc: &mut NativeTestCase) -> Result<Value, StopTest> {
    let v = ntc.weighted(0.5, None)?;
    Ok(Value::Bool(v))
}

pub(super) fn interpret_constant(schema: &Value) -> Result<Value, StopTest> {
    let value = map_get(schema, "value").expect("constant schema must have value");
    Ok(value.clone())
}

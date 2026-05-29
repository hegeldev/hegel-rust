// Numeric schema interpreters: integer, boolean, constant.

use crate::native::bignum::BigInt;
use crate::native::core::{EngineError, NativeTestCase};
use ciborium::Value;

use super::{bigint_to_cbor, cbor_to_bigint, require};

pub(super) fn interpret_integer(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    let min_cbor = require(schema, "min_value")?;
    let max_cbor = require(schema, "max_value")?;
    let min = cbor_to_bigint(min_cbor);
    let max = cbor_to_bigint(max_cbor);
    let value = draw_in_range(ntc, &min, &max)?;
    Ok(bigint_to_cbor(&value))
}

/// Draw an integer in `[min, max]` using the narrowest native width whose range
/// covers the bounds, falling back to [`BigInt`] for ranges beyond `u128` /
/// below `i128::MIN`. Returns the drawn value as a [`BigInt`].
///
/// Picking the concrete width here is what makes generation "use appropriate
/// types based on the schema": a `[0, 255]` range is drawn as a `u8`, a full
/// `[0, u128::MAX]` range as a `u128` (no `BigInt` allocation), and a genuinely
/// unbounded range as a real big integer.
fn draw_in_range(ntc: &mut NativeTestCase, min: &BigInt, max: &BigInt) -> Result<BigInt, EngineError> {
    macro_rules! try_width {
        ($t:ty) => {
            if let (Ok(lo), Ok(hi)) = (<$t>::try_from(min), <$t>::try_from(max)) {
                return Ok(BigInt::from(ntc.draw_integer::<$t>(lo, hi)?));
            }
        };
    }
    // Prefer the unsigned width at each size (covers `[0, T::MAX]` ranges and
    // the full `u128` span without a `BigInt`), then the signed width, widening
    // upward until one covers `[min, max]`.
    try_width!(u8);
    try_width!(i8);
    try_width!(u16);
    try_width!(i16);
    try_width!(u32);
    try_width!(i32);
    try_width!(u64);
    try_width!(i64);
    try_width!(u128);
    try_width!(i128);
    ntc.draw_integer::<BigInt>(min.clone(), max.clone())
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

use crate::cbor_utils::{as_u64, map_get};
use crate::native::core::{EngineError, NativeTestCase};
use crate::native::draws::internet::{generate_domain, generate_email, generate_url};
use ciborium::Value;

fn encode_string(s: String) -> Value {
    Value::Tag(91, Box::new(Value::Bytes(s.into_bytes())))
}

pub(super) fn interpret_domain(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    let max_length = map_get(schema, "max_length")
        .and_then(as_u64)
        .unwrap_or(255) as usize;
    Ok(encode_string(generate_domain(ntc, max_length)?))
}

pub(super) fn interpret_email(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    Ok(encode_string(generate_email(ntc)?))
}

pub(super) fn interpret_url(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    Ok(encode_string(generate_url(ntc)?))
}

use crate::cbor_utils::{as_bool, as_text, map_get};
use crate::native::core::{EngineError, NativeTestCase};
use crate::native::draws::regex::generate_regex;
use ciborium::Value;

use super::text::build_intervals;

pub(super) fn interpret_regex(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    let pattern = map_get(schema, "pattern")
        .and_then(as_text)
        .ok_or_else(|| {
            EngineError::InvalidArgument(
                "regex schema is missing a string \"pattern\" field".to_string(),
            )
        })?;
    let fullmatch = map_get(schema, "fullmatch")
        .and_then(as_bool)
        .unwrap_or(false);
    let alphabet = map_get(schema, "alphabet")
        .map(build_intervals)
        .transpose()?;
    let s = generate_regex(ntc, pattern, fullmatch, &alphabet)?;
    Ok(Value::Tag(91, Box::new(Value::Bytes(s.into_bytes()))))
}

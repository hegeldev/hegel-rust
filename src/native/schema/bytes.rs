// Bytes schema interpreter.

use crate::cbor_utils::{as_u64, map_get};
use crate::native::core::{NativeTestCase, StopTest};
use ciborium::Value;

pub(super) fn interpret_binary(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
    let min_size = map_get(schema, "min_size").and_then(as_u64).unwrap_or(0) as usize;
    // Unbounded `max_size` falls back to a generous ceiling — Hypothesis's
    // server backend also truncates generation at some finite cap, and
    // matching that keeps shrinker and cache behaviour sensible.
    let max_size = map_get(schema, "max_size")
        .and_then(as_u64)
        .map(|n| n as usize)
        .unwrap_or(100);

    let bytes = ntc.draw_bytes(min_size, max_size)?;
    Ok(Value::Bytes(bytes))
}

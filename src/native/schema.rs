// Schema interpreter for the native backend.
//
// Translates CBOR schemas (as sent by hegel generators) into concrete
// values using pbtkit-style choice recording. Only schemas usable from
// pbtkit's core.py are implemented; everything else is `todo!()`.

use crate::cbor_utils::{as_text, map_get};
use crate::native::core::{NativeTestCase, StopTest};
use crate::test_case::StopTestError;
use ciborium::Value;

/// Top-level dispatcher for native request handling.
///
/// Called from TestCase::send_request when the native backend is active.
pub fn dispatch_request(
    ntc: &mut NativeTestCase,
    command: &str,
    payload: &Value,
) -> Result<Value, StopTestError> {
    match command {
        "generate" => {
            let schema = map_get(payload, "schema").expect("generate command missing schema");
            interpret_schema(ntc, schema).map_err(|StopTest| StopTestError)
        }
        "start_span" | "stop_span" => {
            // Spans are tracked locally by TestCase for output purposes.
            // The native backend doesn't need to do anything here yet.
            Ok(Value::Null)
        }
        "new_collection" | "collection_more" | "collection_reject" => {
            todo!(
                "Native backend does not yet support collection protocol commands ({})",
                command
            )
        }
        "new_pool" | "pool_consume" | "pool_add" | "pool_generate" => {
            todo!(
                "Native backend does not yet support variable pool commands ({})",
                command
            )
        }
        _ => panic!("Unknown native command: {}", command),
    }
}

/// Interpret a CBOR schema and produce a value using the native test case.
fn interpret_schema(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let schema_type = map_get(schema, "type")
        .and_then(as_text)
        .expect("Schema must have a \"type\" field");

    match schema_type {
        "integer" => interpret_integer(ntc, schema),
        "boolean" => interpret_boolean(ntc),
        "constant" => interpret_constant(schema),
        "null" => Ok(Value::Null),
        "tuple" => interpret_tuple(ntc, schema),
        "one_of" => interpret_one_of(ntc, schema),

        // Schemas that require features beyond pbtkit core.py:
        "float" => todo!("Native backend does not yet support float schema"),
        "string" => todo!("Native backend does not yet support string schema"),
        "binary" => todo!("Native backend does not yet support binary schema"),
        "regex" => todo!("Native backend does not yet support regex schema"),
        "list" => todo!("Native backend does not yet support list schema"),
        "dict" => todo!("Native backend does not yet support dict schema"),
        "email" => todo!("Native backend does not yet support email schema"),
        "url" => todo!("Native backend does not yet support url schema"),
        "domain" => todo!("Native backend does not yet support domain schema"),
        "ipv4" => todo!("Native backend does not yet support ipv4 schema"),
        "ipv6" => todo!("Native backend does not yet support ipv6 schema"),
        "date" => todo!("Native backend does not yet support date schema"),
        "time" => todo!("Native backend does not yet support time schema"),
        "datetime" => todo!("Native backend does not yet support datetime schema"),
        "sampled_from" => todo!("Native backend does not yet support sampled_from schema"),

        other => panic!("Unknown schema type: {}", other),
    }
}

fn interpret_integer(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let min_value = map_get(schema, "min_value")
        .map(cbor_to_i128)
        .expect("integer schema must have min_value");
    let max_value = map_get(schema, "max_value")
        .map(cbor_to_i128)
        .expect("integer schema must have max_value");

    let v = ntc.draw_integer(min_value, max_value)?;
    Ok(i128_to_cbor(v))
}

fn interpret_boolean(ntc: &mut NativeTestCase) -> Result<Value, StopTest> {
    let v = ntc.weighted(0.5, None)?;
    Ok(Value::Bool(v))
}

fn interpret_constant(schema: &Value) -> Result<Value, StopTest> {
    let value = map_get(schema, "value").expect("constant schema must have value");
    Ok(value.clone())
}

fn interpret_tuple(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let elements = match map_get(schema, "elements") {
        Some(Value::Array(arr)) => arr,
        _ => panic!("tuple schema must have elements array"),
    };
    let mut results = Vec::with_capacity(elements.len());
    for element_schema in elements {
        results.push(interpret_schema(ntc, element_schema)?);
    }
    Ok(Value::Array(results))
}

fn interpret_one_of(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let generators = match map_get(schema, "generators") {
        Some(Value::Array(arr)) => arr,
        _ => panic!("one_of schema must have generators array"),
    };
    assert!(!generators.is_empty(), "one_of schema must have at least one generator");
    let idx = ntc.draw_integer(0, generators.len() as i128 - 1)?;
    interpret_schema(ntc, &generators[idx as usize])
}

/// Convert a CBOR value to i128.
fn cbor_to_i128(value: &Value) -> i128 {
    match value {
        Value::Integer(i) => (*i).into(),
        _ => panic!("Expected CBOR integer, got {:?}", value),
    }
}

/// Convert an i128 to a CBOR value.
///
/// ciborium's Integer type supports up to i64/u64 directly. For values
/// that fit, we use the direct conversion. Values outside that range
/// use serialization via serde.
fn i128_to_cbor(v: i128) -> Value {
    if let Ok(n) = i64::try_from(v) {
        Value::Integer(n.into())
    } else if let Ok(n) = u64::try_from(v) {
        Value::Integer(n.into())
    } else {
        // For values outside i64/u64 range, serialize through serde
        crate::cbor_utils::cbor_serialize(&v)
    }
}

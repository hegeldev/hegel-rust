// Collection schema interpreters: list, dict, tuple, one_of, sampled_from.

use crate::native::core::{ManyState, NativeTestCase, StopTest};
use crate::cbor_utils::{as_bool, as_u64, map_get};
use ciborium::Value;

use super::{interpret_schema, many_more, many_reject};

pub(super) fn interpret_tuple(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
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

pub(super) fn interpret_one_of(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
    let generators = match map_get(schema, "generators") {
        Some(Value::Array(arr)) => arr,
        _ => panic!("one_of schema must have generators array"),
    };
    assert!(!generators.is_empty(), "one_of schema must have at least one generator");
    let idx = ntc.draw_integer(0, generators.len() as i128 - 1)?;
    interpret_schema(ntc, &generators[idx as usize])
}

pub(super) fn interpret_sampled_from(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
    let values = match map_get(schema, "values") {
        Some(Value::Array(arr)) => arr,
        _ => panic!("sampled_from schema must have values array"),
    };
    assert!(!values.is_empty(), "sampled_from schema must have at least one value");
    let idx = ntc.draw_integer(0, values.len() as i128 - 1)?;
    Ok(encode_schema_value(&values[idx as usize]))
}

pub(super) fn interpret_list(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
    let element_schema = map_get(schema, "elements").expect("list schema must have elements");
    let min_size = map_get(schema, "min_size")
        .and_then(as_u64)
        .unwrap_or(0) as usize;
    let max_size = map_get(schema, "max_size").and_then(as_u64).map(|n| n as usize);
    let unique = map_get(schema, "unique")
        .and_then(as_bool)
        .unwrap_or(false);

    let mut state = ManyState::new(min_size, max_size);
    let mut results: Vec<Value> = Vec::new();

    loop {
        if !many_more(ntc, &mut state)? {
            break;
        }
        let element = interpret_schema(ntc, element_schema)?;
        if unique && results.iter().any(|existing| existing == &element) {
            many_reject(ntc, &mut state)?;
            continue;
        }
        results.push(element);
    }

    Ok(Value::Array(results))
}

pub(super) fn interpret_dict(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
    let key_schema = map_get(schema, "keys").expect("dict schema must have keys");
    let val_schema = map_get(schema, "values").expect("dict schema must have values");
    let min_size = map_get(schema, "min_size")
        .and_then(as_u64)
        .unwrap_or(0) as usize;
    let max_size = map_get(schema, "max_size").and_then(as_u64).map(|n| n as usize);

    let mut state = ManyState::new(min_size, max_size);
    let mut pairs: Vec<Value> = Vec::new();
    let mut keys: Vec<Value> = Vec::new();

    loop {
        if !many_more(ntc, &mut state)? {
            break;
        }
        let key = interpret_schema(ntc, key_schema)?;
        if keys.iter().any(|existing| existing == &key) {
            many_reject(ntc, &mut state)?;
            continue;
        }
        let value = interpret_schema(ntc, val_schema)?;
        keys.push(key.clone());
        pairs.push(Value::Array(vec![key, value]));
    }

    Ok(Value::Array(pairs))
}

/// Encode a schema value for transport back to the generator.
///
/// Mirrors hegel-core's `_encode_value`: text strings are wrapped in
/// CBOR tag 91 (HEGEL_STRING_TAG) so they can be deserialized by `HegelValue`.
fn encode_schema_value(value: &Value) -> Value {
    match value {
        Value::Text(s) => Value::Tag(91, Box::new(Value::Bytes(s.as_bytes().to_vec()))),
        other => other.clone(),
    }
}

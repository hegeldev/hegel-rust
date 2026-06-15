// Collection schema interpreters: list, dict, tuple, one_of, sampled_from.

use crate::cbor_utils::{as_bool, as_u64, map_get};
use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::core::{EngineError, ManyState, NativeTestCase};
use ciborium::Value;

use super::{cbor_to_bigint, interpret_schema, many_more, many_reject, require};

pub(super) fn interpret_tuple(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    let elements = match require(schema, "elements")? {
        Value::Array(arr) => arr,
        other => {
            return Err(EngineError::InvalidArgument(format!(
                "tuple schema \"elements\" must be an array, got {other:?}"
            )));
        }
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
) -> Result<Value, EngineError> {
    let generators = match require(schema, "generators")? {
        Value::Array(arr) if !arr.is_empty() => arr,
        _ => {
            return Err(EngineError::InvalidArgument(
                "one_of schema \"generators\" must be a non-empty array".to_string(),
            ));
        }
    };
    let idx = ntc.draw_integer(BigInt::from(0), BigInt::from(generators.len() as i64 - 1))?;
    let idx_i64 = idx.to_i128().unwrap() as i64;
    let value = interpret_schema(ntc, &generators[idx_i64 as usize])?;
    Ok(Value::Array(vec![Value::Integer(idx_i64.into()), value]))
}

pub(super) fn interpret_sampled_from(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    let values = match require(schema, "values")? {
        Value::Array(arr) if !arr.is_empty() => arr,
        _ => {
            return Err(EngineError::InvalidArgument(
                "sampled_from schema \"values\" must be a non-empty array".to_string(),
            ));
        }
    };
    let idx = ntc.draw_integer(BigInt::from(0), BigInt::from(values.len() as i64 - 1))?;
    Ok(encode_schema_value(
        &values[idx.to_i128().unwrap() as usize],
    ))
}

pub(super) fn interpret_list(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    let element_schema = require(schema, "elements")?;
    let min_size = map_get(schema, "min_size").and_then(as_u64).unwrap_or(0) as usize;
    let max_size = map_get(schema, "max_size")
        .and_then(as_u64)
        .map(|n| n as usize);
    let unique = map_get(schema, "unique").and_then(as_bool).unwrap_or(false);

    if unique {
        if let Some((min_val, max_val)) = bounded_integer_range(element_schema) {
            let range_size = (max_val - min_val + 1) as usize;
            return interpret_unique_integer_list(ntc, min_size, max_size, min_val, range_size);
        }
    }

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

/// Pool-based unique list generation for bounded integer ranges.
/// Port of Hypothesis's UniqueSampledListStrategy: draw indices into a
/// shrinking pool of remaining values, avoiding the coupon-collector problem.
fn interpret_unique_integer_list(
    ntc: &mut NativeTestCase,
    min_size: usize,
    max_size: Option<usize>,
    min_val: i128,
    range_size: usize,
) -> Result<Value, EngineError> {
    let effective_max = max_size.map_or(range_size, |m| m.min(range_size));
    let mut state = ManyState::new(min_size, Some(effective_max));
    let mut remaining: Vec<i128> = (min_val..min_val + range_size as i128).collect();
    let mut results = Vec::new();

    loop {
        if remaining.is_empty() || !many_more(ntc, &mut state)? {
            break;
        }
        let j = ntc
            .draw_integer(BigInt::from(0), BigInt::from(remaining.len() as i64 - 1))?
            .to_i128()
            .unwrap() as usize;
        let value = remaining.remove(j);
        results.push(Value::Integer((value as i64).into()));
    }

    Ok(Value::Array(results))
}

fn bounded_integer_range(schema: &Value) -> Option<(i128, i128)> {
    use crate::cbor_utils::as_text;
    let schema_type = map_get(schema, "type").and_then(as_text)?;
    if schema_type != "integer" {
        return None;
    }
    let min_val = cbor_to_bigint(map_get(schema, "min_value")?).ok()?;
    let max_val = cbor_to_bigint(map_get(schema, "max_value")?).ok()?;
    let span = &max_val - &min_val + 1;
    if !(BigInt::from(1)..=BigInt::from(10_000)).contains(&span) {
        return None;
    }
    // The span check above guarantees both bounds fit comfortably in i128
    // unless they are themselves astronomically large; in that rare case bail
    // out of the small-range optimisation rather than truncate.
    Some((min_val.to_i128()?, max_val.to_i128()?))
}

pub(super) fn interpret_dict(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    let key_schema = require(schema, "keys")?;
    let val_schema = require(schema, "values")?;
    let min_size = map_get(schema, "min_size").and_then(as_u64).unwrap_or(0) as usize;
    let max_size = map_get(schema, "max_size")
        .and_then(as_u64)
        .map(|n| n as usize);

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

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/collections_tests.rs"]
mod tests;

// Schema interpreter for the native backend.
//
// Translates CBOR schemas (as sent by hegel generators) into concrete
// values using pbtkit-style choice recording.
//
// Split into submodules:
//   numeric     — interpret_integer, interpret_boolean, interpret_constant
//   float       — interpret_float
//   text        — interpret_string, interpret_binary, StringAlphabet helpers
//   regex       — interpret_regex, generate_hir_string
//   collections — interpret_list, interpret_dict, interpret_tuple, interpret_one_of, interpret_sampled_from
//   special     — date, time, datetime, ipv4, ipv6, domain, email, url

mod collections;
mod float;
mod numeric;
mod regex;
mod special;
mod text;

use crate::cbor_utils::{as_bool, as_u64, map_get};
use crate::native::core::{ManyState, NativeTestCase, Status, StopTest};
use ciborium::Value;

/// Top-level dispatcher for native request handling.
///
/// Called from NativeDataSource when the native backend is active.
pub fn dispatch_request(
    ntc: &mut NativeTestCase,
    command: &str,
    payload: &Value,
) -> Result<Value, StopTest> {
    match command {
        "generate" => {
            let schema = map_get(payload, "schema").expect("generate command missing schema");
            interpret_schema(ntc, schema)
        }
        "start_span" | "stop_span" => {
            // Spans are tracked locally by TestCase for output purposes.
            // The native backend doesn't need to do anything here yet.
            Ok(Value::Null)
        }
        "new_collection" => {
            let min_size = map_get(payload, "min_size").and_then(as_u64).unwrap_or(0) as usize;
            let max_size = map_get(payload, "max_size")
                .and_then(as_u64)
                .map(|n| n as usize);
            let state = ManyState::new(min_size, max_size);
            let id = ntc.new_collection(state);
            Ok(Value::Integer(id.into()))
        }
        "collection_more" => {
            let id = map_get(payload, "collection_id")
                .map(cbor_to_i64)
                .expect("collection_more missing collection_id");
            let mut state = ntc
                .collections
                .remove(&id)
                .expect("collection_more: unknown collection_id");
            let result = many_more(ntc, &mut state).map_err(|StopTest| StopTest)?;
            ntc.collections.insert(id, state);
            Ok(Value::Bool(result))
        }
        "collection_reject" => {
            let id = map_get(payload, "collection_id")
                .map(cbor_to_i64)
                .expect("collection_reject missing collection_id");
            let mut state = ntc
                .collections
                .remove(&id)
                .expect("collection_reject: unknown collection_id");
            many_reject(ntc, &mut state).map_err(|StopTest| StopTest)?;
            ntc.collections.insert(id, state);
            Ok(Value::Null)
        }
        "new_pool" => {
            let pool_id = ntc.variable_pools.len() as i64;
            ntc.variable_pools
                .push(crate::native::core::NativeVariables::new());
            Ok(Value::Integer(pool_id.into()))
        }
        "pool_add" => {
            let pool_id = map_get(payload, "pool_id")
                .map(cbor_to_i64)
                .expect("pool_add missing pool_id") as usize;
            let variable_id = ntc.variable_pools[pool_id].next() as i64;
            Ok(Value::Integer(variable_id.into()))
        }
        "pool_consume" => {
            let pool_id = map_get(payload, "pool_id")
                .map(cbor_to_i64)
                .expect("pool_consume missing pool_id") as usize;
            let variable_id = map_get(payload, "variable_id")
                .map(cbor_to_i64)
                .expect("pool_consume missing variable_id") as i128;
            ntc.variable_pools[pool_id].consume(variable_id);
            Ok(Value::Null)
        }
        "pool_generate" => {
            let pool_id = map_get(payload, "pool_id")
                .map(cbor_to_i64)
                .expect("pool_generate missing pool_id") as usize;
            let consume = map_get(payload, "consume")
                .and_then(as_bool)
                .unwrap_or(false);

            let active = ntc.variable_pools[pool_id].active();
            if active.is_empty() {
                // No variables available: mark test case as invalid.
                return Err(StopTest);
            }
            let n = active.len() as i128;
            // Draw index from [0, n-1]. Shrink towards n-1 (last added = most recent)
            // by drawing k from [0, n-1] and using index = n-1-k.
            let k = ntc.draw_integer(0, n - 1).map_err(|StopTest| StopTest)?;
            let idx = (n - 1 - k) as usize;
            let variable_id = active[idx] as i64;
            if consume {
                ntc.variable_pools[pool_id].consume(variable_id as i128);
            }
            Ok(Value::Integer(variable_id.into()))
        }
        _ => panic!("Unknown native command: {}", command),
    }
}

/// Interpret a CBOR schema and produce a value using the native test case.
///
/// For leaf schemas (those that don't call `interpret_schema` recursively),
/// records a span in `ntc.spans` so that span-mutation exploration can find
/// structurally-duplicate values (e.g. two equal strings in a tuple).
/// Only leaf schemas are tracked to avoid overlapping spans from nested schemas.
fn interpret_schema(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    use crate::cbor_utils::as_text;
    let schema_type = map_get(schema, "type")
        .and_then(as_text)
        .expect("Schema must have a \"type\" field");

    // Record spans only for leaf schemas (no recursive interpret_schema calls).
    // This avoids overlapping spans that would corrupt span-mutation results.
    let is_leaf = matches!(
        schema_type,
        "integer" | "boolean" | "float" | "string" | "binary" | "sampled_from"
    );
    let span_start = if is_leaf { ntc.nodes.len() } else { 0 };

    let result = match schema_type {
        "integer" => numeric::interpret_integer(ntc, schema),
        "boolean" => numeric::interpret_boolean(ntc),
        "constant" => numeric::interpret_constant(schema),
        "null" => Ok(Value::Null),
        "tuple" => collections::interpret_tuple(ntc, schema),
        "one_of" => collections::interpret_one_of(ntc, schema),
        "sampled_from" => collections::interpret_sampled_from(ntc, schema),
        "list" => collections::interpret_list(ntc, schema),
        "dict" => collections::interpret_dict(ntc, schema),
        "string" => text::interpret_string(ntc, schema),
        "binary" => text::interpret_binary(ntc, schema),

        "float" => float::interpret_float(ntc, schema),
        "regex" => regex::interpret_regex(ntc, schema),
        "email" => special::interpret_email(ntc),
        "url" => special::interpret_url(ntc),
        "domain" => special::interpret_domain(ntc, schema),
        "ipv4" => special::interpret_ipv4(ntc),
        "ipv6" => special::interpret_ipv6(ntc),
        "date" => special::interpret_date(ntc),
        "time" => special::interpret_time(ntc),
        "datetime" => special::interpret_datetime(ntc),

        other => panic!("Unknown schema type: {}", other),
    };
    if is_leaf && result.is_ok() {
        ntc.record_span(span_start, ntc.nodes.len(), schema_type.to_string());
    }
    result
}

/// Advance the many state by one element. Returns true if another element should be drawn.
///
/// Port of pbtkit's `many.more()`.
fn many_more(ntc: &mut NativeTestCase, state: &mut ManyState) -> Result<bool, StopTest> {
    let should_continue = if state.min_size as f64 == state.max_size {
        // Fixed size: draw exactly min_size elements.
        state.count < state.min_size
    } else {
        let forced = if state.force_stop {
            Some(false)
        } else if state.count < state.min_size {
            Some(true)
        } else if state.count as f64 >= state.max_size {
            Some(false)
        } else {
            None
        };
        ntc.weighted(state.p_continue, forced)?
    };

    if should_continue {
        state.count += 1;
    }
    Ok(should_continue)
}

/// Reject the last drawn element. Port of pbtkit's `many.reject()`.
fn many_reject(ntc: &mut NativeTestCase, state: &mut ManyState) -> Result<(), StopTest> {
    assert!(state.count > 0);
    state.count -= 1;
    state.rejections += 1;
    if state.rejections > std::cmp::max(3, 2 * state.count) {
        if state.count < state.min_size {
            ntc.status = Some(Status::Invalid);
            return Err(StopTest);
        } else {
            state.force_stop = true;
        }
    }
    Ok(())
}

/// Convert a CBOR value to i128, handling bignum tags.
///
/// For positive bignums (tag 2) that exceed i128::MAX (e.g. u128::MAX),
/// we saturate at i128::MAX so the integer range remains valid.
fn cbor_to_i128(value: &Value) -> i128 {
    match value {
        Value::Integer(i) => (*i).into(),
        Value::Tag(2, inner) => {
            // CBOR tag 2: positive bignum (big-endian bytes)
            let Value::Bytes(bytes) = inner.as_ref() else {
                panic!("Expected Bytes inside bignum tag 2, got {:?}", inner)
            };
            let mut n = 0u128;
            for b in bytes {
                n = (n << 8) | (*b as u128);
            }
            // Saturating cast: values above i128::MAX (e.g. u128::MAX) cap at i128::MAX.
            i128::try_from(n).unwrap_or(i128::MAX)
        }
        Value::Tag(3, inner) => {
            // CBOR tag 3: negative bignum, value is -1 - n
            let Value::Bytes(bytes) = inner.as_ref() else {
                panic!("Expected Bytes inside bignum tag 3, got {:?}", inner)
            };
            let mut n = 0u128;
            for b in bytes {
                n = (n << 8) | (*b as u128);
            }
            // Safe: -1 - n where n <= i128::MAX is always representable.
            -1i128 - i128::try_from(n).unwrap_or(i128::MAX)
        }
        _ => panic!("Expected CBOR integer, got {:?}", value),
    }
}

fn cbor_to_i64(value: &Value) -> i64 {
    let n: i128 = cbor_to_i128(value);
    n as i64
}

/// Return true if the CBOR value is a positive bignum (tag 2) whose value exceeds i128::MAX.
fn bignum_overflows_i128(value: &Value) -> bool {
    match value {
        Value::Tag(2, inner) => {
            let Value::Bytes(bytes) = inner.as_ref() else {
                return false;
            };
            // Value overflows i128 if it needs more than 16 bytes, or if the high bit
            // of a 16-byte value is set (i.e. > i128::MAX).
            if bytes.len() > 16 {
                return true;
            }
            if bytes.len() == 16 && bytes[0] >= 0x80 {
                return true;
            }
            // Also check: if any byte beyond what i128 can hold is non-zero.
            let mut n = 0u128;
            for b in bytes {
                n = (n << 8) | (*b as u128);
            }
            n > i128::MAX as u128
        }
        _ => false,
    }
}

/// Encode a u128 value as CBOR. Values up to u64::MAX use normal integer encoding;
/// larger values use CBOR positive bignum tag 2 with big-endian bytes.
fn u128_to_cbor(v: u128) -> Value {
    if let Ok(n) = u64::try_from(v) {
        return Value::Integer(n.into());
    }
    // Encode as CBOR tag 2 (positive bignum), big-endian, minimal encoding.
    let bytes = v.to_be_bytes();
    // Strip leading zero bytes for minimal encoding.
    let first_nonzero = bytes
        .iter()
        .position(|&b| b != 0)
        .unwrap_or(bytes.len() - 1);
    Value::Tag(2, Box::new(Value::Bytes(bytes[first_nonzero..].to_vec())))
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

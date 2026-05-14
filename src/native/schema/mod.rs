// Schema interpreter for the native backend.
//
// Translates CBOR schemas (as sent by hegel generators) into concrete
// values using Hypothesis-style choice recording.
//
// Split into submodules:
//   numeric     — interpret_integer, interpret_boolean, interpret_constant
//   float       — interpret_float
//   text        — interpret_string, interpret_binary, StringAlphabet helpers
//   regex       — interpret_regex, generate_hir_string
//   collections — interpret_list, interpret_dict, interpret_tuple, interpret_one_of, interpret_sampled_from
//   special     — date, time, datetime, ipv4, ipv6, domain, email, url

mod collections;
mod numeric;

use crate::cbor_utils::map_get;
use crate::native::core::{ManyState, NativeTestCase, Status, StopTest};
use ciborium::Value;

/// Interpret a CBOR schema and produce a value using the native test case.
///
/// For leaf schemas (those that don't call `interpret_schema` recursively),
/// records a span in `ntc.spans` so that span-mutation exploration can find
/// structurally-duplicate values (e.g. two equal strings in a tuple).
/// Compound schemas get their spans from the user-level `start_span` /
/// `stop_span` commands that higher-level generators emit.
pub(crate) fn interpret_schema(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
    use crate::cbor_utils::as_text;
    let schema_type = map_get(schema, "type")
        .and_then(as_text)
        .expect("Schema must have a \"type\" field");

    // Record spans for leaf schemas (no recursive interpret_schema calls).
    let is_leaf = matches!(schema_type, "integer" | "boolean" | "sampled_from");
    let span_start = if is_leaf { ntc.nodes.len() } else { 0 };

    // Minimal native: integer + boolean leaves, plus the compound schemas
    // (tuple/list/dict/one_of/sampled_from) that recurse into them. Schemas
    // backed by float/string/binary/regex/datetime/etc. leaves panic with
    // todo!() until those schema interpreters land in a follow-up PR.
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

        "string" | "binary" | "float" | "regex" | "email" | "url" | "domain" | "ip_address"
        | "uuid" | "ipv4" | "ipv6" | "date" | "time" | "datetime" => {
            todo!("schema {:?} not yet supported in native mode", schema_type)
        }

        other => panic!("Unknown schema type: {}", other),
    };
    if is_leaf && result.is_ok() {
        ntc.record_span(span_start, ntc.nodes.len(), schema_type.to_string());
    }
    result
}

/// Advance the many state by one element.  Returns true if another
/// element should be drawn.  Mirrors `Hypothesis`'s `many.more()`.
pub(crate) fn many_more(ntc: &mut NativeTestCase, state: &mut ManyState) -> Result<bool, StopTest> {
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

/// Reject the last drawn element.  Mirrors Hypothesis's `many.reject()`.
pub(crate) fn many_reject(ntc: &mut NativeTestCase, state: &mut ManyState) -> Result<(), StopTest> {
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
pub(super) fn cbor_to_i128(value: &Value) -> i128 {
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

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/mod_tests.rs"]
mod tests;

// Schema interpreter for the native backend.
//
// Translates CBOR schemas (as sent by hegel generators) into concrete
// values using Hypothesis-style choice recording.
//
// Split into submodules:
//   numeric     — interpret_integer, interpret_boolean, interpret_constant
//   float       — interpret_float
//   bytes       — interpret_binary
//   text        — interpret_string, IntervalSet helpers
//   regex       — interpret_regex (Python-compatible regex strategy)
//   collections — interpret_list, interpret_dict, interpret_tuple, interpret_one_of, interpret_sampled_from
//   special     — date, time, datetime, ip_address, uuid
//   internet    — domain, email, url

mod bytes;
mod collections;
mod float;
mod internet;
mod numeric;
mod regex;
mod special;
mod text;

use crate::cbor_utils::map_get;
use crate::native::bignum::{BigInt, Sign, ToPrimitive};
use crate::native::core::state::MAX_DEPTH;
use crate::native::core::{EngineError, ManyState, NativeTestCase, Span, Status};
use ciborium::Value;

/// Look up a required schema field, returning [`EngineError::InvalidArgument`]
/// (rather than panicking) when it is absent. Used for fields whose presence
/// is part of the schema contract — a missing one means the caller's schema
/// is malformed.
pub(super) fn require<'a>(schema: &'a Value, field: &str) -> Result<&'a Value, EngineError> {
    map_get(schema, field).ok_or_else(|| {
        EngineError::InvalidArgument(format!("schema is missing required \"{field}\" field"))
    })
}

/// Interpret a CBOR schema and produce a value using the native test case.
///
/// Records an enclosing span around the dispatch so the shrinker sees every
/// schema's result — leaf or compound — as a logical unit. The span's label
/// is the schema-type string, and nested `interpret_schema` calls correctly
/// attribute their parent through `ntc.span_stack`. Mirrors Hypothesis's
/// `ConjectureData.draw`, which wraps every strategy draw in a matched
/// `start_span` / `stop_span` pair.
pub(crate) fn interpret_schema(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    use crate::cbor_utils::as_text;
    let schema_type = map_get(schema, "type").and_then(as_text).ok_or_else(|| {
        EngineError::InvalidArgument("schema is missing a string \"type\" field".to_string())
    })?;

    // Open a dispatch span. We push a Span directly rather than calling
    // `start_span` because `start_span` takes a u64 label, and schema
    // types are naturally strings. The structural-coverage stack stays
    // untouched — coverage tags are a property of the u64 label space
    // that user-level `start_span` uses.
    let span_idx = ntc.spans.len();
    let span_start = ntc.nodes.len();
    let depth = ntc.span_stack.len() as u32;
    let parent = ntc.span_stack.last().copied();
    ntc.spans.push(Span {
        start: span_start,
        end: span_start,
        label: schema_type.to_string(),
        depth,
        parent,
        discarded: false,
    });
    ntc.span_stack.push(span_idx);
    if depth + 1 > MAX_DEPTH && ntc.status.is_none() {
        ntc.status = Some(Status::Invalid);
        ntc.freeze();
    }

    let result = match schema_type {
        "integer" => numeric::interpret_integer(ntc, schema),
        "boolean" => numeric::interpret_boolean(ntc),
        "constant" => numeric::interpret_constant(schema),
        "null" => Ok(Value::Null),
        "float" => float::interpret_float(ntc, schema),
        "binary" => bytes::interpret_binary(ntc, schema),
        "string" => text::interpret_string(ntc, schema),
        "regex" => regex::interpret_regex(ntc, schema),
        "tuple" => collections::interpret_tuple(ntc, schema),
        "one_of" => collections::interpret_one_of(ntc, schema),
        "sampled_from" => collections::interpret_sampled_from(ntc, schema),
        "list" => collections::interpret_list(ntc, schema),
        "dict" => collections::interpret_dict(ntc, schema),
        "date" => special::interpret_date(ntc),
        "time" => special::interpret_time(ntc),
        "datetime" => special::interpret_datetime(ntc),
        "ip_address" => special::interpret_ip_address(ntc, schema),
        "uuid" => special::interpret_uuid(ntc, schema),
        "domain" => internet::interpret_domain(ntc, schema),
        "email" => internet::interpret_email(ntc),
        "url" => internet::interpret_url(ntc),

        other => Err(EngineError::InvalidArgument(format!(
            "unknown schema type: {other:?}"
        ))),
    };

    ntc.span_stack.pop();
    if let Some(span) = ntc.spans.get_mut(span_idx) {
        span.end = ntc.nodes.len();
    }
    result
}

/// Advance the many state by one element.  Returns true if another
/// element should be drawn.  Mirrors `Hypothesis`'s `many.more()`.
pub(crate) fn many_more(
    ntc: &mut NativeTestCase,
    state: &mut ManyState,
) -> Result<bool, EngineError> {
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
pub(crate) fn many_reject(
    ntc: &mut NativeTestCase,
    state: &mut ManyState,
) -> Result<(), EngineError> {
    assert!(state.count > 0);
    state.count -= 1;
    state.rejections += 1;
    if state.rejections > std::cmp::max(3, 2 * state.count) {
        if state.count < state.min_size {
            ntc.status = Some(Status::Invalid);
            return Err(EngineError::StopTest);
        } else {
            state.force_stop = true;
        }
    }
    Ok(())
}

/// Convert a CBOR value to a [`BigInt`], handling bignum tags. Unlike the old
/// `cbor_to_i128` this is exact — arbitrarily large magnitudes are preserved.
///
/// Returns [`EngineError::InvalidArgument`] for any value that is not a CBOR
/// integer (or a malformed bignum tag), since that means the caller's
/// schema is invalid.
pub(super) fn cbor_to_bigint(value: &Value) -> BigInt {
    match value {
        Value::Integer(i) => BigInt::from(i128::from(*i)),
        Value::Tag(2, inner) => {
            // CBOR tag 2: positive bignum (big-endian bytes).
            let Value::Bytes(bytes) = inner.as_ref() else {
                panic!("Expected Bytes inside bignum tag 2, got {inner:?}");
            };
            BigInt::from_bytes_be(Sign::Plus, bytes)
        }
        Value::Tag(3, inner) => {
            // CBOR tag 3: negative bignum, value is `-1 - n`.
            let Value::Bytes(bytes) = inner.as_ref() else {
                panic!("Expected Bytes inside bignum tag 3, got {inner:?}");
            };
            -BigInt::from_bytes_be(Sign::Plus, bytes) - 1
        }
        _ => panic!("Expected CBOR integer, got {value:?}"),
    }
}

/// Convert a [`BigInt`] to a CBOR value. Values that fit `i64`/`u64` use the
/// direct integer encoding; larger magnitudes use the CBOR bignum tags
/// (2 = non-negative, 3 = negative, each carrying minimal big-endian bytes).
pub(super) fn bigint_to_cbor(v: &BigInt) -> Value {
    if let Some(n) = v.to_i64() {
        return Value::Integer(n.into());
    }
    if let Some(n) = v.to_u64() {
        return Value::Integer(n.into());
    }
    if v.sign() == Sign::Minus {
        // Tag 3 stores `n` where the value is `-1 - n`, i.e. `n = |v| - 1`.
        let n = (-v) - BigInt::from(1);
        let (_sign, bytes) = n.to_bytes_be();
        Value::Tag(3, Box::new(Value::Bytes(bytes)))
    } else {
        let (_sign, bytes) = v.to_bytes_be();
        Value::Tag(2, Box::new(Value::Bytes(bytes)))
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/mod_tests.rs"]
mod tests;

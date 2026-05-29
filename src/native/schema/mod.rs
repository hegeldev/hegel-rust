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
use crate::native::bignum::BigInt;
use crate::native::core::state::MAX_DEPTH;
use crate::native::core::{ManyState, NativeTestCase, Span, Status, StopTest};
use ciborium::Value;

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
) -> Result<Value, StopTest> {
    use crate::cbor_utils::as_text;
    let schema_type = map_get(schema, "type")
        .and_then(as_text)
        .expect("Schema must have a \"type\" field");

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

        other => panic!("Unknown schema type: {}", other),
    };

    ntc.span_stack.pop();
    if let Some(span) = ntc.spans.get_mut(span_idx) {
        span.end = ntc.nodes.len();
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

/// Convert a CBOR value to an arbitrary-precision [`BigInt`], handling bignum
/// tags (tag 2 positive, tag 3 negative) of any width. Unlike [`cbor_to_i128`]
/// this never saturates, so integer choices can carry bounds of any magnitude.
pub(super) fn cbor_to_bigint(value: &Value) -> BigInt {
    match value {
        Value::Integer(i) => BigInt::from(i128::from(*i)),
        Value::Tag(2, inner) => {
            let Value::Bytes(bytes) = inner.as_ref() else {
                panic!("Expected Bytes inside bignum tag 2, got {:?}", inner)
            };
            BigInt::from_unsigned_be_bytes(bytes)
        }
        Value::Tag(3, inner) => {
            // CBOR tag 3: negative bignum, value is `-1 - n`.
            let Value::Bytes(bytes) = inner.as_ref() else {
                panic!("Expected Bytes inside bignum tag 3, got {:?}", inner)
            };
            -BigInt::one() - BigInt::from_unsigned_be_bytes(bytes)
        }
        _ => panic!("Expected CBOR integer, got {:?}", value),
    }
}

/// Encode a [`BigInt`] as CBOR. Values within `i128` reuse [`i128_to_cbor`]
/// (direct integer / serde); larger magnitudes use CBOR bignum tags
/// (tag 2 positive, tag 3 negative `-1 - n`) with big-endian minimal bytes.
pub(super) fn bigint_to_cbor(v: &BigInt) -> Value {
    if let Ok(n) = i128::try_from(v) {
        return i128_to_cbor(n);
    }
    if *v >= BigInt::zero() {
        Value::Tag(2, Box::new(Value::Bytes(v.to_unsigned_be_bytes())))
    } else {
        // value = -1 - n  =>  n = -1 - value = -(value + 1), non-negative.
        let n = -(v + BigInt::one());
        Value::Tag(3, Box::new(Value::Bytes(n.to_unsigned_be_bytes())))
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

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/mod_tests.rs"]
mod tests;

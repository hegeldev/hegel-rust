// Embedded tests for src/native/schema/mod.rs — covers the interpret_schema
// dispatch, many_reject's invalid path, and the CBOR helper functions.

use rand::SeedableRng;
use rand::rngs::SmallRng;

use super::*;
use crate::cbor_utils::cbor_map;
use crate::native::core::NativeTestCase;

fn fresh_ntc() -> NativeTestCase {
    NativeTestCase::new_random(SmallRng::seed_from_u64(1))
}

#[test]
fn interpret_schema_unknown_type_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "mystery" };
    let err = interpret_schema(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("unknown schema type"));
}

#[test]
fn interpret_schema_missing_type_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "min_value" => 0 };
    let err = interpret_schema(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("\"type\""));
}

// ── Every schema dispatch records an enclosing span ─────────────────────────
//
// Without an enclosing span, the basic-generator path (which goes from the
// generator API straight through `interpret_schema` without any user-level
// `start_span`/`stop_span` calls) leaves the shrinker with no way to recognise
// a compound draw as a logical unit.

#[test]
fn interpret_schema_records_leaf_span_for_integer() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "integer",
        "min_value" => 0,
        "max_value" => 0,
    };
    interpret_schema(&mut ntc, &schema).ok().unwrap();

    assert_eq!(ntc.spans.len(), 1);
    assert_eq!(ntc.spans[0usize].label, "integer");
    assert_eq!(ntc.spans[0usize].start, 0);
    assert_eq!(ntc.spans[0usize].end, 1);
    assert_eq!(ntc.spans[0usize].parent, None);
    assert_eq!(ntc.spans[0usize].depth, 0);
}

#[test]
fn interpret_schema_records_enclosing_span_for_tuple() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "tuple",
        "elements" => vec![
            cbor_map! { "type" => "integer", "min_value" => 0, "max_value" => 0 },
            cbor_map! { "type" => "integer", "min_value" => 7, "max_value" => 7 },
        ],
    };
    interpret_schema(&mut ntc, &schema).ok().unwrap();

    // Three spans: the outer tuple plus the two integer children.
    assert_eq!(ntc.spans.len(), 3);

    // The outer tuple was pushed first (so it has the lowest index) and
    // covers all the child nodes.
    assert_eq!(ntc.spans[0usize].label, "tuple");
    assert_eq!(ntc.spans[0usize].start, 0);
    assert_eq!(ntc.spans[0usize].end, 2);
    assert_eq!(ntc.spans[0usize].parent, None);
    assert_eq!(ntc.spans[0usize].depth, 0);

    // The integer children point at the tuple as their parent.
    for child in ntc.spans.iter().skip(1) {
        assert_eq!(child.label, "integer");
        assert_eq!(child.parent, Some(0));
        assert_eq!(child.depth, 1);
    }
}

#[test]
fn interpret_schema_records_enclosing_span_for_list() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "list",
        "elements" => cbor_map! { "type" => "boolean" },
        "min_size" => 0,
        "max_size" => 5,
    };
    interpret_schema(&mut ntc, &schema).ok().unwrap();

    // The outer list span exists at index 0, regardless of how many
    // elements were drawn.
    assert_eq!(ntc.spans[0usize].label, "list");
    assert_eq!(ntc.spans[0usize].parent, None);
    assert_eq!(ntc.spans[0usize].depth, 0);
    for child in ntc.spans.iter().skip(1) {
        assert_eq!(child.parent, Some(0));
        assert_eq!(child.depth, 1);
    }
}

#[test]
fn interpret_schema_records_enclosing_span_for_one_of() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "one_of",
        "generators" => vec![
            cbor_map! { "type" => "integer", "min_value" => 10, "max_value" => 10 },
            cbor_map! { "type" => "integer", "min_value" => 20, "max_value" => 20 },
        ],
    };
    interpret_schema(&mut ntc, &schema).ok().unwrap();

    assert_eq!(ntc.spans[0usize].label, "one_of");
    assert_eq!(ntc.spans[0usize].parent, None);
    assert_eq!(ntc.spans[0usize].depth, 0);
}

#[test]
fn interpret_schema_nests_spans_for_tuple_of_tuples() {
    let mut ntc = fresh_ntc();
    let inner_tuple = cbor_map! {
        "type" => "tuple",
        "elements" => vec![
            cbor_map! { "type" => "integer", "min_value" => 0, "max_value" => 0 },
        ],
    };
    let schema = cbor_map! {
        "type" => "tuple",
        "elements" => vec![inner_tuple.clone(), inner_tuple],
    };
    interpret_schema(&mut ntc, &schema).ok().unwrap();

    // Outer tuple at index 0, depth 0; the two inner tuples have
    // parent = 0, depth = 1; the integer leaves have depth 2.
    assert_eq!(ntc.spans[0usize].label, "tuple");
    assert_eq!(ntc.spans[0usize].depth, 0);

    let inner_tuple_indices: Vec<usize> = ntc
        .spans
        .iter()
        .enumerate()
        .filter_map(|(i, s)| (s.label == "tuple" && i != 0).then_some(i))
        .collect();
    assert_eq!(inner_tuple_indices.len(), 2);
    for &i in &inner_tuple_indices {
        assert_eq!(ntc.spans[i].parent, Some(0));
        assert_eq!(ntc.spans[i].depth, 1);
    }
    for span in ntc.spans.iter().filter(|s| s.label == "integer") {
        assert_eq!(span.depth, 2);
        // The parent must be one of the inner tuples.
        assert!(inner_tuple_indices.contains(&span.parent.unwrap()));
    }
}

#[test]
fn interpret_schema_records_zero_node_span_for_null() {
    // `null` consumes no choice nodes but still represents a logical draw.
    // Recording an empty span keeps parity with start_span/stop_span, which
    // also retain empty spans (see test_has_examples_even_when_empty on the
    // Hypothesis side).
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "null" };
    interpret_schema(&mut ntc, &schema).ok().unwrap();

    assert_eq!(ntc.spans.len(), 1);
    assert_eq!(ntc.spans[0usize].label, "null");
    assert_eq!(ntc.spans[0usize].start, ntc.spans[0usize].end);
}

// ── many_reject: invalid when too many rejections under min_size ────────────

#[test]
fn many_reject_marks_invalid_when_cannot_reach_min_size() {
    let mut ntc = fresh_ntc();
    let mut state = ManyState::new(6, Some(10));
    // Simulate a history where we've already accepted 5 elements and been
    // rejected 9 times: one more rejection drops count to 4 (< min_size=6)
    // while pushing rejections to 10 (> max(3, 2*4) = 8), which should
    // mark the test case invalid.
    state.count = 5;
    state.rejections = 9;

    let result = many_reject(&mut ntc, &mut state);
    assert!(
        result.is_err(),
        "expected StopTest once rejections overflow"
    );
    assert_eq!(ntc.status, Some(Status::Invalid));
}

// ── cbor_to_bigint panic branches ───────────────────────────────────────────

#[test]
#[should_panic(expected = "Expected Bytes inside bignum tag 2")]
fn cbor_to_bigint_tag2_non_bytes_panics() {
    let bad = Value::Tag(2, Box::new(Value::Integer(1.into())));
    cbor_to_bigint(&bad);
}

#[test]
#[should_panic(expected = "Expected Bytes inside bignum tag 3")]
fn cbor_to_bigint_tag3_non_bytes_panics() {
    let bad = Value::Tag(3, Box::new(Value::Integer(1.into())));
    cbor_to_bigint(&bad);
}

#[test]
#[should_panic(expected = "Expected CBOR integer")]
fn cbor_to_bigint_non_integer_panics() {
    cbor_to_bigint(&Value::Bool(true));
}

// ── cbor_to_bigint / bigint_to_cbor round-trips ─────────────────────────────

#[test]
fn cbor_to_bigint_plain_integer() {
    assert_eq!(cbor_to_bigint(&Value::Integer(42.into())), BigInt::from(42));
    assert_eq!(
        cbor_to_bigint(&Value::Integer((-7).into())),
        BigInt::from(-7)
    );
}

#[test]
fn cbor_to_bigint_positive_bignum_tag2() {
    // 0x01_00 big-endian = 256.
    let v = Value::Tag(2, Box::new(Value::Bytes(vec![0x01, 0x00])));
    assert_eq!(cbor_to_bigint(&v), BigInt::from(256));
}

#[test]
fn cbor_to_bigint_negative_bignum_tag3() {
    // tag 3 encodes -1 - n; n = 255 here, so value = -256.
    let v = Value::Tag(3, Box::new(Value::Bytes(vec![0xFF])));
    assert_eq!(cbor_to_bigint(&v), BigInt::from(-256));
}

#[test]
fn bigint_to_cbor_roundtrips_across_magnitudes() {
    // Spans the i64 (negative), u64 (above i64::MAX), positive-bignum
    // (above u64::MAX), and negative-bignum (below i64::MIN) encodings.
    let cases = [
        BigInt::from(0),
        BigInt::from(-5),
        BigInt::from(u64::MAX),
        BigInt::from(u128::MAX),
        BigInt::from(i128::MIN) * BigInt::from(1_000_000),
    ];
    for original in cases {
        let encoded = bigint_to_cbor(&original);
        assert_eq!(cbor_to_bigint(&encoded), original);
    }
}

#[test]
fn interpret_integer_draws_real_bigint_beyond_u128() {
    // Bounds well outside the u128 range force the `BigInt` width (the
    // `draw_in_range` fallback) and the big-range sampler. Looping exercises
    // both the nasty-pool and the uniform (`sample_biguint_at_most`) branches.
    // Seeing a value beyond ±u128::MAX confirms a genuine arbitrary-precision
    // value was generated rather than a saturated one.
    let u128_max = BigInt::from(u128::MAX);
    let min = &u128_max * BigInt::from(-1_000_000);
    let max = &u128_max * BigInt::from(1_000_000);
    let schema = cbor_map! {
        "type" => "integer",
        "min_value" => bigint_to_cbor(&min),
        "max_value" => bigint_to_cbor(&max),
    };
    let neg_u128_max = -&u128_max;
    let mut saw_beyond_u128 = false;
    for seed in 0..200u64 {
        let mut ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(seed));
        let value = interpret_schema(&mut ntc, &schema).ok().unwrap();
        let decoded = cbor_to_bigint(&value);
        assert!(decoded >= min && decoded <= max, "out of range: {decoded}");
        if decoded > u128_max || decoded < neg_u128_max {
            saw_beyond_u128 = true;
        }
    }
    assert!(
        saw_beyond_u128,
        "expected at least one value beyond the u128 range"
    );
}

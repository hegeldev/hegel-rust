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
#[should_panic(expected = "Unknown schema type: mystery")]
fn interpret_schema_unknown_type_panics() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "mystery" };
    let _ = interpret_schema(&mut ntc, &schema);
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

// ── integer schema with bounds beyond i128 ─────────────────────────────────

#[test]
fn interpret_integer_handles_bounds_beyond_i128() {
    use crate::native::bignum::BigInt;
    use ciborium::Value;
    let min = BigInt::from(2).pow(200);
    let max = &min + &BigInt::from(1_000_000);
    let schema = Value::Map(vec![
        (Value::Text("type".into()), Value::Text("integer".into())),
        (Value::Text("min_value".into()), bigint_to_cbor(&min)),
        (Value::Text("max_value".into()), bigint_to_cbor(&max)),
    ]);
    for seed in 0..16u64 {
        let mut ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(seed));
        let result = interpret_schema(&mut ntc, &schema).ok().unwrap();
        let v = cbor_to_bigint(&result);
        assert!(v >= min && v <= max, "out of range: {v}");
    }
}

// ── cbor_to_bigint panic branches ───────────────────────────────────────────

#[test]
#[should_panic(expected = "Expected Bytes inside bignum tag 2")]
fn cbor_to_bigint_tag2_non_bytes_panics() {
    let bad = Value::Tag(2, Box::new(Value::Integer(1.into())));
    let _ = cbor_to_bigint(&bad);
}

#[test]
#[should_panic(expected = "Expected Bytes inside bignum tag 3")]
fn cbor_to_bigint_tag3_non_bytes_panics() {
    let bad = Value::Tag(3, Box::new(Value::Integer(1.into())));
    let _ = cbor_to_bigint(&bad);
}

#[test]
#[should_panic(expected = "Expected CBOR integer")]
fn cbor_to_bigint_non_integer_panics() {
    let _ = cbor_to_bigint(&Value::Bool(true));
}

// ── cbor_to_bigint / bigint_to_cbor round-trips ─────────────────────────────

#[test]
fn cbor_bigint_round_trips_across_widths() {
    use crate::native::bignum::BigInt;
    // Small ints, u64-range, beyond-i128 positive (tag 2), and negative
    // beyond-i128 (tag 3) all round-trip through CBOR.
    let cases = [
        BigInt::from(0),
        BigInt::from(-1),
        BigInt::from(u64::MAX),
        BigInt::from(u128::MAX),
        BigInt::from(i128::MIN),
        BigInt::from(u128::MAX) * BigInt::from(u128::MAX),
        -(BigInt::from(u128::MAX) * BigInt::from(u128::MAX)),
    ];
    for v in cases {
        assert_eq!(cbor_to_bigint(&bigint_to_cbor(&v)), v);
    }
}

// Embedded tests for src/native/schema/collections.rs — exercise each
// interpret_* function with representative schemas. Tests drive the
// NativeTestCase with a deterministic RNG.

use super::*;
use crate::cbor_utils::cbor_map;
use crate::native::core::NativeTestCase;
use crate::native::rng::EngineRng;

fn fresh_ntc() -> NativeTestCase {
    NativeTestCase::new_random(EngineRng::seeded(7))
}

// ── interpret_tuple ─────────────────────────────────────────────────────────

#[test]
fn interpret_tuple_returns_array_of_elements() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "tuple",
        "elements" => vec![
            cbor_map! { "type" => "integer", "min_value" => 0, "max_value" => 0 },
            cbor_map! { "type" => "integer", "min_value" => 7, "max_value" => 7 },
        ],
    };
    let result = interpret_tuple(&mut ntc, &schema).ok().unwrap();
    let Value::Array(items) = result else {
        panic!("expected array")
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0], Value::Integer(0.into()));
    assert_eq!(items[1], Value::Integer(7.into()));
}

#[test]
fn interpret_tuple_without_elements_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "tuple" };
    let err = interpret_tuple(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("elements"));
}

#[test]
fn interpret_tuple_non_array_elements_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "tuple", "elements" => 7 };
    let err = interpret_tuple(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("must be an array"));
}

// ── interpret_one_of ────────────────────────────────────────────────────────

#[test]
fn interpret_one_of_selects_a_branch() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "one_of",
        "generators" => vec![
            cbor_map! { "type" => "integer", "min_value" => 10, "max_value" => 10 },
            cbor_map! { "type" => "integer", "min_value" => 20, "max_value" => 20 },
        ],
    };
    let result = interpret_one_of(&mut ntc, &schema).ok().unwrap();
    let Value::Array(items) = result else {
        panic!("expected [index, value] array")
    };
    assert_eq!(items.len(), 2);
    let Value::Integer(ref idx) = items[0] else {
        panic!("expected integer index")
    };
    let idx: i128 = (*idx).into();
    assert!(idx == 0 || idx == 1);
    let Value::Integer(ref val) = items[1] else {
        panic!("expected integer value")
    };
    let val: i128 = (*val).into();
    assert_eq!(idx == 0, val == 10);
    assert_eq!(idx == 1, val == 20);
}

#[test]
fn interpret_one_of_without_generators_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "one_of" };
    let err = interpret_one_of(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("generators"));
}

#[test]
fn interpret_one_of_empty_generators_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "one_of",
        "generators" => Vec::<Value>::new(),
    };
    let err = interpret_one_of(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("non-empty"));
}

// ── interpret_sampled_from ──────────────────────────────────────────────────

#[test]
fn interpret_sampled_from_returns_a_value_and_wraps_text() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "sampled_from",
        "values" => vec![Value::Text("hi".into())],
    };
    let result = interpret_sampled_from(&mut ntc, &schema).ok().unwrap();
    // Text values are tagged with HEGEL_STRING_TAG (91).
    match result {
        Value::Tag(91, boxed) => {
            let Value::Bytes(bytes) = *boxed else {
                panic!("expected bytes inside tag 91")
            };
            assert_eq!(bytes, b"hi".to_vec());
        }
        other => panic!("expected tagged bytes, got {:?}", other),
    }
}

#[test]
fn interpret_sampled_from_returns_non_text_as_is() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "sampled_from",
        "values" => vec![Value::Integer(42.into())],
    };
    let result = interpret_sampled_from(&mut ntc, &schema).ok().unwrap();
    assert_eq!(result, Value::Integer(42.into()));
}

#[test]
fn interpret_sampled_from_without_values_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "sampled_from" };
    let err = interpret_sampled_from(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("values"));
}

#[test]
fn interpret_sampled_from_empty_values_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "sampled_from",
        "values" => Vec::<Value>::new(),
    };
    let err = interpret_sampled_from(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("non-empty"));
}

// ── interpret_list ──────────────────────────────────────────────────────────

#[test]
fn interpret_list_without_elements_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "list" };
    let err = interpret_list(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("elements"));
}

#[test]
fn interpret_list_unique_with_invalid_integer_bounds_falls_back() {
    // A `unique` integer list whose bounds are not CBOR integers can't take
    // the bounded-range fast path (`bounded_integer_range` returns `None`);
    // the generic path then surfaces the real `InvalidArgument` when it
    // interprets the element schema.
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "list",
        "unique" => true,
        "min_size" => 1u64,
        "elements" => cbor_map! {
            "type" => "integer", "min_value" => "lo", "max_value" => 5,
        },
    };
    let err = interpret_list(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
}

#[test]
fn interpret_list_with_fixed_size_returns_that_many_elements() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "list",
        "elements" => cbor_map! {
            "type" => "integer", "min_value" => 3, "max_value" => 3,
        },
        "min_size" => 2u64,
        "max_size" => 2u64,
    };
    let result = interpret_list(&mut ntc, &schema).ok().unwrap();
    let Value::Array(items) = result else {
        panic!("expected array")
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0], Value::Integer(3.into()));
}

#[test]
fn interpret_list_unique_rejects_duplicates() {
    // Force duplicates with a 1-element domain, bounded size 1..=1, and unique.
    // With max == min == 1 we fill to the minimum without triggering rejections.
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "list",
        "elements" => cbor_map! {
            "type" => "integer", "min_value" => 0, "max_value" => 0,
        },
        "min_size" => 1u64,
        "max_size" => 1u64,
        "unique" => true,
    };
    let result = interpret_list(&mut ntc, &schema).ok().unwrap();
    let Value::Array(items) = result else {
        panic!("expected array")
    };
    assert_eq!(items, vec![Value::Integer(0.into())]);
}

#[test]
fn interpret_list_unique_rejects_duplicates_over_min() {
    // Variable-size unique list over a 1-element domain: second draw
    // duplicates, so the reject path fires and the list caps at one element.
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "list",
        "elements" => cbor_map! {
            "type" => "integer", "min_value" => 0, "max_value" => 0,
        },
        "min_size" => 0u64,
        "max_size" => 5u64,
        "unique" => true,
    };
    let result = interpret_list(&mut ntc, &schema).ok().unwrap();
    let Value::Array(items) = result else {
        panic!("expected array")
    };
    // Either empty or containing exactly the single possible value.
    assert!(items.len() <= 1);
    if !items.is_empty() {
        assert_eq!(items[0], Value::Integer(0.into()));
    }
}

#[test]
fn interpret_list_unique_large_range_falls_back_to_rejection() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "list",
        "elements" => cbor_map! {
            "type" => "integer", "min_value" => 0, "max_value" => 10_000u64,
        },
        "min_size" => 1u64,
        "max_size" => 1u64,
        "unique" => true,
    };
    let result = interpret_list(&mut ntc, &schema).ok().unwrap();
    let Value::Array(items) = result else {
        panic!("expected array")
    };
    assert_eq!(items.len(), 1);
}

// ── interpret_dict ──────────────────────────────────────────────────────────

#[test]
fn interpret_dict_without_keys_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "dict" };
    let err = interpret_dict(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("keys"));
}

#[test]
fn interpret_dict_without_values_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "dict",
        "keys" => cbor_map! { "type" => "integer", "min_value" => 0, "max_value" => 1 },
    };
    let err = interpret_dict(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("values"));
}

#[test]
fn interpret_dict_with_fixed_size_returns_pairs() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "dict",
        "keys" => cbor_map! {
            "type" => "integer", "min_value" => 0, "max_value" => 100,
        },
        "values" => cbor_map! {
            "type" => "integer", "min_value" => 1, "max_value" => 1,
        },
        "min_size" => 1u64,
        "max_size" => 1u64,
    };
    let result = interpret_dict(&mut ntc, &schema).ok().unwrap();
    let Value::Array(pairs) = result else {
        panic!("expected array")
    };
    assert_eq!(pairs.len(), 1);
    let Value::Array(kv) = &pairs[0] else {
        panic!("expected array")
    };
    assert_eq!(kv.len(), 2);
    assert_eq!(kv[1], Value::Integer(1.into()));
}

#[test]
fn interpret_dict_rejects_duplicate_keys() {
    // Keys come from a 1-element domain so every draw after the first
    // duplicates — exercises the reject-on-duplicate-key branch.
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "dict",
        "keys" => cbor_map! {
            "type" => "integer", "min_value" => 0, "max_value" => 0,
        },
        "values" => cbor_map! {
            "type" => "integer", "min_value" => 9, "max_value" => 9,
        },
        "min_size" => 0u64,
        "max_size" => 5u64,
    };
    let result = interpret_dict(&mut ntc, &schema).ok().unwrap();
    let Value::Array(pairs) = result else {
        panic!("expected array")
    };
    assert!(pairs.len() <= 1);
}

// ── encode_schema_value ─────────────────────────────────────────────────────

#[test]
fn encode_schema_value_wraps_text_in_tag_91() {
    let wrapped = encode_schema_value(&Value::Text("abc".into()));
    let Value::Tag(tag, boxed) = wrapped else {
        panic!("expected tag")
    };
    assert_eq!(tag, 91);
    let Value::Bytes(bytes) = *boxed else {
        panic!("expected bytes")
    };
    assert_eq!(bytes, b"abc".to_vec());
}

#[test]
fn encode_schema_value_passes_non_text_through() {
    let v = Value::Integer(5.into());
    assert_eq!(encode_schema_value(&v), v);
}

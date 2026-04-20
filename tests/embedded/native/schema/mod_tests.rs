// Embedded tests for src/native/schema/mod.rs — covers the dispatch_request
// dispatcher, many_reject's invalid path, and the CBOR helper functions.

use rand::SeedableRng;
use rand::rngs::SmallRng;

use super::*;
use crate::cbor_utils::cbor_map;
use crate::native::core::NativeTestCase;

fn fresh_ntc() -> NativeTestCase {
    NativeTestCase::new_random(SmallRng::seed_from_u64(1))
}

// ── dispatch_request: pool_consume ──────────────────────────────────────────

#[test]
fn pool_consume_removes_variable_from_pool() {
    let mut ntc = fresh_ntc();
    let pool_payload = cbor_map! {};
    let pool_id = dispatch_request(&mut ntc, "new_pool", &pool_payload)
        .ok()
        .unwrap();
    let Value::Integer(pool_id) = pool_id else {
        panic!("expected integer pool id")
    };
    let pool_id_cbor = Value::Integer(pool_id);
    let add_payload = cbor_map! { "pool_id" => pool_id_cbor.clone() };
    let variable_id = dispatch_request(&mut ntc, "pool_add", &add_payload)
        .ok()
        .unwrap();
    let Value::Integer(variable_id) = variable_id else {
        panic!("expected integer variable id")
    };

    let consume_payload = cbor_map! {
        "pool_id" => pool_id_cbor.clone(),
        "variable_id" => Value::Integer(variable_id),
    };
    let result = dispatch_request(&mut ntc, "pool_consume", &consume_payload)
        .ok()
        .unwrap();
    assert_eq!(result, Value::Null);

    // After consumption, the pool is empty and pool_generate returns StopTest.
    let gen_payload = cbor_map! { "pool_id" => pool_id_cbor };
    let err = dispatch_request(&mut ntc, "pool_generate", &gen_payload);
    assert!(
        err.is_err(),
        "pool_generate on empty pool must signal StopTest"
    );
}

// ── dispatch_request: unknown commands / schemas ────────────────────────────

#[test]
#[should_panic(expected = "Unknown native command: nope")]
fn dispatch_request_unknown_command_panics() {
    let mut ntc = fresh_ntc();
    let _ = dispatch_request(&mut ntc, "nope", &cbor_map! {});
}

#[test]
#[should_panic(expected = "Unknown schema type: mystery")]
fn interpret_schema_unknown_type_panics() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "mystery" };
    let generate_payload = cbor_map! { "schema" => schema };
    let _ = dispatch_request(&mut ntc, "generate", &generate_payload);
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

// ── cbor_to_i128 panic branches ─────────────────────────────────────────────

#[test]
#[should_panic(expected = "Expected Bytes inside bignum tag 2")]
fn cbor_to_i128_tag2_non_bytes_panics() {
    let bad = Value::Tag(2, Box::new(Value::Integer(1.into())));
    let _ = cbor_to_i128(&bad);
}

#[test]
#[should_panic(expected = "Expected Bytes inside bignum tag 3")]
fn cbor_to_i128_tag3_non_bytes_panics() {
    let bad = Value::Tag(3, Box::new(Value::Integer(1.into())));
    let _ = cbor_to_i128(&bad);
}

#[test]
#[should_panic(expected = "Expected CBOR integer")]
fn cbor_to_i128_non_integer_panics() {
    let _ = cbor_to_i128(&Value::Bool(true));
}

// ── bignum_overflows_i128 branches ──────────────────────────────────────────

#[test]
fn bignum_overflows_i128_false_for_non_tag2() {
    assert!(!bignum_overflows_i128(&Value::Integer(5.into())));
    // Tag 2 with malformed (non-Bytes) inner also reports false.
    let malformed = Value::Tag(2, Box::new(Value::Integer(1.into())));
    assert!(!bignum_overflows_i128(&malformed));
}

#[test]
fn bignum_overflows_i128_true_for_more_than_16_bytes() {
    let bytes = vec![0xFFu8; 17];
    let big = Value::Tag(2, Box::new(Value::Bytes(bytes)));
    assert!(bignum_overflows_i128(&big));
}

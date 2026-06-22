// Embedded tests for src/native/schema/numeric.rs — focuses on the
// caller-reachable `InvalidArgument` paths for malformed integer/constant
// schemas. The happy paths are covered by the integration generator tests.

use super::*;
use crate::cbor_utils::cbor_map;
use crate::native::core::NativeTestCase;
use crate::native::rng::EngineRng;

fn fresh_ntc() -> NativeTestCase {
    NativeTestCase::new_random(EngineRng::seeded(0))
}

#[test]
fn interpret_integer_missing_min_value_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "integer", "max_value" => 10 };
    let err = interpret_integer(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("min_value"));
}

#[test]
fn interpret_integer_missing_max_value_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "integer", "min_value" => 0 };
    let err = interpret_integer(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("max_value"));
}

#[test]
fn interpret_integer_non_integer_bound_is_invalid_argument() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "integer", "min_value" => "lo", "max_value" => 10 };
    let err = interpret_integer(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("CBOR integer"));
}

#[test]
fn interpret_constant_missing_value_is_invalid_argument() {
    let schema = cbor_map! { "type" => "constant" };
    let err = interpret_constant(&schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("value"));
}

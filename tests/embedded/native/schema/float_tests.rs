use super::*;
use crate::native::rng::EngineRng;

#[test]
fn cbor_to_f64_from_integer() {
    assert_eq!(cbor_to_f64(&Value::Integer(42.into())).unwrap(), 42.0);
}

#[test]
fn cbor_to_f64_from_negative_integer() {
    assert_eq!(cbor_to_f64(&Value::Integer((-7i64).into())).unwrap(), -7.0);
}

#[test]
fn cbor_to_f64_non_numeric_is_invalid_argument() {
    let err = cbor_to_f64(&Value::Bool(true)).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("CBOR float or integer"));
}

// interpret_float must reject widths outside `{32, 64}`: Hypothesis only
// supports `{16, 32, 64}` and we have no Rust `f16` to back width 16, so
// the schema interpreter fails loud at the boundary rather than silently
// treating unknown widths as f64.

#[test]
fn interpret_float_rejects_width_16() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let schema = cbor_map! { "type" => "float", "width" => 16 };
    let err = interpret_float(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("unsupported float width"));
}

#[test]
fn interpret_float_rejects_width_128() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let schema = cbor_map! { "type" => "float", "width" => 128 };
    let err = interpret_float(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("unsupported float width"));
}

#[test]
fn interpret_float_non_numeric_bound_is_invalid_argument() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let schema = cbor_map! { "type" => "float", "min_value" => "low" };
    let err = interpret_float(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("CBOR float or integer"));
}

#[test]
fn interpret_float_accepts_width_32_and_64() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let schema_64 = cbor_map! { "type" => "float", "width" => 64 };
    assert!(interpret_float(&mut ntc, &schema_64).is_ok());

    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let schema_32 = cbor_map! { "type" => "float", "width" => 32 };
    assert!(interpret_float(&mut ntc, &schema_32).is_ok());
}

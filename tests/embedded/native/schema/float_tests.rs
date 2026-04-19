use super::*;

#[test]
fn cbor_to_f64_from_integer() {
    assert_eq!(cbor_to_f64(&Value::Integer(42.into())), 42.0);
}

#[test]
fn cbor_to_f64_from_negative_integer() {
    assert_eq!(cbor_to_f64(&Value::Integer((-7i64).into())), -7.0);
}

#[test]
#[should_panic(expected = "Expected CBOR float/integer")]
fn cbor_to_f64_panics_on_non_numeric() {
    let _ = cbor_to_f64(&Value::Bool(true));
}

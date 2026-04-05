use super::*;

#[test]
fn test_deserialize_f64() {
    let v = HegelValue::Number(42.5);
    let result: f64 = from_hegel_value(v).unwrap();
    assert_eq!(result, 42.5);
}

#[test]
fn test_deserialize_nan() {
    let v = HegelValue::Number(f64::NAN);
    let result: f64 = from_hegel_value(v).unwrap();
    assert!(result.is_nan());
}

#[test]
fn test_deserialize_infinity() {
    let v = HegelValue::Number(f64::INFINITY);
    let result: f64 = from_hegel_value(v).unwrap();
    assert!(result.is_infinite() && result.is_sign_positive());
}

#[test]
fn test_deserialize_neg_infinity() {
    let v = HegelValue::Number(f64::NEG_INFINITY);
    let result: f64 = from_hegel_value(v).unwrap();
    assert!(result.is_infinite() && result.is_sign_negative());
}

#[test]
fn test_deserialize_vec_f64() {
    let v = HegelValue::Array(vec![
        HegelValue::Number(1.0),
        HegelValue::Number(f64::NAN),
        HegelValue::Number(f64::INFINITY),
    ]);
    let result: Vec<f64> = from_hegel_value(v).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0], 1.0);
    assert!(result[1].is_nan());
    assert!(result[2].is_infinite());
}

#[test]
fn test_from_ciborium_nan() {
    let cbor = ciborium::Value::Float(f64::NAN);
    let hegel = HegelValue::from(cbor);
    if let HegelValue::Number(n) = hegel {
        assert!(n.is_nan());
    } else {
        panic!("expected Number"); // nocov
    }
}

#[test]
fn test_from_ciborium_infinity() {
    let cbor = ciborium::Value::Float(f64::INFINITY);
    let hegel = HegelValue::from(cbor);
    let result: f64 = from_hegel_value(hegel).unwrap();
    assert!(result.is_infinite() && result.is_sign_positive());
}

#[test]
fn test_from_ciborium_neg_infinity() {
    let cbor = ciborium::Value::Float(f64::NEG_INFINITY);
    let hegel = HegelValue::from(cbor);
    let result: f64 = from_hegel_value(hegel).unwrap();
    assert!(result.is_infinite() && result.is_sign_negative());
}

#[test]
fn test_from_ciborium_big_integer() {
    // Value larger than 2^53
    let cbor = ciborium::Value::Integer(9223372036854776833u64.into());
    let hegel = HegelValue::from(cbor);
    let result: u64 = from_hegel_value(hegel).unwrap();
    assert_eq!(result, 9223372036854776833u64);
}

#[test]
fn test_from_ciborium_array_with_nan() {
    let cbor = ciborium::Value::Array(vec![
        ciborium::Value::Float(1.0),
        ciborium::Value::Float(f64::NAN),
        ciborium::Value::Float(f64::INFINITY),
        ciborium::Value::Float(f64::NEG_INFINITY),
    ]);
    let hegel = HegelValue::from(cbor);
    let result: Vec<f64> = from_hegel_value(hegel).unwrap();
    assert_eq!(result[0], 1.0);
    assert!(result[1].is_nan());
    assert!(result[2].is_infinite() && result[2].is_sign_positive());
    assert!(result[3].is_infinite() && result[3].is_sign_negative());
}

#[test]
fn test_deserialize_struct() {
    #[derive(serde::Deserialize, Debug)]
    struct TestStruct {
        value: f64,
        name: String,
    }

    let v = HegelValue::Object(HashMap::from([
        ("value".to_string(), HegelValue::Number(f64::NAN)),
        ("name".to_string(), HegelValue::String("test".to_string())),
    ]));
    let result: TestStruct = from_hegel_value(v).unwrap();
    assert!(result.value.is_nan());
    assert_eq!(result.name, "test");
}

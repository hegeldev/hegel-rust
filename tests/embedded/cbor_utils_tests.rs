use super::*;

#[test]
fn test_cbor_map_macro() {
    let m = cbor_map! {
        "type" => "integer",
        "min_value" => 0
    };
    assert_eq!(as_text(map_get(&m, "type").unwrap()), Some("integer"));
    assert_eq!(as_u64(map_get(&m, "min_value").unwrap()), Some(0));
}

#[test]
fn test_cbor_array_macro() {
    let a = cbor_array![Value::from("a"), Value::from("b")];
    if let Value::Array(items) = &a {
        assert_eq!(items.len(), 2);
    } else {
        panic!("expected array"); // nocov
    }
}

#[test]
fn test_map_insert() {
    let mut m = cbor_map! { "a" => 1 };
    map_insert(&mut m, "b", 2);
    assert_eq!(as_u64(map_get(&m, "b").unwrap()), Some(2));

    map_insert(&mut m, "a", 10);
    assert_eq!(as_u64(map_get(&m, "a").unwrap()), Some(10));
}

#[test]
fn test_as_bool() {
    assert_eq!(as_bool(&Value::Bool(true)), Some(true));
    assert_eq!(as_bool(&Value::Bool(false)), Some(false));
    assert_eq!(as_bool(&Value::from(42)), None);
}

#[test]
fn test_map_extend() {
    let mut target = cbor_map! { "a" => 1, "b" => 2 };
    let source = cbor_map! { "b" => 20, "c" => 3 };
    map_extend(&mut target, source);
    assert_eq!(as_u64(map_get(&target, "a").unwrap()), Some(1));
    assert_eq!(as_u64(map_get(&target, "b").unwrap()), Some(20));
    assert_eq!(as_u64(map_get(&target, "c").unwrap()), Some(3));
}

#[test]
#[should_panic(expected = "expected Value::Map")]
fn test_map_extend_non_map_source() {
    let mut target = cbor_map! { "a" => 1 };
    map_extend(&mut target, Value::from(42));
}

#[test]
#[should_panic(expected = "expected Value::Text")]
fn test_map_extend_non_text_key() {
    let mut target = cbor_map! { "a" => 1 };
    let source = Value::Map(vec![(Value::from(42), Value::from("val"))]);
    map_extend(&mut target, source);
}

#[test]
fn test_cbor_serialize() {
    let v = cbor_serialize(&42i32);
    assert_eq!(as_u64(&v), Some(42));

    let v = cbor_serialize(&"hello");
    assert_eq!(as_text(&v), Some("hello"));
}

fn roundtrip(value: &Value) -> Value {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf).unwrap();
    read_value(&mut &buf[..]).unwrap()
}

#[test]
fn test_integers() {
    assert_eq!(roundtrip(&Value::from(0)), Value::from(0));
    assert_eq!(roundtrip(&Value::from(42)), Value::from(42));
    assert_eq!(roundtrip(&Value::from(-1)), Value::from(-1));
    assert_eq!(roundtrip(&Value::from(i64::MAX)), Value::from(i64::MAX));
    assert_eq!(roundtrip(&Value::from(i64::MIN)), Value::from(i64::MIN));
}

#[test]
fn test_strings() {
    let v = Value::Text("hello".into());
    assert_eq!(roundtrip(&v), v);
}

#[test]
fn test_bytes() {
    let v = Value::Bytes(vec![1, 2, 3]);
    assert_eq!(roundtrip(&v), v);
}

#[test]
fn test_array() {
    let v = Value::Array(vec![Value::from(1), Value::Text("two".into())]);
    assert_eq!(roundtrip(&v), v);
}

#[test]
fn test_map() {
    let v = Value::Map(vec![
        (Value::Text("key".into()), Value::from(42)),
        (Value::Text("other".into()), Value::Bool(true)),
    ]);
    assert_eq!(roundtrip(&v), v);
}

#[test]
fn test_bignum_tags_preserved() {
    let big_bytes = {
        let mut b = vec![1u8];
        b.extend(std::iter::repeat_n(0u8, 16));
        b
    };
    let v = Value::Tag(2, Box::new(Value::Bytes(big_bytes.clone())));
    let mut buf = Vec::new();
    ciborium::into_writer(&v, &mut buf).unwrap();
    let parsed = read_value(&mut &buf[..]).unwrap();
    assert_eq!(parsed, Value::Tag(2, Box::new(Value::Bytes(big_bytes))));
}

#[test]
fn test_floats() {
    let v = Value::Float(std::f64::consts::PI);
    let parsed = roundtrip(&v);
    if let Value::Float(f) = parsed {
        assert_eq!(f, std::f64::consts::PI);
    } else {
        panic!("expected float");
    }
}

#[test]
fn test_booleans_and_null() {
    assert_eq!(roundtrip(&Value::Bool(true)), Value::Bool(true));
    assert_eq!(roundtrip(&Value::Bool(false)), Value::Bool(false));
    assert_eq!(roundtrip(&Value::Null), Value::Null);
}

#[test]
fn test_invalid_additional_info() {
    // additional = 28 (0x1c) is reserved/invalid; major 0 with additional 28 = 0x1c
    let data = [0x1c_u8];
    let err = read_value(&mut &data[..]).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("invalid CBOR additional info"));
}

#[test]
fn test_indefinite_bytes() {
    // Major 2, additional 31 = 0x5f (indefinite-length byte string)
    // Two chunks: 0x42 (2 bytes) [0x01, 0x02] and 0x41 (1 byte) [0x03], then 0xff break
    let data = [0x5f, 0x42, 0x01, 0x02, 0x41, 0x03, 0xff];
    let v = read_value(&mut &data[..]).unwrap();
    assert_eq!(v, Value::Bytes(vec![1, 2, 3]));
}

#[test]
fn test_indefinite_text() {
    // Major 3, additional 31 = 0x7f (indefinite-length text string)
    // Two chunks: "he" and "llo", then 0xff break
    let data = [0x7f, 0x62, b'h', b'e', 0x63, b'l', b'l', b'o', 0xff];
    let v = read_value(&mut &data[..]).unwrap();
    assert_eq!(v, Value::Text("hello".into()));
}

#[test]
fn test_indefinite_array() {
    // Major 4, additional 31 = 0x9f (indefinite-length array)
    // Contains: 0x01 (integer 1), 0x02 (integer 2), then 0xff break
    let data = [0x9f, 0x01, 0x02, 0xff];
    let v = read_value(&mut &data[..]).unwrap();
    assert_eq!(v, Value::Array(vec![Value::from(1), Value::from(2)]));
}

#[test]
fn test_indefinite_map() {
    // Major 5, additional 31 = 0xbf (indefinite-length map)
    // Contains: key 0x01, value 0x02, then 0xff break
    let data = [0xbf, 0x01, 0x02, 0xff];
    let v = read_value(&mut &data[..]).unwrap();
    assert_eq!(v, Value::Map(vec![(Value::from(1), Value::from(2))]));
}

#[test]
fn test_negative_bignum_overflow() {
    // Major 1 with argument > i64::MAX triggers the Tag(3, ...) path.
    // Use additional=27 (8-byte arg) with value = u64::MAX (0xffffffffffffffff)
    // That's initial byte 0x3b followed by 8 bytes of 0xff.
    // The result should be Tag(3, Bytes([0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]))
    let data = [0x3b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
    let v = read_value(&mut &data[..]).unwrap();
    // v = u64::MAX, bytes = v.to_be_bytes() = [0xff; 8], start = 0
    assert_eq!(v, Value::Tag(3, Box::new(Value::Bytes(vec![0xff; 8]))));
}

#[test]
fn test_negative_bignum_overflow_leading_zeros() {
    // Major 1, additional=27, value = i64::MAX as u64 + 1 = 0x8000000000000000
    let data = [0x3b, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let v = read_value(&mut &data[..]).unwrap();
    // bytes = [0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], start = 0
    assert_eq!(
        v,
        Value::Tag(
            3,
            Box::new(Value::Bytes(vec![
                0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            ]))
        )
    );
}

#[test]
fn test_two_byte_simple_false() {
    // Major 7, additional 24 = 0xf8, then value 20 (false)
    let data = [0xf8, 20];
    let v = read_value(&mut &data[..]).unwrap();
    assert_eq!(v, Value::Bool(false));
}

#[test]
fn test_two_byte_simple_true() {
    // Major 7, additional 24 = 0xf8, then value 21 (true)
    let data = [0xf8, 21];
    let v = read_value(&mut &data[..]).unwrap();
    assert_eq!(v, Value::Bool(true));
}

#[test]
fn test_two_byte_simple_other_is_null() {
    // Major 7, additional 24 = 0xf8, then value 255 (unassigned simple)
    let data = [0xf8, 255];
    let v = read_value(&mut &data[..]).unwrap();
    assert_eq!(v, Value::Null);
}

#[test]
fn test_float32() {
    // Major 7, additional 26 = 0xfa, then 4-byte f32
    let f: f32 = 1.5;
    let bytes = f.to_be_bytes();
    let data = [0xfa, bytes[0], bytes[1], bytes[2], bytes[3]];
    let v = read_value(&mut &data[..]).unwrap();
    assert_eq!(v, Value::Float(1.5));
}

#[test]
fn test_unsupported_simple_value() {
    // Major 7, additional 28 = 0xfc (reserved/unsupported)
    let data = [0xfc_u8];
    let err = read_value(&mut &data[..]).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("unsupported simple value"));
}

#[test]
fn test_nested_map_with_bignum() {
    let big_bytes = vec![0xFF; 17];
    let v = Value::Map(vec![(
        Value::Text("result".into()),
        Value::Tag(2, Box::new(Value::Bytes(big_bytes.clone()))),
    )]);
    let mut buf = Vec::new();
    ciborium::into_writer(&v, &mut buf).unwrap();
    let parsed = read_value(&mut &buf[..]).unwrap();
    assert_eq!(
        parsed,
        Value::Map(vec![(
            Value::Text("result".into()),
            Value::Tag(2, Box::new(Value::Bytes(big_bytes))),
        )])
    );
}

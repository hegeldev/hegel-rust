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

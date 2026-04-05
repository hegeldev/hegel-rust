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
fn test_cbor_serialize() {
    let v = cbor_serialize(&42i32);
    assert_eq!(as_u64(&v), Some(42));

    let v = cbor_serialize(&"hello");
    assert_eq!(as_text(&v), Some("hello"));
}

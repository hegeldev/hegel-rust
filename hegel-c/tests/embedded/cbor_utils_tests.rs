use super::*;

#[test]
fn test_map_get() {
    let m = Value::Map(vec![
        (Value::from("type"), Value::from("integer")),
        (Value::from("min_value"), Value::from(0)),
    ]);
    assert_eq!(as_text(map_get(&m, "type").unwrap()), Some("integer"));
    assert_eq!(as_u64(map_get(&m, "min_value").unwrap()), Some(0));
    assert!(map_get(&m, "missing").is_none());
}

#[test]
fn test_as_bool() {
    assert_eq!(as_bool(&Value::Bool(true)), Some(true));
    assert_eq!(as_bool(&Value::Bool(false)), Some(false));
    assert_eq!(as_bool(&Value::from(42)), None);
}

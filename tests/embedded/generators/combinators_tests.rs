use super::super::Generator;
use super::*;
use crate::cbor_utils::{as_text, map_get};
use ciborium::Value;

/// Get the `"type"` field of a CBOR-encoded schema as a string.
fn schema_type(schema: &Value) -> &str {
    as_text(map_get(schema, "type").unwrap()).unwrap()
}

/// Get the `"generators"` array of a `one_of` schema.
fn one_of_children(schema: &Value) -> &[Value] {
    let Value::Array(items) = map_get(schema, "generators").unwrap() else {
        panic!("expected array, got {:?}", schema);
    };
    items
}

#[test]
fn test_one_of_schema_is_flat_one_of() {
    let g = one_of(vec![
        super::super::booleans().boxed(),
        super::super::booleans().boxed(),
        super::super::booleans().boxed(),
    ]);
    let basic = g.as_basic().expect("one_of of basics should be basic");
    let schema = basic.schema();

    assert_eq!(schema_type(schema), "one_of");

    // Children are emitted directly with no tagged-tuple wrapping.
    let children = one_of_children(schema);
    assert_eq!(children.len(), 3);
    for child in children {
        assert_eq!(schema_type(child), "boolean");
    }
}

#[test]
fn test_one_of_basic_dispatches_by_index() {
    // Two children with map transforms; the wire response selects which.
    let g = one_of(vec![
        super::super::booleans()
            .map(|_| "first".to_string())
            .boxed(),
        super::super::booleans()
            .map(|_| "second".to_string())
            .boxed(),
    ]);
    let basic = g.as_basic().expect("one_of of basics should be basic");

    // Simulate the server response: [index, value].
    let raw_first = Value::Array(vec![Value::Integer(0.into()), Value::Bool(false)]);
    let raw_second = Value::Array(vec![Value::Integer(1.into()), Value::Bool(true)]);

    assert_eq!(basic.parse_raw(raw_first), "first");
    assert_eq!(basic.parse_raw(raw_second), "second");
}

#[test]
fn test_optional_schema_is_flat_one_of() {
    let g = optional(super::super::booleans());
    let basic = g.as_basic().unwrap();
    let schema = basic.schema();

    assert_eq!(schema_type(schema), "one_of");

    let children = one_of_children(schema);
    assert_eq!(children.len(), 2);
    // First child is `null`, second is the inner schema (`boolean`).
    assert_eq!(schema_type(&children[0]), "null");
    assert_eq!(schema_type(&children[1]), "boolean");
}

#[test]
fn test_optional_basic_dispatches_by_index() {
    let g = optional(super::super::booleans());
    let basic = g.as_basic().unwrap();

    let raw_none = Value::Array(vec![Value::Integer(0.into()), Value::Null]);
    let raw_some = Value::Array(vec![Value::Integer(1.into()), Value::Bool(true)]);

    assert_eq!(basic.parse_raw(raw_none), None);
    assert_eq!(basic.parse_raw(raw_some), Some(true));
}

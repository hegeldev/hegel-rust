use ciborium::Value;

/// Build a `ciborium::Value::Map`:
///
/// ```ignore
/// let schema = cbor_map!{
///     "type" => "integer",
///     "min_value" => 0,
///     "max_value" => 100
/// };
/// ```
macro_rules! cbor_map {
    ($($key:expr => $value:expr),* $(,)?) => {
        ciborium::Value::Map(vec![
            $((
                ciborium::Value::Text(String::from($key)),
                ciborium::Value::from($value),
            )),*
        ])
    };
}

/// Build a `ciborium::Value::Array`:
///
/// ```ignore
/// let elements = cbor_array![schema1, schema2];
/// ```
macro_rules! cbor_array {
    ($($value:expr),* $(,)?) => {
        ciborium::Value::Array(vec![$($value),*])
    };
}

pub(crate) use cbor_array;
pub(crate) use cbor_map;

pub fn map_get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    let Value::Map(entries) = value else {
        panic!("expected Value::Map, got {value:?}"); // nocov
    };
    for (k, v) in entries {
        let Value::Text(s) = k else {
            panic!("expected Value::Text, got {k:?}"); // nocov
        };
        if s == key {
            return Some(v);
        }
    }
    None
}

pub fn map_insert(value: &mut Value, key: &str, val: impl Into<Value>) {
    let Value::Map(entries) = value else {
        panic!("expected Value::Map, got {value:?}"); // nocov
    };
    let val = val.into();
    for (k, v) in entries.iter_mut() {
        let Value::Text(s) = k else {
            panic!("expected Value::Text, got {k:?}"); // nocov
        };
        if s == key {
            *v = val;
            return;
        }
    }
    entries.push((Value::Text(String::from(key)), val));
}

pub fn as_text(value: &Value) -> Option<&str> {
    match value {
        Value::Text(s) => Some(s),
        _ => None, // nocov
    }
}

pub fn as_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Integer(i) => u64::try_from(i128::from(*i)).ok(),
        _ => None, // nocov
    }
}

pub fn as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

pub fn cbor_serialize<T: serde::Serialize>(value: &T) -> Value {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).expect("CBOR serialization failed");
    ciborium::from_reader(&bytes[..]).expect("CBOR deserialization failed")
}

#[cfg(test)]
mod tests {
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
}

//! Ergonomic helpers for working with `ciborium::Value`.
//!
//! ciborium::Value::Map is `Vec<(Value, Value)>` (not a HashMap),
//! so these helpers provide convenient access patterns.

use ciborium::Value;

/// Build a `ciborium::Value::Map` from key-value pairs.
///
/// Keys are converted to `Value::Text`, values use `Value::from()`.
///
/// # Example
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
                ciborium::Value::Text($key.to_string()),
                ciborium::Value::from($value),
            )),*
        ])
    };
}

/// Build a `ciborium::Value::Array` from values.
///
/// # Example
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

/// Look up a text key in a `Value::Map`.
pub fn map_get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    if let Value::Map(entries) = value {
        for (k, v) in entries {
            if let Value::Text(s) = k {
                if s == key {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Insert or update a text key in a `Value::Map`.
pub fn map_insert(value: &mut Value, key: &str, val: Value) {
    if let Value::Map(entries) = value {
        // Check if key already exists
        for (k, v) in entries.iter_mut() {
            if let Value::Text(s) = k {
                if s == key {
                    *v = val;
                    return;
                }
            }
        }
        // Key not found, append
        entries.push((Value::Text(key.to_string()), val));
    }
}

/// Extract a string from a `Value::Text`.
pub fn as_text(value: &Value) -> Option<&str> {
    if let Value::Text(s) = value {
        Some(s)
    } else {
        None
    }
}

/// Extract a u64 from a `Value::Integer`.
pub fn as_u64(value: &Value) -> Option<u64> {
    if let Value::Integer(i) = value {
        let n: i128 = (*i).into();
        u64::try_from(n).ok()
    } else {
        None
    }
}

/// Extract a bool from a `Value::Bool`.
pub fn as_bool(value: &Value) -> Option<bool> {
    if let Value::Bool(b) = value {
        Some(*b)
    } else {
        None
    }
}

/// Format a ciborium::Value in a human-readable way (similar to JSON).
pub fn display_value(value: &Value) -> String {
    use std::fmt::Write;
    match value {
        Value::Integer(i) => {
            let n: i128 = (*i).into();
            n.to_string()
        }
        Value::Float(f) => {
            if f.is_nan() {
                "NaN".to_string()
            } else if f.is_infinite() {
                if f.is_sign_positive() {
                    "Infinity".to_string()
                } else {
                    "-Infinity".to_string()
                }
            } else if *f == f.trunc() && f.is_finite() {
                // Display whole floats as "1.0" not "1"
                format!("{:.1}", f)
            } else {
                f.to_string()
            }
        }
        Value::Text(s) => format!("{:?}", s),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(arr) => {
            let mut s = String::from("[");
            for (i, v) in arr.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&display_value(v));
            }
            s.push(']');
            s
        }
        Value::Map(entries) => {
            let mut s = String::from("{");
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                let _ = write!(s, "{}: {}", display_value(k), display_value(v));
            }
            s.push('}');
            s
        }
        Value::Bytes(b) => format!("<{} bytes>", b.len()),
        Value::Tag(tag, inner) => format!("tag({})({})", tag, display_value(inner)),
        _ => format!("{:?}", value),
    }
}

/// Convert a ciborium::Value to a serde-compatible value via CBOR round-trip.
/// This is used for serializing Rust values into ciborium::Value.
pub fn cbor_serialize<T: serde::Serialize>(value: &T) -> Value {
    // Serialize to CBOR bytes, then deserialize to ciborium::Value
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
            panic!("expected array");
        }
    }

    #[test]
    fn test_map_insert() {
        let mut m = cbor_map! { "a" => 1 };
        map_insert(&mut m, "b", Value::from(2));
        assert_eq!(as_u64(map_get(&m, "b").unwrap()), Some(2));

        // Update existing key
        map_insert(&mut m, "a", Value::from(10));
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

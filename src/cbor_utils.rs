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

// merge the keys of two maps. If both `target` and `source` contain the same key,
// prefer `source`.
pub fn map_extend(target: &mut Value, source: Value) {
    let Value::Map(source_entries) = source else {
        panic!("expected Value::Map, got {source:?}");
    };
    for (k, v) in source_entries {
        let Value::Text(ref key) = k else {
            panic!("expected Value::Text, got {k:?}");
        };
        map_insert(target, key, v);
    }
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
#[path = "../tests/embedded/cbor_utils_tests.rs"]
mod tests;

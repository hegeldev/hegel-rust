use ciborium::Value;

/// Build a `ciborium::Value::Map`, e.g.
/// `cbor_map!{ "type" => "integer", "min_value" => 0 }`. Test-only: the engine
/// interprets schemas, it never constructs them, so this helper exists purely
/// to keep the schema-interpreter tests readable.
#[cfg(test)]
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

/// Build a `ciborium::Value::Array`, e.g. `cbor_array![schema1, schema2]`.
/// Test-only, for the same reason as [`cbor_map!`].
#[cfg(test)]
macro_rules! cbor_array {
    ($($value:expr),* $(,)?) => {
        ciborium::Value::Array(vec![$($value),*])
    };
}

#[cfg(test)]
pub(crate) use cbor_array;
#[cfg(test)]
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

#[cfg(test)]
#[path = "../tests/embedded/cbor_utils_tests.rs"]
mod tests;

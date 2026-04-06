use ciborium::Value;
use std::io::{self, Read};

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

pub fn read_value(r: &mut impl Read) -> io::Result<Value> {
    let initial = read_u8(r)?;
    decode(r, initial)
}

fn read_u8(r: &mut impl Read) -> io::Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_argument(r: &mut impl Read, additional: u8) -> io::Result<u64> {
    match additional {
        0..=23 => Ok(additional as u64),
        24 => Ok(read_u8(r)? as u64),
        25 => {
            let mut b = [0u8; 2];
            r.read_exact(&mut b)?;
            Ok(u16::from_be_bytes(b) as u64)
        }
        26 => {
            let mut b = [0u8; 4];
            r.read_exact(&mut b)?;
            Ok(u32::from_be_bytes(b) as u64)
        }
        27 => {
            let mut b = [0u8; 8];
            r.read_exact(&mut b)?;
            Ok(u64::from_be_bytes(b))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid CBOR additional info: {additional}"),
        )),
    }
}

fn read_raw_bytes(r: &mut impl Read, additional: u8) -> io::Result<Vec<u8>> {
    if additional == 31 {
        let mut buf = Vec::new();
        loop {
            let peek = read_u8(r)?;
            if peek == 0xff {
                break;
            }
            let len = read_argument(r, peek & 0x1f)? as usize;
            let mut chunk = vec![0u8; len];
            r.read_exact(&mut chunk)?;
            buf.extend_from_slice(&chunk);
        }
        Ok(buf)
    } else {
        let len = read_argument(r, additional)? as usize;
        let mut buf = vec![0u8; len];
        r.read_exact(&mut buf)?;
        Ok(buf)
    }
}

fn read_indefinite<R: Read, T>(
    r: &mut R,
    mut item: impl FnMut(&mut R, u8) -> io::Result<T>,
) -> io::Result<Vec<T>> {
    let mut out = Vec::new();
    loop {
        let peek = read_u8(r)?;
        if peek == 0xff {
            return Ok(out);
        }
        out.push(item(r, peek)?);
    }
}

/// Decode a single CBOR value given its already-read initial byte.
fn decode(r: &mut impl Read, initial: u8) -> io::Result<Value> {
    let major = initial >> 5;
    let additional = initial & 0x1f;

    match major {
        0 => {
            let v = read_argument(r, additional)?;
            Ok(if v <= i64::MAX as u64 {
                Value::from(v as i64)
            } else {
                Value::Integer(ciborium::value::Integer::from(v))
            })
        }
        1 => {
            let v = read_argument(r, additional)?;
            Ok(if v <= i64::MAX as u64 {
                Value::from(-(v as i64) - 1)
            } else {
                let bytes = v.to_be_bytes();
                let start = bytes.iter().position(|&b| b != 0).unwrap_or(7);
                Value::Tag(3, Box::new(Value::Bytes(bytes[start..].to_vec())))
            })
        }
        2 => Ok(Value::Bytes(read_raw_bytes(r, additional)?)),
        3 => {
            let bytes = read_raw_bytes(r, additional)?;
            let s = String::from_utf8(bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            Ok(Value::Text(s))
        }
        4 => {
            if additional == 31 {
                Ok(Value::Array(read_indefinite(r, decode)?))
            } else {
                let n = read_argument(r, additional)? as usize;
                (0..n)
                    .map(|_| read_value(r))
                    .collect::<io::Result<Vec<_>>>()
                    .map(Value::Array)
            }
        }
        5 => {
            if additional == 31 {
                let items = read_indefinite(r, |r, peek| Ok((decode(r, peek)?, read_value(r)?)))?;
                Ok(Value::Map(items))
            } else {
                let n = read_argument(r, additional)? as usize;
                let mut entries = Vec::with_capacity(n);
                for _ in 0..n {
                    entries.push((read_value(r)?, read_value(r)?));
                }
                Ok(Value::Map(entries))
            }
        }
        6 => {
            let tag = read_argument(r, additional)?;
            let inner = read_value(r)?;
            Ok(Value::Tag(tag, Box::new(inner)))
        }
        7 => match additional {
            20 => Ok(Value::Bool(false)),
            21 => Ok(Value::Bool(true)),
            22 | 23 => Ok(Value::Null), // null and undefined map to same
            24 => Ok(match read_u8(r)? {
                20 => Value::Bool(false),
                21 => Value::Bool(true),
                _ => Value::Null,
            }),
            25 => {
                let mut b = [0u8; 2];
                r.read_exact(&mut b)?;
                Ok(Value::Float(f64::from(half::f16::from_be_bytes(b))))
            }
            26 => {
                let mut b = [0u8; 4];
                r.read_exact(&mut b)?;
                Ok(Value::Float(f32::from_be_bytes(b) as f64))
            }
            27 => {
                let mut b = [0u8; 8];
                r.read_exact(&mut b)?;
                Ok(Value::Float(f64::from_be_bytes(b)))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported simple value: additional={additional}"),
            )),
        },
        _ => unreachable!("CBOR major type > 7"),
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/cbor_utils_tests.rs"]
mod tests;

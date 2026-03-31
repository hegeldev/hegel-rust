use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::forward_to_deserialize_any;
use std::collections::HashMap;
use std::fmt;

/// A JSON-like value that can hold NaN, Infinity, and large integers.
#[derive(Clone, Debug)]
pub enum HegelValue {
    Null,
    Bool(bool),
    Number(f64),
    /// Large integer that doesn't fit in f64 precisely (abs >= 2^53)
    BigInt(String),
    String(String),
    Array(Vec<HegelValue>),
    Object(HashMap<String, HegelValue>),
}

impl From<ciborium::Value> for HegelValue {
    fn from(v: ciborium::Value) -> Self {
        match v {
            ciborium::Value::Null => HegelValue::Null, // nocov
            ciborium::Value::Bool(b) => HegelValue::Bool(b),
            ciborium::Value::Float(f) => {
                // NaN and Infinity are preserved natively by ciborium::Value
                HegelValue::Number(f)
            }
            ciborium::Value::Integer(i) => {
                let n: i128 = i.into();
                // Check if the integer can be represented precisely as f64
                let abs = n.unsigned_abs();
                if abs > (1u128 << 53) {
                    HegelValue::BigInt(n.to_string())
                } else {
                    HegelValue::Number(n as f64)
                }
            }
            ciborium::Value::Text(s) => HegelValue::String(s),
            // nocov start
            ciborium::Value::Bytes(b) => {
                // nocov end
                // Encode bytes as array of numbers
                HegelValue::Array(
                    // nocov start
                    b.into_iter()
                        .map(|byte| HegelValue::Number(byte as f64))
                        .collect(),
                    // nocov end
                )
            }
            ciborium::Value::Array(arr) => {
                HegelValue::Array(arr.into_iter().map(HegelValue::from).collect())
            }
            // nocov start
            ciborium::Value::Map(map) => HegelValue::Object(
                map.into_iter()
                    .map(|(k, v)| {
                        let key = match k {
                            ciborium::Value::Text(s) => s,
                            other => format!("{:?}", other),
                        };
                        (key, HegelValue::from(v))
                    })
                    .collect(),
                // nocov end
            ),
            ciborium::Value::Tag(2, inner) => {
                // CBOR tag 2: positive bignum, encoded as big-endian bytes
                let ciborium::Value::Bytes(bytes) = *inner else {
                    panic!("Expected Bytes inside bignum tag 2, got {:?}", inner) // nocov
                };
                let mut n = 0u128;
                for b in &bytes {
                    n = (n << 8) | (*b as u128);
                }
                HegelValue::BigInt(n.to_string())
            }
            ciborium::Value::Tag(3, inner) => {
                // CBOR tag 3: negative bignum, value is -1 - n
                let ciborium::Value::Bytes(bytes) = *inner else {
                    panic!("Expected Bytes inside bignum tag 3, got {:?}", inner) // nocov
                };
                let mut n = 0u128;
                for b in &bytes {
                    n = (n << 8) | (*b as u128);
                }
                let result = -1i128 - n as i128;
                HegelValue::BigInt(result.to_string())
            }
            // nocov start
            ciborium::Value::Tag(tag, _) => {
                panic!("Unexpected CBOR tag {tag} in protocol value")
                // nocov end
            }
            other => panic!("Unexpected CBOR value type: {:?}", other), // nocov
        }
    }
}

#[derive(Debug)]
pub struct HegelValueError(String);

impl fmt::Display for HegelValueError {
    // nocov start
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
        // nocov end
    }
}

impl std::error::Error for HegelValueError {}

impl de::Error for HegelValueError {
    // nocov start
    fn custom<T: fmt::Display>(msg: T) -> Self {
        HegelValueError(msg.to_string())
        // nocov end
    }
}

impl<'de> Deserializer<'de> for HegelValue {
    type Error = HegelValueError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self {
            HegelValue::Null => visitor.visit_unit(), // nocov
            HegelValue::Bool(b) => visitor.visit_bool(b),
            HegelValue::Number(n) => {
                // For whole numbers that fit in i64, use visit_i64 so integer
                // deserialization works. NaN/Inf have fract() != 0, so they
                // go to visit_f64.
                if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                    visitor.visit_i64(n as i64)
                } else {
                    visitor.visit_f64(n)
                }
            }
            HegelValue::BigInt(s) => {
                // Parse the string and use the smallest visitor type that fits.
                // This ensures compatibility with serde's primitive deserializers.
                if let Ok(n) = s.parse::<u64>() {
                    visitor.visit_u64(n)
                } else if let Ok(n) = s.parse::<i64>() {
                    visitor.visit_i64(n)
                } else if let Ok(n) = s.parse::<u128>() {
                    visitor.visit_u128(n)
                } else if let Ok(n) = s.parse::<i128>() {
                    visitor.visit_i128(n)
                } else {
                    Err(HegelValueError(format!("invalid big integer value: {}", s))) // nocov
                }
            }
            HegelValue::String(s) => visitor.visit_string(s),
            HegelValue::Array(arr) => visitor.visit_seq(HegelSeqAccess {
                iter: arr.into_iter(),
            }),
            HegelValue::Object(map) => visitor.visit_map(HegelMapAccess {
                iter: map.into_iter(),
                value: None,
            }),
        }
    }

    // nocov start
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self {
            HegelValue::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
            // nocov end
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

struct HegelSeqAccess {
    iter: std::vec::IntoIter<HegelValue>,
}

impl<'de> SeqAccess<'de> for HegelSeqAccess {
    type Error = HegelValueError;

    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Self::Error> {
        match self.iter.next() {
            Some(value) => seed.deserialize(value).map(Some),
            None => Ok(None),
        }
    }
}

struct HegelMapAccess {
    iter: std::collections::hash_map::IntoIter<String, HegelValue>,
    value: Option<HegelValue>,
}

impl<'de> MapAccess<'de> for HegelMapAccess {
    type Error = HegelValueError;

    fn next_key_seed<K: de::DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        match self.iter.next() {
            Some((key, value)) => {
                self.value = Some(value);
                seed.deserialize(StringDeserializer(key)).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V: de::DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, Self::Error> {
        let value = self
            .value
            .take()
            .expect("next_value called before next_key");
        seed.deserialize(value)
    }
}

struct StringDeserializer(String);

impl<'de> Deserializer<'de> for StringDeserializer {
    type Error = HegelValueError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_string(self.0)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

pub fn from_hegel_value<T: de::DeserializeOwned>(value: HegelValue) -> Result<T, HegelValueError> {
    T::deserialize(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_f64() {
        let v = HegelValue::Number(42.5);
        let result: f64 = from_hegel_value(v).unwrap();
        assert_eq!(result, 42.5);
    }

    #[test]
    fn test_deserialize_nan() {
        let v = HegelValue::Number(f64::NAN);
        let result: f64 = from_hegel_value(v).unwrap();
        assert!(result.is_nan());
    }

    #[test]
    fn test_deserialize_infinity() {
        let v = HegelValue::Number(f64::INFINITY);
        let result: f64 = from_hegel_value(v).unwrap();
        assert!(result.is_infinite() && result.is_sign_positive());
    }

    #[test]
    fn test_deserialize_neg_infinity() {
        let v = HegelValue::Number(f64::NEG_INFINITY);
        let result: f64 = from_hegel_value(v).unwrap();
        assert!(result.is_infinite() && result.is_sign_negative());
    }

    #[test]
    fn test_deserialize_vec_f64() {
        let v = HegelValue::Array(vec![
            HegelValue::Number(1.0),
            HegelValue::Number(f64::NAN),
            HegelValue::Number(f64::INFINITY),
        ]);
        let result: Vec<f64> = from_hegel_value(v).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], 1.0);
        assert!(result[1].is_nan());
        assert!(result[2].is_infinite());
    }

    #[test]
    fn test_from_ciborium_nan() {
        let cbor = ciborium::Value::Float(f64::NAN);
        let hegel = HegelValue::from(cbor);
        if let HegelValue::Number(n) = hegel {
            assert!(n.is_nan());
        } else {
            panic!("expected Number"); // nocov
        }
    }

    #[test]
    fn test_from_ciborium_infinity() {
        let cbor = ciborium::Value::Float(f64::INFINITY);
        let hegel = HegelValue::from(cbor);
        let result: f64 = from_hegel_value(hegel).unwrap();
        assert!(result.is_infinite() && result.is_sign_positive());
    }

    #[test]
    fn test_from_ciborium_neg_infinity() {
        let cbor = ciborium::Value::Float(f64::NEG_INFINITY);
        let hegel = HegelValue::from(cbor);
        let result: f64 = from_hegel_value(hegel).unwrap();
        assert!(result.is_infinite() && result.is_sign_negative());
    }

    #[test]
    fn test_from_ciborium_big_integer() {
        // Value larger than 2^53
        let cbor = ciborium::Value::Integer(9223372036854776833u64.into());
        let hegel = HegelValue::from(cbor);
        let result: u64 = from_hegel_value(hegel).unwrap();
        assert_eq!(result, 9223372036854776833u64);
    }

    #[test]
    fn test_from_ciborium_array_with_nan() {
        let cbor = ciborium::Value::Array(vec![
            ciborium::Value::Float(1.0),
            ciborium::Value::Float(f64::NAN),
            ciborium::Value::Float(f64::INFINITY),
            ciborium::Value::Float(f64::NEG_INFINITY),
        ]);
        let hegel = HegelValue::from(cbor);
        let result: Vec<f64> = from_hegel_value(hegel).unwrap();
        assert_eq!(result[0], 1.0);
        assert!(result[1].is_nan());
        assert!(result[2].is_infinite() && result[2].is_sign_positive());
        assert!(result[3].is_infinite() && result[3].is_sign_negative());
    }

    #[test]
    fn test_deserialize_struct() {
        #[derive(serde::Deserialize, Debug)]
        struct TestStruct {
            value: f64,
            name: String,
        }

        let v = HegelValue::Object(HashMap::from([
            ("value".to_string(), HegelValue::Number(f64::NAN)),
            ("name".to_string(), HegelValue::String("test".to_string())),
        ]));
        let result: TestStruct = from_hegel_value(v).unwrap();
        assert!(result.value.is_nan());
        assert_eq!(result.name, "test");
    }
}

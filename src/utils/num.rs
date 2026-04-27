use ciborium::Value;
use num_bigint::{BigInt, BigUint, Sign};
use num_traits::{One, ToPrimitive};

/// CBOR tag for rational numbers. See https://peteroupc.github.io/CBOR/rational.html.
pub const HEGEL_RATIONAL_TAG: u64 = 30;
/// CBOR tag for complex numbers. See https://www.iana.org/assignments/cbor-tags/template/43000.
pub const HEGEL_COMPLEX_TAG: u64 = 43000;

/// Convert a `BigInt` or `BigUint` to a `ciborium::Value` (integer or bignum tag).
pub fn int_to_cbor(n: impl Into<BigInt>) -> Value {
    let n: BigInt = n.into();
    // Try to fit in i128 first (avoids bignum tags for small values).
    if let Some(v) = n.to_i64() {
        return Value::from(v);
    }
    // Encode as CBOR bignum tag 2 (positive) or 3 (negative).
    let (sign, bytes) = n.to_bytes_be();
    match sign {
        Sign::NoSign | Sign::Plus => Value::Tag(2, Box::new(Value::Bytes(bytes))),
        Sign::Minus => {
            // Tag 3 encodes -1 - n, where n is the unsigned magnitude.
            let adjusted = n.magnitude() - BigUint::one();
            let adj_bytes = adjusted.to_bytes_be();
            Value::Tag(3, Box::new(Value::Bytes(adj_bytes)))
        }
    }
}

/// Parse a `ciborium::Value` (integer or bignum tag) into a `BigInt`.
pub fn cbor_to_bigint(v: Value) -> BigInt {
    match v {
        Value::Integer(i) => BigInt::from(i128::from(i)),
        Value::Tag(2, inner) => {
            let Value::Bytes(bytes) = *inner else {
                panic!("Expected Bytes inside bignum tag 2, got {:?}", inner); // nocov
            };
            BigInt::from_bytes_be(Sign::Plus, &bytes)
        }
        Value::Tag(3, inner) => {
            let Value::Bytes(bytes) = *inner else {
                panic!("Expected Bytes inside bignum tag 3, got {:?}", inner); // nocov
            };
            // Tag 3 value is -1 - n
            let n = BigUint::from_bytes_be(&bytes);
            BigInt::from(n) - BigInt::one()
        }
        other => panic!("Expected integer or bignum tag, got {:?}", other), // nocov
    }
}

/// Parse a `ciborium::Value` into a `BigUint`.
pub fn cbor_to_biguint(v: Value) -> BigUint {
    let n = cbor_to_bigint(v);
    n.try_into().unwrap()
}

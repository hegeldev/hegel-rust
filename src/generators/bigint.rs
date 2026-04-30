use crate::utils::bigint::{cbor_to_bigint, cbor_to_biguint, int_to_cbor};
use ciborium::Value;
use num_bigint::{BigInt, BigUint};
use num_traits::{One, Zero};

impl super::Integer for BigInt {
    fn default_min() -> Self {
        -(<BigInt as One>::one() << 128u32)
    }
    fn default_max() -> Self {
        (<BigInt as One>::one() << 128u32) - <BigInt as One>::one()
    }
    fn one() -> Self {
        <BigInt as One>::one()
    }
    fn to_cbor(&self) -> Value {
        int_to_cbor(self.clone())
    }
    fn from_cbor(v: Value) -> Self {
        cbor_to_bigint(v)
    }
}

impl super::Integer for BigUint {
    fn default_min() -> Self {
        BigUint::zero()
    }
    fn default_max() -> Self {
        (<BigUint as One>::one() << 128u32) - <BigUint as One>::one()
    }
    fn one() -> Self {
        <BigUint as One>::one()
    }
    fn to_cbor(&self) -> Value {
        int_to_cbor(BigInt::from(self.clone()))
    }
    fn from_cbor(v: Value) -> Self {
        cbor_to_biguint(v)
    }
}

use super::*;

fn i(v: i128) -> BigInt {
    BigInt::from(v)
}

fn u(v: u128) -> BigUint {
    BigUint::from(v)
}

#[test]
fn sign_is_three_valued() {
    assert_eq!(i(5).sign(), Sign::Plus);
    assert_eq!(i(-5).sign(), Sign::Minus);
    assert_eq!(i(0).sign(), Sign::NoSign);
}

#[test]
fn magnitude_drops_sign() {
    assert_eq!(i(-42).magnitude(), u(42));
    assert_eq!(i(42).magnitude(), u(42));
    assert_eq!(i(0).magnitude(), u(0));
}

#[test]
fn bytes_be_roundtrip() {
    let v = BigInt::from(u128::MAX) * BigInt::from(u128::MAX);
    let (sign, bytes) = v.to_bytes_be();
    assert_eq!(sign, Sign::Plus);
    assert_eq!(BigInt::from_bytes_be(Sign::Plus, &bytes), v);

    let (sign, bytes) = (-&v).to_bytes_be();
    assert_eq!(sign, Sign::Minus);
    assert_eq!(BigInt::from_bytes_be(Sign::Minus, &bytes), -&v);

    let (sign, bytes) = i(0).to_bytes_be();
    assert_eq!(sign, Sign::NoSign);
    assert!(bytes.is_empty());
}

#[test]
fn from_bytes_be_no_sign_is_zero() {
    assert_eq!(BigInt::from_bytes_be(Sign::NoSign, &[]), i(0));
}

#[test]
fn signed_bytes_le_roundtrip() {
    let cases = [
        i(0),
        i(1),
        i(-1),
        i(127),
        i(128),
        i(-128),
        i(-129),
        i(255),
        i(256),
        i(-256),
        i(i64::MAX as i128),
        i(i64::MIN as i128),
        BigInt::from(i128::MIN) * i(7),
        BigInt::from(u128::MAX) * BigInt::from(u128::MAX),
        -(BigInt::from(u128::MAX) * BigInt::from(u128::MAX)),
    ];
    for v in cases {
        let bytes = v.to_signed_bytes_le();
        assert_eq!(BigInt::from_signed_bytes_le(&bytes), v, "roundtrip {v}");
    }
    assert_eq!(i(0).to_signed_bytes_le(), vec![0]);
    assert_eq!(BigInt::from_signed_bytes_le(&[]), i(0));
}

#[test]
fn signed_bytes_le_is_minimal_at_negative_boundaries() {
    assert_eq!(i(-128).to_signed_bytes_le(), vec![0x80]);
    assert_eq!(i(-32768).to_signed_bytes_le(), vec![0x00, 0x80]);
    assert_eq!(
        BigInt::from(i64::MIN).to_signed_bytes_le(),
        i64::MIN.to_le_bytes().to_vec()
    );
    assert_eq!(
        BigInt::from(i128::MIN).to_signed_bytes_le(),
        i128::MIN.to_le_bytes().to_vec()
    );
    assert_eq!(i(-127).to_signed_bytes_le(), vec![0x81]);
    assert_eq!(i(-129).to_signed_bytes_le(), vec![0x7F, 0xFF]);
    assert_eq!(i(-255).to_signed_bytes_le(), vec![0x01, 0xFF]);
    assert_eq!(i(-256).to_signed_bytes_le(), vec![0x00, 0xFF]);
}

#[test]
fn bits_counts_magnitude_bits() {
    assert_eq!(i(0).bits(), 0);
    assert_eq!(u(0).bits(), 0);
    assert_eq!(i(1).bits(), 1);
    assert_eq!(i(-255).bits(), 8);
    assert_eq!(u(255).bits(), 8);
    assert_eq!(u(256).bits(), 9);
}

#[test]
fn pow_and_from_bytes_le() {
    assert_eq!(u(2).pow(10), u(1024));
    assert_eq!(BigUint::from_bytes_le(&[1, 1]), u(257));
}

#[test]
fn zero_trait() {
    assert!(<BigInt as Zero>::zero().is_zero());
    assert!(<BigUint as Zero>::zero().is_zero());
    assert!(!i(1).is_zero());
    assert!(!u(1).is_zero());
}

#[test]
fn signed_abs() {
    assert_eq!(i(-7).abs(), i(7));
    assert_eq!(i(7).abs(), i(7));
    assert_eq!(i(0).abs(), i(0));
}

#[test]
fn to_primitive_bigint() {
    let v = i(100);
    assert_eq!(v.to_i64(), Some(100));
    assert_eq!(v.to_i128(), Some(100));
    assert_eq!(v.to_u64(), Some(100));
    assert_eq!(v.to_u128(), Some(100));
    assert_eq!(v.to_f64(), Some(100.0));
}

#[test]
fn to_primitive_biguint() {
    let v = u(100);
    assert_eq!(v.to_i64(), Some(100));
    assert_eq!(v.to_i128(), Some(100));
    assert_eq!(v.to_u64(), Some(100));
    assert_eq!(v.to_u128(), Some(100));
    assert_eq!(v.to_f64(), Some(100.0));
}

#[test]
fn from_native_integers() {
    assert_eq!(BigInt::from(1i8), i(1));
    assert_eq!(BigInt::from(1i16), i(1));
    assert_eq!(BigInt::from(1i32), i(1));
    assert_eq!(BigInt::from(1i64), i(1));
    assert_eq!(BigInt::from(1i128), i(1));
    assert_eq!(BigInt::from(1u8), i(1));
    assert_eq!(BigInt::from(1u16), i(1));
    assert_eq!(BigInt::from(1u32), i(1));
    assert_eq!(BigInt::from(1u64), i(1));
    assert_eq!(BigInt::from(1u128), i(1));
    assert_eq!(BigUint::from(1u8), u(1));
    assert_eq!(BigUint::from(1u16), u(1));
    assert_eq!(BigUint::from(1u32), u(1));
    assert_eq!(BigUint::from(1u64), u(1));
    assert_eq!(BigUint::from(1u128), u(1));
    assert_eq!(BigInt::from(u(42)), i(42));
}

#[test]
fn try_from_bigint_into_native() {
    assert_eq!(i8::try_from(&i(5)), Ok(5));
    assert_eq!(i16::try_from(&i(5)), Ok(5));
    assert_eq!(i32::try_from(&i(5)), Ok(5));
    assert_eq!(i64::try_from(&i(5)), Ok(5));
    assert_eq!(i128::try_from(&i(5)), Ok(5));
    assert_eq!(u8::try_from(&i(5)), Ok(5));
    assert_eq!(u16::try_from(&i(5)), Ok(5));
    assert_eq!(u32::try_from(&i(5)), Ok(5));
    assert_eq!(u64::try_from(&i(5)), Ok(5));
    assert_eq!(u128::try_from(&i(5)), Ok(5));
    assert_eq!(u8::try_from(&i(-1)), Err(()));
    assert_eq!(i8::try_from(&i(1000)), Err(()));
}

#[test]
fn try_from_biguint_into_native() {
    assert_eq!(u8::try_from(u(5)), Ok(5));
    assert_eq!(u16::try_from(u(5)), Ok(5));
    assert_eq!(u32::try_from(u(5)), Ok(5));
    assert_eq!(u64::try_from(u(5)), Ok(5));
    assert_eq!(u128::try_from(u(5)), Ok(5));
    assert_eq!(u8::try_from(&u(5)), Ok(5));
    assert_eq!(u16::try_from(&u(5)), Ok(5));
    assert_eq!(u32::try_from(&u(5)), Ok(5));
    assert_eq!(u64::try_from(&u(5)), Ok(5));
    assert_eq!(u128::try_from(&u(5)), Ok(5));
    assert_eq!(u8::try_from(u(999)), Err(()));
    assert_eq!(u8::try_from(&u(999)), Err(()));
}

#[test]
fn bigint_arithmetic_all_combos() {
    let a = i(10);
    let b = i(3);
    assert_eq!(a.clone() + b.clone(), i(13));
    assert_eq!(a.clone() + &b, i(13));
    assert_eq!(&a + b.clone(), i(13));
    assert_eq!(&a + &b, i(13));
    assert_eq!(a.clone() - b.clone(), i(7));
    assert_eq!(a.clone() - &b, i(7));
    assert_eq!(&a - b.clone(), i(7));
    assert_eq!(&a - &b, i(7));
    assert_eq!(a.clone() * b.clone(), i(30));
    assert_eq!(a.clone() * &b, i(30));
    assert_eq!(&a * b.clone(), i(30));
    assert_eq!(&a * &b, i(30));
    assert_eq!(a.clone() + 1, i(11));
    assert_eq!(&a + 1, i(11));
    assert_eq!(a.clone() - 1, i(9));
    assert_eq!(&a - 1, i(9));
    assert_eq!(a.clone() / 2, i(5));
    assert_eq!(&a / 2, i(5));
    assert_eq!(-a.clone(), i(-10));
    assert_eq!(-&a, i(-10));
    assert_eq!(i(16) >> 2usize, i(4));
    assert_eq!(&i(16) >> 2usize, i(4));
}

#[test]
fn biguint_arithmetic_all_combos() {
    let a = u(10);
    let b = u(3);
    assert_eq!(a.clone() + b.clone(), u(13));
    assert_eq!(a.clone() + &b, u(13));
    assert_eq!(&a + b.clone(), u(13));
    assert_eq!(&a + &b, u(13));
    assert_eq!(a.clone() - b.clone(), u(7));
    assert_eq!(a.clone() - &b, u(7));
    assert_eq!(&a - b.clone(), u(7));
    assert_eq!(&a - &b, u(7));
    assert_eq!(a.clone() * b.clone(), u(30));
    assert_eq!(a.clone() * &b, u(30));
    assert_eq!(&a * b.clone(), u(30));
    assert_eq!(&a * &b, u(30));
    assert_eq!(a.clone() / b.clone(), u(3));
    assert_eq!(a.clone() / &b, u(3));
    assert_eq!(&a / b.clone(), u(3));
    assert_eq!(&a / &b, u(3));
    assert_eq!(a.clone() % b.clone(), u(1));
    assert_eq!(a.clone() % &b, u(1));
    assert_eq!(&a % b.clone(), u(1));
    assert_eq!(&a % &b, u(1));
    assert_eq!(u(8) >> 1u32, u(4));
    assert_eq!(&u(8) >> 1u32, u(4));
    assert_eq!(u(1) << 3usize, u(8));
    assert_eq!(&u(1) << 3usize, u(8));
}

#[test]
fn assign_ops() {
    let mut n = i(5);
    n += 1;
    assert_eq!(n, i(6));

    let mut x = u(5);
    x += u(2);
    assert_eq!(x, u(7));
    x -= u(3);
    assert_eq!(x, u(4));
    x *= u(2);
    assert_eq!(x, u(8));
    x /= &u(4);
    assert_eq!(x, u(2));
}

#[test]
fn display_matches_value() {
    assert_eq!(format!("{}", i(-123)), "-123");
    assert_eq!(format!("{}", u(123)), "123");
}

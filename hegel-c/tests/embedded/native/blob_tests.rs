use super::*;
use crate::native::base64::{base64_decode, base64_encode};
use crate::native::bignum::BigInt;
use crate::native::core::ChoiceValue;

fn sample_choices() -> Vec<ChoiceValue> {
    vec![
        ChoiceValue::Integer(BigInt::from(42)),
        ChoiceValue::Integer(BigInt::from(-7)),
        ChoiceValue::Boolean(true),
        ChoiceValue::Boolean(false),
        ChoiceValue::Float(3.5),
        ChoiceValue::Float(-0.0),
        ChoiceValue::Bytes(vec![0, 1, 2, 255]),
        ChoiceValue::String(vec![0x48, 0x69, 0x1F600]),
    ]
}

#[test]
fn round_trips_a_mixed_choice_sequence() {
    let choices = sample_choices();
    let blob = encode_failure(&choices);
    let decoded = decode_failure(&blob).unwrap();
    assert_eq!(decoded, choices);
}

#[test]
fn round_trips_an_empty_choice_sequence() {
    let blob = encode_failure(&[]);
    assert_eq!(decode_failure(&blob).unwrap(), Vec::<ChoiceValue>::new());
}

#[test]
fn small_sequence_uses_the_raw_prefix() {
    let blob = encode_failure(&[ChoiceValue::Boolean(true)]);
    let bytes = base64_decode(&blob).unwrap();
    assert_eq!(bytes[0], PREFIX_RAW);
    assert_eq!(
        decode_failure(&blob).unwrap(),
        vec![ChoiceValue::Boolean(true)]
    );
}

#[test]
fn long_repetitive_sequence_uses_the_zlib_prefix() {
    let choices: Vec<ChoiceValue> = (0..500)
        .map(|_| ChoiceValue::Integer(BigInt::from(1)))
        .collect();
    let blob = encode_failure(&choices);
    let bytes = base64_decode(&blob).unwrap();
    assert_eq!(bytes[0], PREFIX_ZLIB);
    assert_eq!(decode_failure(&blob).unwrap(), choices);
}

#[test]
fn decode_rejects_invalid_base64() {
    assert!(decode_failure("abc").is_none());
    assert!(decode_failure("ab*=").is_none());
}

#[test]
fn decode_rejects_empty_payload() {
    assert!(decode_failure("").is_none());
}

#[test]
fn decode_rejects_unknown_prefix_byte() {
    let mut payload = vec![9u8];
    payload.extend_from_slice(&serialize_choices(&sample_choices()));
    let blob = base64_encode(&payload);
    assert!(decode_failure(&blob).is_none());
}

#[test]
fn decode_rejects_corrupt_zlib_stream() {
    let blob = base64_encode(&[PREFIX_ZLIB, 0xFF, 0xFF, 0xFF, 0xFF]);
    assert!(decode_failure(&blob).is_none());
}

#[test]
fn decode_rejects_raw_payload_that_is_not_valid_choices() {
    let blob = base64_encode(&[PREFIX_RAW, 0xAB]);
    assert!(decode_failure(&blob).is_none());
}

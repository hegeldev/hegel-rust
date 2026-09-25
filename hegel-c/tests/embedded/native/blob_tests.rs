use super::*;
use crate::native::base64::{base64_decode, base64_encode};
use crate::native::bignum::BigInt;
use crate::native::core::{ChoiceValue, CloneRecord, MAX_CLONE_DEPTH};
use alloc::vec;

fn nested_clones(depth: usize) -> Vec<ChoiceValue> {
    let mut choices = vec![ChoiceValue::Boolean(true)];
    for _ in 0..depth {
        choices = vec![ChoiceValue::Clone(std::sync::Arc::new(
            CloneRecord::from_values(choices),
        ))];
    }
    choices
}

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
    let blob = encode_failure(&choices).unwrap();
    let decoded = decode_failure(&blob).unwrap();
    assert_eq!(decoded, choices);
}

#[test]
fn round_trips_an_empty_choice_sequence() {
    let blob = encode_failure(&[]).unwrap();
    assert_eq!(decode_failure(&blob).unwrap(), Vec::<ChoiceValue>::new());
}

#[test]
fn small_sequence_uses_the_raw_prefix() {
    let blob = encode_failure(&[ChoiceValue::Boolean(true)]).unwrap();
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
    let blob = encode_failure(&choices).unwrap();
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
    payload.extend_from_slice(&serialize_choices(&sample_choices()).unwrap());
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

#[test]
fn decode_rejects_zlib_bomb() {
    let raw = serialize_choices(&[ChoiceValue::Bytes(vec![0u8; MAX_DECOMPRESSED_LEN])]).unwrap();
    assert!(raw.len() > MAX_DECOMPRESSED_LEN);
    let mut payload = vec![PREFIX_ZLIB];
    payload.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(
        &raw, ZLIB_LEVEL,
    ));
    let blob = base64_encode(&payload);
    assert!(decode_failure(&blob).is_none());
}

#[test]
fn over_bound_payload_takes_the_raw_prefix_and_round_trips() {
    let choices = vec![ChoiceValue::Bytes(vec![0u8; MAX_DECOMPRESSED_LEN])];
    let blob = encode_failure(&choices).unwrap();
    let bytes = base64_decode(&blob).unwrap();
    assert_eq!(bytes[0], PREFIX_RAW);
    assert_eq!(decode_failure(&blob).unwrap(), choices);
}

#[test]
fn zlib_decode_limit_admits_large_legitimate_blobs() {
    let choices = vec![ChoiceValue::Bytes(vec![0u8; MAX_DECOMPRESSED_LEN - 9])];
    assert_eq!(
        serialize_choices(&choices).unwrap().len(),
        MAX_DECOMPRESSED_LEN
    );
    let blob = encode_failure(&choices).unwrap();
    let bytes = base64_decode(&blob).unwrap();
    assert_eq!(bytes[0], PREFIX_ZLIB);
    assert_eq!(decode_failure(&blob).unwrap(), choices);
}

#[test]
fn blob_roundtrips_clone_values() {
    let choices = vec![
        ChoiceValue::Boolean(true),
        ChoiceValue::Clone(std::sync::Arc::new(
            crate::native::core::CloneRecord::from_values(vec![
                ChoiceValue::Integer(crate::native::bignum::BigInt::from(7)),
                ChoiceValue::Clone(std::sync::Arc::new(
                    crate::native::core::CloneRecord::from_values(Vec::new()),
                )),
            ]),
        )),
    ];
    let blob = encode_failure(&choices).unwrap();
    assert_eq!(decode_failure(&blob), Some(choices));
}

#[test]
fn blob_round_trips_clones_nested_to_max_depth() {
    let choices = nested_clones(MAX_CLONE_DEPTH);
    let blob = encode_failure(&choices).unwrap();
    assert_eq!(decode_failure(&blob), Some(choices));
}

#[test]
fn encode_refuses_clones_nested_beyond_max_depth() {
    assert!(encode_failure(&nested_clones(MAX_CLONE_DEPTH + 1)).is_none());
}

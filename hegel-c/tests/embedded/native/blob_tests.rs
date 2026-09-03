use super::*;
use crate::native::base64::{base64_decode, base64_encode};
use crate::native::bignum::BigInt;
use crate::native::core::ChoiceValue;
use alloc::vec;

/// [`decode_blob`] narrowed to the deterministic (prefix 0/1) form.
fn decode_failure(blob: &str) -> Option<Vec<ChoiceValue>> {
    match decode_blob(blob)? {
        DecodedBlob::Choices(choices) => Some(choices),
        DecodedBlob::Nd(_) => None,
    }
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
    let blob = encode_failure(&choices);
    assert_eq!(decode_failure(&blob), Some(choices));
}

fn sample_nd_state() -> NdReproState {
    NdReproState {
        timelines: vec![
            sample_choices(),
            vec![ChoiceValue::Boolean(true)],
            vec![ChoiceValue::Boolean(false), ChoiceValue::Float(1.5)],
        ],
        entropy: 0xDEAD_BEEF_CAFE_F00D,
        extension: 12,
    }
}

#[test]
fn nd_blob_round_trips_pool_entropy_and_extension() {
    let state = sample_nd_state();
    let blob = encode_nd_failure(&state);
    let Some(DecodedBlob::Nd(decoded)) = decode_blob(&blob) else {
        panic!("expected nd state");
    };
    assert_eq!(decoded.timelines, state.timelines);
    assert_eq!(decoded.incumbent(), state.timelines[0].as_slice());
    assert_eq!(decoded.entropy, state.entropy);
    assert_eq!(decoded.extension, state.extension);
}

#[test]
fn incompressible_nd_state_uses_the_raw_prefix() {
    let noise: Vec<u8> = (0..200u32)
        .map(|i| (i.wrapping_mul(197).wrapping_add(i * i * 31) % 251) as u8)
        .collect();
    let state = NdReproState {
        timelines: vec![vec![ChoiceValue::Bytes(noise)]],
        entropy: 1,
        extension: 4,
    };
    let blob = encode_nd_failure(&state);
    let bytes = base64_decode(&blob).unwrap();
    assert_eq!(bytes[0], PREFIX_ND_RAW);
    assert!(matches!(decode_blob(&blob), Some(DecodedBlob::Nd(_))));
}

#[test]
fn long_nd_state_uses_the_zlib_prefix_and_round_trips() {
    let state = NdReproState {
        timelines: vec![
            (0..500)
                .map(|_| ChoiceValue::Integer(BigInt::from(1)))
                .collect(),
        ],
        entropy: 7,
        extension: 4,
    };
    let blob = encode_nd_failure(&state);
    let bytes = base64_decode(&blob).unwrap();
    assert_eq!(bytes[0], PREFIX_ND_ZLIB);
    let Some(DecodedBlob::Nd(decoded)) = decode_blob(&blob) else {
        panic!("expected nd state");
    };
    assert_eq!(decoded.timelines, state.timelines);
}

#[test]
fn nd_state_bytes_are_rejected_by_the_choice_deserializer() {
    let bytes = encode_nd_state(&sample_nd_state());
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn nd_state_decode_rejects_malformed_bytes() {
    let good = encode_nd_state(&sample_nd_state());
    assert!(decode_nd_state(&good).is_some());
    assert!(decode_nd_state(&[]).is_none());
    assert!(decode_nd_state(&good[1..]).is_none(), "wrong magic");
    assert!(
        decode_nd_state(&good[..good.len() - 1]).is_none(),
        "truncated timeline"
    );
    let mut wrong_version = good.clone();
    wrong_version[4] = 9;
    assert!(decode_nd_state(&wrong_version).is_none());
    let mut zero_count = good.clone();
    zero_count[17..21].copy_from_slice(&0u32.to_le_bytes());
    assert!(decode_nd_state(&zero_count).is_none());
    let mut absurd_count = good.clone();
    absurd_count[17..21].copy_from_slice(&1_000u32.to_le_bytes());
    assert!(decode_nd_state(&absurd_count).is_none());
    let mut corrupt_timeline = good.clone();
    corrupt_timeline[29] = 9;
    assert!(
        decode_nd_state(&corrupt_timeline).is_none(),
        "an unknown choice tag inside a timeline is rejected"
    );
}

#[test]
fn nd_blob_decode_rejects_corrupt_payloads() {
    let blob = base64_encode(&[PREFIX_ND_RAW, 0xAB]);
    assert!(decode_blob(&blob).is_none());
    let blob = base64_encode(&[PREFIX_ND_ZLIB, 0xFF, 0xFF, 0xFF, 0xFF]);
    assert!(decode_blob(&blob).is_none());
}

#[test]
fn decode_blob_rejects_zlib_bomb_v1() {
    let choices = vec![ChoiceValue::Bytes(vec![0u8; MAX_DECOMPRESSED_LEN])];
    assert!(serialize_choices(&choices).len() > MAX_DECOMPRESSED_LEN);
    let blob = encode_failure(&choices);
    let bytes = base64_decode(&blob).unwrap();
    assert_eq!(bytes[0], PREFIX_ZLIB);
    assert!(decode_blob(&blob).is_none());
}

#[test]
fn decode_blob_rejects_zlib_bomb_nd() {
    let state = NdReproState {
        timelines: vec![vec![ChoiceValue::Bytes(vec![0u8; MAX_DECOMPRESSED_LEN])]],
        entropy: 1,
        extension: 0,
    };
    assert!(encode_nd_state(&state).len() > MAX_DECOMPRESSED_LEN);
    let blob = encode_nd_failure(&state);
    let bytes = base64_decode(&blob).unwrap();
    assert_eq!(bytes[0], PREFIX_ND_ZLIB);
    assert!(decode_blob(&blob).is_none());
}

#[test]
fn zlib_decode_limit_admits_large_legitimate_blobs() {
    let choices = vec![ChoiceValue::Bytes(vec![0u8; MAX_DECOMPRESSED_LEN - 9])];
    assert_eq!(serialize_choices(&choices).len(), MAX_DECOMPRESSED_LEN);
    let blob = encode_failure(&choices);
    let bytes = base64_decode(&blob).unwrap();
    assert_eq!(bytes[0], PREFIX_ZLIB);
    assert_eq!(decode_failure(&blob).unwrap(), choices);
}

#[test]
fn v1_blobs_still_decode_as_plain_choice_sequences() {
    let blob = encode_failure(&sample_choices());
    assert!(matches!(
        decode_blob(&blob),
        Some(DecodedBlob::Choices(choices)) if choices == sample_choices()
    ));
}

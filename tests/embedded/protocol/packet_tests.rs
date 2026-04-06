use super::*;
use std::os::unix::net::UnixStream;

use ciborium::Value;

#[test]
fn test_packet_roundtrip() {
    let (mut client, mut server) = UnixStream::pair().unwrap();

    let packet = Packet {
        stream: 1,
        message_id: 42,
        is_reply: false,
        payload: b"hello world".to_vec(),
    };
    write_packet(&mut client, &packet).unwrap();

    let received = read_packet(&mut server).unwrap();
    assert_eq!(received.stream, 1);
    assert_eq!(received.message_id, 42);
    assert!(!received.is_reply);
    assert_eq!(received.payload, b"hello world");
}

#[test]
fn test_reply_packet() {
    let (mut client, mut server) = UnixStream::pair().unwrap();

    let packet = Packet {
        stream: 2,
        message_id: 100,
        is_reply: true,
        payload: b"response".to_vec(),
    };
    write_packet(&mut client, &packet).unwrap();

    let received = read_packet(&mut server).unwrap();
    assert_eq!(received.stream, 2);
    assert_eq!(received.message_id, 100);
    assert!(received.is_reply);
    assert_eq!(received.payload, b"response");
}

#[test]
fn test_cbor_value_roundtrip() {
    use crate::utils::cbor_utils::cbor_map;
    // Test that ciborium::Value roundtrips through CBOR
    let value = cbor_map! {
        "type" => "integer",
        "min_value" => 0,
        "max_value" => 100
    };

    // Serialize to CBOR bytes
    let mut cbor_bytes = Vec::new();
    ciborium::into_writer(&value, &mut cbor_bytes).unwrap();

    // Deserialize back
    let back: Value = ciborium::from_reader(&cbor_bytes[..]).unwrap();

    assert_eq!(value, back);
}

#[test]
fn test_cbor_nan_preserved() {
    let value = Value::Float(f64::NAN);
    let mut bytes = Vec::new();
    ciborium::into_writer(&value, &mut bytes).unwrap();
    let back: Value = ciborium::from_reader(&bytes[..]).unwrap();
    if let Value::Float(f) = back {
        assert!(f.is_nan());
    } else {
        panic!("expected Float"); // nocov
    }
}

#[test]
fn test_cbor_infinity_preserved() {
    let value = Value::Float(f64::INFINITY);
    let mut bytes = Vec::new();
    ciborium::into_writer(&value, &mut bytes).unwrap();
    let back: Value = ciborium::from_reader(&bytes[..]).unwrap();
    assert_eq!(back, Value::Float(f64::INFINITY));
}

#[test]
fn test_cbor_neg_infinity_preserved() {
    let value = Value::Float(f64::NEG_INFINITY);
    let mut bytes = Vec::new();
    ciborium::into_writer(&value, &mut bytes).unwrap();
    let back: Value = ciborium::from_reader(&bytes[..]).unwrap();
    assert_eq!(back, Value::Float(f64::NEG_INFINITY));
}

use super::*;
use crate::native::bignum::BigInt;
use crate::native::core::ChoiceValue;
use tempfile::TempDir;

fn fresh_db() -> (DirectoryTestCaseDatabase, TempDir) {
    let dir = TempDir::new().unwrap();
    let db = DirectoryTestCaseDatabase::new(dir.path().to_str().unwrap());
    (db, dir)
}

#[test]
fn save_then_fetch_round_trips_a_value() {
    let (db, _dir) = fresh_db();
    db.save(b"key", b"value");
    let fetched = db.fetch(b"key");
    assert_eq!(fetched, vec![b"value".to_vec()]);
}

#[test]
fn save_is_idempotent() {
    let (db, _dir) = fresh_db();
    db.save(b"key", b"value");
    db.save(b"key", b"value");
    assert_eq!(db.fetch(b"key"), vec![b"value".to_vec()]);
}

#[test]
fn delete_removes_a_saved_value() {
    let (db, _dir) = fresh_db();
    db.save(b"key", b"value");
    db.delete(b"key", b"value");
    assert!(db.fetch(b"key").is_empty());
}

#[test]
fn delete_of_absent_value_is_silent() {
    let (db, _dir) = fresh_db();
    db.delete(b"key", b"nonexistent");
    assert!(db.fetch(b"key").is_empty());
}

#[test]
fn fetch_of_absent_key_returns_empty() {
    let (db, _dir) = fresh_db();
    assert!(db.fetch(b"never-saved").is_empty());
}

#[test]
fn move_value_within_same_key_is_a_resave() {
    let (db, _dir) = fresh_db();
    db.save(b"key", b"v");
    db.move_value(b"key", b"key", b"v");
    assert_eq!(db.fetch(b"key"), vec![b"v".to_vec()]);
}

#[test]
fn move_value_relocates_entry() {
    let (db, _dir) = fresh_db();
    db.save(b"src", b"v");
    db.move_value(b"src", b"dst", b"v");
    assert!(db.fetch(b"src").is_empty());
    assert_eq!(db.fetch(b"dst"), vec![b"v".to_vec()]);
}

#[test]
fn move_value_creates_dst_when_absent() {
    let (db, _dir) = fresh_db();
    db.save(b"src", b"v");
    db.move_value(b"src", b"brand-new-dst", b"v");
    assert_eq!(db.fetch(b"brand-new-dst"), vec![b"v".to_vec()]);
}

#[test]
fn move_value_falls_back_to_delete_save_when_rename_fails() {
    let (db, _dir) = fresh_db();
    db.save(b"dst", b"existing");
    db.move_value(b"src", b"dst", b"v");
    let mut got = db.fetch(b"dst");
    got.sort();
    let mut want = vec![b"existing".to_vec(), b"v".to_vec()];
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn round_trip_integer_choices() {
    let choices = vec![
        ChoiceValue::Integer(BigInt::from(0)),
        ChoiceValue::Integer(BigInt::from(-1)),
        ChoiceValue::Integer(BigInt::from(i128::MAX)),
        ChoiceValue::Integer(BigInt::from(i128::MIN)),
    ];
    let bytes = serialize_choices(&choices);
    assert_eq!(deserialize_choices(&bytes), Some(choices));
}

#[test]
fn round_trip_mixed_choices() {
    let choices = vec![
        ChoiceValue::Boolean(false),
        ChoiceValue::Integer(BigInt::from(42)),
        ChoiceValue::Boolean(true),
    ];
    let bytes = serialize_choices(&choices);
    assert_eq!(deserialize_choices(&bytes), Some(choices));
}

#[test]
fn deserialize_legacy_per_width_integers_as_bigint() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&10u32.to_le_bytes());

    bytes.push(0);
    bytes.push(0);
    bytes.extend_from_slice(&(-1i8).to_le_bytes());

    bytes.push(0);
    bytes.push(1);
    bytes.extend_from_slice(&(-256i16).to_le_bytes());

    bytes.push(0);
    bytes.push(2);
    bytes.extend_from_slice(&70000i32.to_le_bytes());

    bytes.push(0);
    bytes.push(3);
    bytes.extend_from_slice(&i64::MIN.to_le_bytes());

    bytes.push(0);
    bytes.push(4);
    bytes.extend_from_slice(&42i128.to_le_bytes());

    bytes.push(0);
    bytes.push(5);
    bytes.extend_from_slice(&255u8.to_le_bytes());

    bytes.push(0);
    bytes.push(6);
    bytes.extend_from_slice(&1000u16.to_le_bytes());

    bytes.push(0);
    bytes.push(7);
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());

    bytes.push(0);
    bytes.push(8);
    bytes.extend_from_slice(&u64::MAX.to_le_bytes());

    bytes.push(0);
    bytes.push(9);
    bytes.extend_from_slice(&u128::MAX.to_le_bytes());

    let result = deserialize_choices(&bytes).unwrap();
    assert_eq!(result.len(), 10);
    assert_eq!(result[0], ChoiceValue::Integer(BigInt::from(-1)));
    assert_eq!(result[1], ChoiceValue::Integer(BigInt::from(-256)));
    assert_eq!(result[2], ChoiceValue::Integer(BigInt::from(70000)));
    assert_eq!(result[3], ChoiceValue::Integer(BigInt::from(i64::MIN)));
    assert_eq!(result[4], ChoiceValue::Integer(BigInt::from(42)));
    assert_eq!(result[5], ChoiceValue::Integer(BigInt::from(255)));
    assert_eq!(result[6], ChoiceValue::Integer(BigInt::from(1000)));
    assert_eq!(result[7], ChoiceValue::Integer(BigInt::from(u32::MAX)));
    assert_eq!(result[8], ChoiceValue::Integer(BigInt::from(u64::MAX)));
    assert_eq!(result[9], ChoiceValue::Integer(BigInt::from(u128::MAX)));
}

#[test]
fn deserialize_returns_none_on_too_short_header() {
    assert!(deserialize_choices(&[0, 0, 0]).is_none());
}

#[test]
fn deserialize_returns_none_on_missing_type_tag() {
    assert!(deserialize_choices(&1u32.to_le_bytes()).is_none());
}

#[test]
fn deserialize_returns_none_on_truncated_integer_body() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(0);
    bytes.push(4);
    bytes.extend_from_slice(&[0u8; 8]);
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn deserialize_returns_none_on_missing_integer_width_subtag() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(0);
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn deserialize_returns_none_on_unknown_integer_width_subtag() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(0);
    bytes.push(99);
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn deserialize_returns_none_on_truncated_bigint_body() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(0);
    bytes.push(10);
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.push(0xFF);
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn serialize_roundtrips_various_integer_values() {
    let values = vec![
        ChoiceValue::Integer(BigInt::from(-12i8)),
        ChoiceValue::Integer(BigInt::from(-1234i16)),
        ChoiceValue::Integer(BigInt::from(-123456i32)),
        ChoiceValue::Integer(BigInt::from(-1234567890i64)),
        ChoiceValue::Integer(BigInt::from(i128::MIN)),
        ChoiceValue::Integer(BigInt::from(200u8)),
        ChoiceValue::Integer(BigInt::from(60000u16)),
        ChoiceValue::Integer(BigInt::from(4_000_000_000u32)),
        ChoiceValue::Integer(BigInt::from(u64::MAX)),
        ChoiceValue::Integer(BigInt::from(u128::MAX)),
        ChoiceValue::Integer(BigInt::from(i128::MIN) * BigInt::from(7)),
        ChoiceValue::Integer(BigInt::from(0)),
    ];
    let bytes = serialize_choices(&values);
    assert_eq!(deserialize_choices(&bytes), Some(values));
}

#[test]
fn deserialize_returns_none_on_truncated_boolean_body() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(1);
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn deserialize_returns_none_on_unknown_type_tag() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(42);
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn round_trip_float_choices_preserves_bit_pattern() {
    let choices = vec![
        ChoiceValue::Float(0.0),
        ChoiceValue::Float(-0.0),
        ChoiceValue::Float(1.5),
        ChoiceValue::Float(-1.5),
        ChoiceValue::Float(f64::INFINITY),
        ChoiceValue::Float(f64::NEG_INFINITY),
        ChoiceValue::Float(f64::NAN),
        ChoiceValue::Float(f64::MAX),
        ChoiceValue::Float(f64::MIN_POSITIVE),
    ];
    let bytes = serialize_choices(&choices);
    let round_tripped = deserialize_choices(&bytes).unwrap();
    assert_eq!(round_tripped.len(), choices.len());
    for (got, want) in round_tripped.iter().zip(choices.iter()) {
        match (got, want) {
            (ChoiceValue::Float(g), ChoiceValue::Float(w)) => {
                assert_eq!(g.to_bits(), w.to_bits(), "bit pattern mismatch");
            }
            _ => panic!("non-float variant produced"),
        }
    }
}

#[test]
fn deserialize_returns_none_on_truncated_float_body() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(2);
    bytes.extend_from_slice(&[0u8; 4]);
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn round_trip_bytes_choices() {
    let choices = vec![
        ChoiceValue::Bytes(Vec::new()),
        ChoiceValue::Bytes(vec![0u8]),
        ChoiceValue::Bytes(vec![0xff, 0x00, 0x80, 0x7f]),
        ChoiceValue::Bytes(vec![1; 1024]),
    ];
    let bytes = serialize_choices(&choices);
    assert_eq!(deserialize_choices(&bytes), Some(choices));
}

#[test]
fn deserialize_returns_none_on_truncated_bytes_length() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(3);
    bytes.extend_from_slice(&[0u8; 2]);
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn deserialize_returns_none_on_truncated_bytes_body() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(3);
    bytes.extend_from_slice(&5u32.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 2]);
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn round_trip_string_choices() {
    let choices = vec![
        ChoiceValue::String(Vec::new()),
        ChoiceValue::String(vec![b'0' as u32]),
        ChoiceValue::String(vec![b'a' as u32, b'b' as u32, b'c' as u32]),
        ChoiceValue::String(vec![0x2603, 0x1F600, 0]),
    ];
    let bytes = serialize_choices(&choices);
    assert_eq!(deserialize_choices(&bytes), Some(choices));
}

#[test]
fn deserialize_returns_none_on_truncated_string_length() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(4);
    bytes.extend_from_slice(&[0u8; 2]);
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn deserialize_returns_none_on_truncated_string_body() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(4);
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 4]);
    assert!(deserialize_choices(&bytes).is_none());
}

/// Construct a DB whose root is a regular file rather than a
/// directory.  Any `create_dir_all` under it will fail — exercising
/// the previously-nocov filesystem-error guards in `save` and
/// `move_value`.
#[cfg(unix)]
#[test]
fn save_silently_returns_when_create_dir_all_fails() {
    let dir = TempDir::new().unwrap();
    let file_root = dir.path().join("im-a-file");
    std::fs::write(&file_root, b"").unwrap();
    let db = DirectoryTestCaseDatabase::new(file_root.to_str().unwrap());
    db.save(b"key", b"value");
    assert!(db.fetch(b"key").is_empty());
}

#[cfg(unix)]
#[test]
fn move_value_falls_back_to_delete_save_when_dst_dir_create_fails() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let db = DirectoryTestCaseDatabase::new(dir.path().to_str().unwrap());
    db.save(b"src", b"val");
    let mut perms = std::fs::metadata(dir.path()).unwrap().permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(dir.path(), perms.clone()).unwrap();
    db.move_value(b"src", b"dst", b"val");
    perms.set_mode(0o755);
    std::fs::set_permissions(dir.path(), perms).unwrap();
}

fn clone_value(children: Vec<ChoiceValue>) -> ChoiceValue {
    ChoiceValue::Clone(std::sync::Arc::new(
        crate::native::core::CloneRecord::from_values(children),
    ))
}

#[test]
fn serialize_roundtrips_clone_values() {
    let choices = vec![
        ChoiceValue::Boolean(true),
        clone_value(vec![
            ChoiceValue::Integer(BigInt::from(42)),
            clone_value(vec![ChoiceValue::String(vec![0x61, 0x62])]),
            clone_value(Vec::new()),
        ]),
        ChoiceValue::Bytes(vec![7]),
    ];
    let bytes = serialize_choices(&choices);
    assert_eq!(deserialize_choices(&bytes), Some(choices));
}

#[test]
fn serialize_clone_drops_realized_info_but_preserves_equality() {
    use crate::native::core::choices::BooleanChoice;
    use crate::native::core::{ChoiceKind, ChoiceNode, CloneRecord, Span, SpanEvent};
    let realized = ChoiceValue::Clone(std::sync::Arc::new(CloneRecord::from_run(
        vec![ChoiceNode::new(
            ChoiceKind::Boolean(BooleanChoice),
            ChoiceValue::Boolean(true),
            false,
        )],
        vec![Span {
            start: 0,
            end: 1,
            label: "9".to_string(),
            depth: 0,
            parent: None,
            discarded: false,
        }],
        vec![(0, SpanEvent::Open { label: 9 })],
    )));
    let bytes = serialize_choices(std::slice::from_ref(&realized));
    let round_tripped = deserialize_choices(&bytes).unwrap();
    assert_eq!(round_tripped.len(), 1);
    assert_eq!(round_tripped[0], realized);
    let ChoiceValue::Clone(record) = &round_tripped[0] else {
        panic!("expected a clone value");
    };
    assert!(record.realized_nodes().is_none());
    assert!(record.spans().is_empty());
}

#[test]
fn deserialize_returns_none_on_truncated_clone_children() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(5);
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.push(1);
    bytes.push(1);
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn deserialize_returns_none_on_missing_clone_header() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(5);
    bytes.extend_from_slice(&[0, 0]);
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn deserialize_rejects_clone_nesting_beyond_max_depth() {
    let depth = crate::native::core::MAX_CLONE_DEPTH + 1;
    let mut bytes = Vec::new();
    for _ in 0..depth {
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.push(5);
    }
    bytes.extend_from_slice(&0u32.to_le_bytes());
    assert!(deserialize_choices(&bytes).is_none());

    let mut ok_bytes = Vec::new();
    for _ in 0..crate::native::core::MAX_CLONE_DEPTH {
        ok_bytes.extend_from_slice(&1u32.to_le_bytes());
        ok_bytes.push(5);
    }
    ok_bytes.extend_from_slice(&0u32.to_le_bytes());
    let decoded = deserialize_choices(&ok_bytes).unwrap();
    assert_eq!(decoded.len(), 1);
}

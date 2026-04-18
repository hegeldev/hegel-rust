use super::*;

// ── serialize / deserialize round-trips ────────────────────────────────────

#[test]
fn test_roundtrip_empty() {
    let choices: Vec<ChoiceValue> = vec![];
    let bytes = serialize_choices(&choices);
    let decoded = deserialize_choices(&bytes).unwrap();
    assert_eq!(decoded, choices);
}

#[test]
fn test_roundtrip_integer() {
    let choices = vec![
        ChoiceValue::Integer(0),
        ChoiceValue::Integer(1),
        ChoiceValue::Integer(-1),
        ChoiceValue::Integer(i128::MAX),
        ChoiceValue::Integer(i128::MIN),
        ChoiceValue::Integer(1_000_000),
    ];
    let bytes = serialize_choices(&choices);
    let decoded = deserialize_choices(&bytes).unwrap();
    assert_eq!(decoded, choices);
}

#[test]
fn test_roundtrip_boolean() {
    let choices = vec![ChoiceValue::Boolean(false), ChoiceValue::Boolean(true)];
    let bytes = serialize_choices(&choices);
    let decoded = deserialize_choices(&bytes).unwrap();
    assert_eq!(decoded, choices);
}

#[test]
fn test_roundtrip_float() {
    let choices = vec![
        ChoiceValue::Float(0.0),
        ChoiceValue::Float(1.0),
        ChoiceValue::Float(-1.0),
        ChoiceValue::Float(f64::INFINITY),
        ChoiceValue::Float(f64::NEG_INFINITY),
        ChoiceValue::Float(f64::NAN),
    ];
    let bytes = serialize_choices(&choices);
    let decoded = deserialize_choices(&bytes).unwrap();
    assert_eq!(decoded, choices);
}

#[test]
fn test_roundtrip_mixed() {
    let choices = vec![
        ChoiceValue::Integer(42),
        ChoiceValue::Boolean(true),
        ChoiceValue::Float(3.125),
        ChoiceValue::Integer(-999),
        ChoiceValue::Boolean(false),
    ];
    let bytes = serialize_choices(&choices);
    let decoded = deserialize_choices(&bytes).unwrap();
    assert_eq!(decoded, choices);
}

// ── deserialize error cases ─────────────────────────────────────────────────

#[test]
fn test_deserialize_empty_bytes_returns_none() {
    assert!(deserialize_choices(&[]).is_none());
}

#[test]
fn test_deserialize_truncated_count_returns_none() {
    assert!(deserialize_choices(&[0, 0, 0]).is_none());
}

#[test]
fn test_deserialize_truncated_integer_returns_none() {
    // count=1, type=Integer, but only 8 of the required 16 value bytes
    let mut bytes = vec![1, 0, 0, 0, 0u8]; // count=1, type=0
    bytes.extend_from_slice(&[0u8; 8]); // only 8 bytes instead of 16
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn test_deserialize_truncated_boolean_returns_none() {
    // count=1, type=Boolean, but no value byte
    let bytes = vec![1, 0, 0, 0, 1u8]; // count=1, type=1, no value
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn test_deserialize_truncated_float_returns_none() {
    // count=1, type=Float, but only 4 of the required 8 value bytes
    let mut bytes = vec![1, 0, 0, 0, 2u8]; // count=1, type=2
    bytes.extend_from_slice(&[0u8; 4]); // only 4 bytes instead of 8
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn test_deserialize_unknown_type_tag_returns_none() {
    // count=1, type=99 (unknown)
    let bytes = vec![1, 0, 0, 0, 99u8];
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn test_deserialize_truncated_string_length_returns_none() {
    // count=1, type=String, but only 2 of the required 4 codepoint-count bytes.
    let mut bytes = vec![1, 0, 0, 0, 4u8]; // count=1, type=4
    bytes.extend_from_slice(&[0u8; 2]);
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn test_deserialize_truncated_string_payload_returns_none() {
    // count=1, type=String, codepoint count=5 (20 payload bytes expected),
    // but only 4 bytes of payload.
    let mut bytes = vec![1, 0, 0, 0, 4u8]; // count=1, type=4
    bytes.extend_from_slice(&5u32.to_le_bytes()); // 5 codepoints
    bytes.extend_from_slice(&[0u8; 4]); // 4 bytes instead of 20
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn test_deserialize_out_of_range_codepoint_returns_none() {
    // count=1, type=String, codepoint count=1, payload is a u32 above 0x10FFFF.
    let mut bytes = vec![1, 0, 0, 0, 4u8];
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0x20_0000u32.to_le_bytes());
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn test_roundtrip_string() {
    let choices = vec![
        ChoiceValue::String(vec![]),
        ChoiceValue::String(vec![b'a' as u32, b'b' as u32, b'c' as u32]),
        // A non-BMP codepoint (U+1F600, 😀) round-trips as a single u32 entry.
        ChoiceValue::String(vec![0x1F600]),
        // Surrogate codepoints are preserved in the raw-u32 representation.
        ChoiceValue::String(vec![0xD800, 0xDFFF]),
    ];
    let bytes = serialize_choices(&choices);
    let decoded = deserialize_choices(&bytes).unwrap();
    assert_eq!(decoded, choices);
}

#[test]
fn test_deserialize_count_exceeds_data_returns_none() {
    // count=5 but no data
    let bytes = vec![5, 0, 0, 0];
    assert!(deserialize_choices(&bytes).is_none());
}

// ── NativeDatabase save / fetch / delete / move_value ───────────────────────
//
// Mirrors `tests/cover/test_database_backend.py` in the Hypothesis tree:
// multi-value round-trips, idempotent save, delete, and move semantics.

#[test]
fn test_database_fetch_missing_key_returns_empty() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    assert!(db.fetch(b"no-such-key").is_empty());
}

#[test]
fn test_database_save_and_fetch_roundtrip() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"foo", b"bar");
    assert_eq!(db.fetch(b"foo"), vec![b"bar".to_vec()]);
}

#[test]
fn test_database_multiple_values_per_key() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"k", b"v1");
    db.save(b"k", b"v2");
    db.save(b"k", b"v3");
    let mut got = db.fetch(b"k");
    got.sort();
    assert_eq!(got, vec![b"v1".to_vec(), b"v2".to_vec(), b"v3".to_vec()]);
}

#[test]
fn test_database_save_same_value_twice_is_idempotent() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"k", b"v");
    db.save(b"k", b"v");
    assert_eq!(db.fetch(b"k"), vec![b"v".to_vec()]);
}

#[test]
fn test_database_different_keys_are_independent() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"key-a", b"va");
    db.save(b"key-b", b"vb");
    assert_eq!(db.fetch(b"key-a"), vec![b"va".to_vec()]);
    assert_eq!(db.fetch(b"key-b"), vec![b"vb".to_vec()]);
}

#[test]
fn test_database_delete_value() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"k", b"a");
    db.save(b"k", b"b");
    db.delete(b"k", b"a");
    assert_eq!(db.fetch(b"k"), vec![b"b".to_vec()]);
}

#[test]
fn test_database_delete_missing_value_is_noop() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"k", b"v");
    db.delete(b"k", b"absent");
    assert_eq!(db.fetch(b"k"), vec![b"v".to_vec()]);
}

#[test]
fn test_database_delete_missing_key_is_noop() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.delete(b"nope", b"v");
}

#[test]
fn test_database_delete_last_value_removes_key_dir() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"k", b"v");
    db.delete(b"k", b"v");
    assert!(db.fetch(b"k").is_empty());
    // Key directory should be cleaned up after its last entry is removed.
    let key_dir = dir.path().join(fnv_hex(b"k"));
    assert!(!key_dir.exists());
}

#[test]
fn test_database_move_value() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"src", b"v");
    db.move_value(b"src", b"dst", b"v");
    assert!(db.fetch(b"src").is_empty());
    assert_eq!(db.fetch(b"dst"), vec![b"v".to_vec()]);
}

#[test]
fn test_database_move_absent_value_still_lands_at_dst() {
    // Mirrors `test_an_absent_value_is_present_after_it_moves` in
    // Hypothesis's test_database_backend.py.
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.move_value(b"a", b"b", b"c");
    assert_eq!(db.fetch(b"b"), vec![b"c".to_vec()]);
}

#[test]
fn test_database_move_to_self_inserts_value() {
    // Mirrors `test_an_absent_value_is_present_after_it_moves_to_self`.
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.move_value(b"a", b"a", b"b");
    assert_eq!(db.fetch(b"a"), vec![b"b".to_vec()]);
}

#[test]
fn test_database_two_instances_share_storage() {
    // Mirrors `test_two_directory_databases_can_interact`.
    let dir = tempfile::TempDir::new().unwrap();
    let db1 = NativeDatabase::new(dir.path().to_str().unwrap());
    let db2 = NativeDatabase::new(dir.path().to_str().unwrap());
    db1.save(b"foo", b"bar");
    assert_eq!(db2.fetch(b"foo"), vec![b"bar".to_vec()]);
    db2.save(b"foo", b"baz");
    let mut got = db1.fetch(b"foo");
    got.sort();
    assert_eq!(got, vec![b"bar".to_vec(), b"baz".to_vec()]);
}

#[test]
fn test_database_fetch_skips_unreadable_entries() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"k", b"v");
    // Create a rogue subdirectory inside the key dir; `fetch` must not
    // crash on it (Hypothesis's `test_can_handle_disappearing_files`
    // covers the same graceful-degradation path).
    let key_dir = dir.path().join(fnv_hex(b"k"));
    std::fs::create_dir(key_dir.join("not-a-file")).unwrap();
    assert_eq!(db.fetch(b"k"), vec![b"v".to_vec()]);
}

#[test]
fn test_database_save_to_non_writable_dir_does_not_panic() {
    // Use a path that cannot be created (file exists where dir should be).
    let dir = tempfile::TempDir::new().unwrap();
    let blocking_file = dir.path().join("blocked");
    std::fs::write(&blocking_file, b"").unwrap();
    let db = NativeDatabase::new(blocking_file.join("sub").to_str().unwrap());
    db.save(b"k", b"v"); // must not panic
    assert!(db.fetch(b"k").is_empty());
}

#[test]
fn test_database_stores_serialized_choices() {
    // End-to-end: the replay path in `runner.rs` round-trips
    // ChoiceValue sequences through `serialize_choices`.
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    let choices = vec![ChoiceValue::Integer(1_000_000), ChoiceValue::Boolean(false)];
    db.save(b"my-test", &serialize_choices(&choices));
    let raw = db.fetch(b"my-test").into_iter().next().unwrap();
    assert_eq!(deserialize_choices(&raw).unwrap(), choices);
}

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

// ── NativeDatabase load / save ──────────────────────────────────────────────

#[test]
fn test_database_load_missing_key_returns_none() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    assert!(db.load("no-such-key").is_none());
}

#[test]
fn test_database_save_and_load_roundtrip() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    let choices = vec![ChoiceValue::Integer(1_000_000), ChoiceValue::Boolean(false)];
    db.save("my-test", &choices);
    let loaded = db.load("my-test").unwrap();
    assert_eq!(loaded, choices);
}

#[test]
fn test_database_different_keys_are_independent() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    let choices_a = vec![ChoiceValue::Integer(1)];
    let choices_b = vec![ChoiceValue::Integer(2)];
    db.save("key-a", &choices_a);
    db.save("key-b", &choices_b);
    assert_eq!(db.load("key-a").unwrap(), choices_a);
    assert_eq!(db.load("key-b").unwrap(), choices_b);
}

#[test]
fn test_database_save_overwrites_previous() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save("k", &[ChoiceValue::Integer(100)]);
    db.save("k", &[ChoiceValue::Integer(1)]);
    let loaded = db.load("k").unwrap();
    assert_eq!(loaded, vec![ChoiceValue::Integer(1)]);
}

#[test]
fn test_database_load_corrupt_file_returns_none() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    // Manually write corrupt bytes into the expected path.
    let key = "corrupt-key";
    let key_dir = dir.path().join(fnv_hex(key));
    std::fs::create_dir_all(&key_dir).unwrap();
    std::fs::write(key_dir.join("best"), b"not valid binary data!!!").unwrap();
    assert!(db.load(key).is_none());
}

#[test]
fn test_database_save_to_non_writable_dir_does_not_panic() {
    // Use a path that cannot be created (file exists where dir should be).
    let dir = tempfile::TempDir::new().unwrap();
    let blocking_file = dir.path().join("blocked");
    std::fs::write(&blocking_file, b"").unwrap();
    // Try to use the file as a db_root subdirectory — create_dir_all should
    // fail, but save() must not panic.
    let db = NativeDatabase::new(blocking_file.join("sub").to_str().unwrap());
    db.save("k", &[ChoiceValue::Integer(0)]); // must not panic
}

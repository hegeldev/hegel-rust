// Embedded tests for src/native/database.rs — exercise the directory
// backend's save/fetch/delete/move_value paths plus the binary
// `serialize_choices` / `deserialize_choices` round-trip.

use super::*;
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
    db.delete(b"key", b"nonexistent"); // must not panic
    assert!(db.fetch(b"key").is_empty());
}

#[test]
fn fetch_of_absent_key_returns_empty() {
    let (db, _dir) = fresh_db();
    assert!(db.fetch(b"never-saved").is_empty());
}

#[test]
fn move_value_within_same_key_is_a_resave() {
    // src == dst is the early-return branch in `move_value`.
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
    // Exercises the `if !self.key_path(dst).exists()` branch.
    let (db, _dir) = fresh_db();
    db.save(b"src", b"v");
    db.move_value(b"src", b"brand-new-dst", b"v");
    assert_eq!(db.fetch(b"brand-new-dst"), vec![b"v".to_vec()]);
}

#[test]
fn move_value_falls_back_to_delete_save_when_rename_fails() {
    // `rename` fails when the source value file doesn't exist; the
    // fallback path inserts at dst anyway.
    let (db, _dir) = fresh_db();
    db.save(b"dst", b"existing"); // dst exists, src value file does not
    db.move_value(b"src", b"dst", b"v");
    let mut got = db.fetch(b"dst");
    got.sort();
    let mut want = vec![b"existing".to_vec(), b"v".to_vec()];
    want.sort();
    assert_eq!(got, want);
}

// ── serialize_choices / deserialize_choices ──────────────────────────────

#[test]
fn round_trip_integer_choices() {
    let choices = vec![
        ChoiceValue::Integer(0),
        ChoiceValue::Integer(-1),
        ChoiceValue::Integer(i128::MAX),
        ChoiceValue::Integer(i128::MIN),
    ];
    let bytes = serialize_choices(&choices);
    assert_eq!(deserialize_choices(&bytes), Some(choices));
}

#[test]
fn round_trip_mixed_choices() {
    let choices = vec![
        ChoiceValue::Boolean(false),
        ChoiceValue::Integer(42),
        ChoiceValue::Boolean(true),
    ];
    let bytes = serialize_choices(&choices);
    assert_eq!(deserialize_choices(&bytes), Some(choices));
}

#[test]
fn deserialize_returns_none_on_too_short_header() {
    // < 4 bytes can't hold the count prefix.
    assert!(deserialize_choices(&[0, 0, 0]).is_none());
}

#[test]
fn deserialize_returns_none_on_missing_type_tag() {
    // count = 1 but no body.
    assert!(deserialize_choices(&1u32.to_le_bytes()).is_none());
}

#[test]
fn deserialize_returns_none_on_truncated_integer_body() {
    // count = 1, type tag 0 (Integer), but fewer than 16 bytes follow.
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(0); // type tag = Integer
    bytes.extend_from_slice(&[0u8; 8]); // only 8 of 16 needed bytes
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn deserialize_returns_none_on_truncated_boolean_body() {
    // count = 1, type tag 1 (Boolean), but no value byte.
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(1); // type tag = Boolean — nothing after
    assert!(deserialize_choices(&bytes).is_none());
}

#[test]
fn deserialize_returns_none_on_unknown_type_tag() {
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(42); // unknown type tag
    assert!(deserialize_choices(&bytes).is_none());
}

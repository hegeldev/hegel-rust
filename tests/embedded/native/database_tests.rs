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

// ── ExampleDatabase trait: fixture-parametrized tests ──────────────────────
//
// Mirrors the `exampledatabase` fixture tests in
// `tests/cover/test_database_backend.py`, which are parametrized over
// `InMemoryExampleDatabase` and `DirectoryBasedExampleDatabase`.
// Each behaviour is expressed as an `assert_*` helper taking
// `&dyn ExampleDatabase` and is driven once per backend.

fn assert_can_delete_a_key_that_is_not_present(db: &dyn ExampleDatabase) {
    db.delete(b"foo", b"bar");
}

fn assert_can_fetch_a_key_that_is_not_present(db: &dyn ExampleDatabase) {
    assert!(db.fetch(b"foo").is_empty());
}

fn assert_saving_a_key_twice_fetches_it_once(db: &dyn ExampleDatabase) {
    db.save(b"foo", b"bar");
    db.save(b"foo", b"bar");
    assert_eq!(db.fetch(b"foo"), vec![b"bar".to_vec()]);
}

fn assert_absent_value_is_present_after_it_moves(db: &dyn ExampleDatabase) {
    db.move_value(b"a", b"b", b"c");
    assert_eq!(db.fetch(b"b"), vec![b"c".to_vec()]);
}

fn assert_absent_value_is_present_after_it_moves_to_self(db: &dyn ExampleDatabase) {
    db.move_value(b"a", b"a", b"b");
    assert_eq!(db.fetch(b"a"), vec![b"b".to_vec()]);
}

#[test]
fn test_memory_can_delete_a_key_that_is_not_present() {
    assert_can_delete_a_key_that_is_not_present(&InMemoryNativeDatabase::new());
}

#[test]
fn test_directory_can_delete_a_key_that_is_not_present() {
    let dir = tempfile::TempDir::new().unwrap();
    assert_can_delete_a_key_that_is_not_present(&NativeDatabase::new(dir.path().to_str().unwrap()));
}

#[test]
fn test_memory_can_fetch_a_key_that_is_not_present() {
    assert_can_fetch_a_key_that_is_not_present(&InMemoryNativeDatabase::new());
}

#[test]
fn test_directory_can_fetch_a_key_that_is_not_present() {
    let dir = tempfile::TempDir::new().unwrap();
    assert_can_fetch_a_key_that_is_not_present(&NativeDatabase::new(dir.path().to_str().unwrap()));
}

#[test]
fn test_memory_saving_a_key_twice_fetches_it_once() {
    assert_saving_a_key_twice_fetches_it_once(&InMemoryNativeDatabase::new());
}

#[test]
fn test_directory_saving_a_key_twice_fetches_it_once() {
    let dir = tempfile::TempDir::new().unwrap();
    assert_saving_a_key_twice_fetches_it_once(&NativeDatabase::new(dir.path().to_str().unwrap()));
}

#[test]
fn test_memory_absent_value_is_present_after_it_moves() {
    assert_absent_value_is_present_after_it_moves(&InMemoryNativeDatabase::new());
}

#[test]
fn test_directory_absent_value_is_present_after_it_moves() {
    let dir = tempfile::TempDir::new().unwrap();
    assert_absent_value_is_present_after_it_moves(&NativeDatabase::new(
        dir.path().to_str().unwrap(),
    ));
}

#[test]
fn test_memory_absent_value_is_present_after_it_moves_to_self() {
    assert_absent_value_is_present_after_it_moves_to_self(&InMemoryNativeDatabase::new());
}

#[test]
fn test_directory_absent_value_is_present_after_it_moves_to_self() {
    let dir = tempfile::TempDir::new().unwrap();
    assert_absent_value_is_present_after_it_moves_to_self(&NativeDatabase::new(
        dir.path().to_str().unwrap(),
    ));
}

// ── InMemoryNativeDatabase-specific tests ──────────────────────────────────
//
// Mirror the InMemory-only tests from `test_database_backend.py`.

#[test]
fn test_in_memory_backend_returns_what_you_put_in() {
    // Direct port of `test_backend_returns_what_you_put_in`. The upstream
    // test is `@given(lists(tuples(binary(), binary())))` — here we pick a
    // fixed representative sample (including duplicate keys and duplicate
    // (key, value) pairs) so the embedded suite stays a plain unit test.
    let db = InMemoryNativeDatabase::new();
    let pairs: Vec<(&[u8], &[u8])> = vec![
        (b"", b""),
        (b"foo", b"bar"),
        (b"foo", b"baz"),
        (b"foo", b"bar"),
        (b"key", b"value"),
        (b"key", b""),
    ];
    let mut mapping: std::collections::HashMap<&[u8], std::collections::HashSet<Vec<u8>>> =
        std::collections::HashMap::new();
    for (k, v) in &pairs {
        mapping.entry(*k).or_default().insert(v.to_vec());
        db.save(k, v);
    }
    for (k, expected) in &mapping {
        let contents = db.fetch(k);
        let distinct: std::collections::HashSet<Vec<u8>> = contents.iter().cloned().collect();
        assert_eq!(contents.len(), distinct.len());
        assert_eq!(&distinct, expected);
    }
}

#[test]
fn test_in_memory_can_delete_keys() {
    let db = InMemoryNativeDatabase::new();
    db.save(b"foo", b"bar");
    db.save(b"foo", b"baz");
    db.delete(b"foo", b"bar");
    assert_eq!(db.fetch(b"foo"), vec![b"baz".to_vec()]);
}

#[test]
fn test_in_memory_delete_missing_value_is_noop() {
    let db = InMemoryNativeDatabase::new();
    db.save(b"k", b"v");
    db.delete(b"k", b"absent");
    assert_eq!(db.fetch(b"k"), vec![b"v".to_vec()]);
}

#[test]
fn test_in_memory_multiple_values_per_key() {
    let db = InMemoryNativeDatabase::new();
    db.save(b"k", b"v1");
    db.save(b"k", b"v2");
    db.save(b"k", b"v3");
    let mut got = db.fetch(b"k");
    got.sort();
    assert_eq!(got, vec![b"v1".to_vec(), b"v2".to_vec(), b"v3".to_vec()]);
}

#[test]
fn test_in_memory_default_is_empty() {
    let db = InMemoryNativeDatabase::default();
    assert!(db.fetch(b"anything").is_empty());
}

#[test]
fn test_in_memory_move_uses_default_trait_impl() {
    // `InMemoryNativeDatabase` does not override `move_value`, so the
    // trait's default delete-then-save runs here.
    let db = InMemoryNativeDatabase::new();
    db.save(b"src", b"v");
    db.move_value(b"src", b"dst", b"v");
    assert!(db.fetch(b"src").is_empty());
    assert_eq!(db.fetch(b"dst"), vec![b"v".to_vec()]);
}

// ── ReadOnlyNativeDatabase ─────────────────────────────────────────────────
//
// Mirrors `test_readonly_db_is_not_writable` in Hypothesis's
// `test_database_backend.py`.

#[test]
fn test_readonly_db_is_not_writable() {
    let inner = std::sync::Arc::new(InMemoryNativeDatabase::new());
    inner.save(b"key", b"value");
    inner.save(b"key", b"value2");
    let wrapped = ReadOnlyNativeDatabase::new(inner.clone());
    wrapped.delete(b"key", b"value");
    wrapped.move_value(b"key", b"key2", b"value2");
    wrapped.save(b"key", b"value3");
    let mut got = wrapped.fetch(b"key");
    got.sort();
    assert_eq!(got, vec![b"value".to_vec(), b"value2".to_vec()]);
    assert!(wrapped.fetch(b"key2").is_empty());
    // Inner database is unchanged by the wrapper's writes.
    let mut got = inner.fetch(b"key");
    got.sort();
    assert_eq!(got, vec![b"value".to_vec(), b"value2".to_vec()]);
    assert!(inner.fetch(b"key2").is_empty());
}

#[test]
fn test_readonly_db_forwards_fetch() {
    let inner = InMemoryNativeDatabase::new();
    inner.save(b"k", b"v");
    let wrapped = ReadOnlyNativeDatabase::new(inner);
    assert_eq!(wrapped.fetch(b"k"), vec![b"v".to_vec()]);
}

// ── MultiplexedNativeDatabase ──────────────────────────────────────────────
//
// Mirrors `test_multiplexed_dbs_read_and_write_all`.

#[test]
fn test_multiplexed_dbs_read_and_write_all() {
    use std::sync::Arc;
    let a = Arc::new(InMemoryNativeDatabase::new());
    let b = Arc::new(InMemoryNativeDatabase::new());
    let multi = MultiplexedNativeDatabase::new(vec![
        a.clone() as Arc<dyn ExampleDatabase>,
        b.clone() as Arc<dyn ExampleDatabase>,
    ]);
    a.save(b"a", b"aa");
    b.save(b"b", b"bb");
    multi.save(b"c", b"cc");
    multi.move_value(b"a", b"b", b"aa");
    let dbs: [&dyn ExampleDatabase; 3] = [a.as_ref(), b.as_ref(), &multi];
    for db in &dbs {
        assert!(db.fetch(b"a").is_empty());
        assert_eq!(db.fetch(b"c"), vec![b"cc".to_vec()]);
    }
    let got = multi.fetch(b"b");
    assert_eq!(got.len(), 2);
    let mut got_sorted = got.clone();
    got_sorted.sort();
    assert_eq!(got_sorted, vec![b"aa".to_vec(), b"bb".to_vec()]);
    multi.delete(b"c", b"cc");
    for db in &dbs {
        assert!(db.fetch(b"c").is_empty());
    }
}

#[test]
fn test_multiplexed_fetch_deduplicates_across_dbs() {
    use std::sync::Arc;
    let a = Arc::new(InMemoryNativeDatabase::new());
    let b = Arc::new(InMemoryNativeDatabase::new());
    a.save(b"k", b"v");
    b.save(b"k", b"v");
    let multi = MultiplexedNativeDatabase::new(vec![
        a.clone() as Arc<dyn ExampleDatabase>,
        b.clone() as Arc<dyn ExampleDatabase>,
    ]);
    assert_eq!(multi.fetch(b"k"), vec![b"v".to_vec()]);
}

// ── BackgroundWriteNativeDatabase ──────────────────────────────────────────
//
// Mirrors `test_background_write_database`.

#[test]
fn test_background_write_database() {
    let db = BackgroundWriteNativeDatabase::new(InMemoryNativeDatabase::new());
    db.save(b"a", b"b");
    db.save(b"a", b"c");
    db.save(b"a", b"d");
    let mut got = db.fetch(b"a");
    got.sort();
    assert_eq!(got, vec![b"b".to_vec(), b"c".to_vec(), b"d".to_vec()]);

    db.move_value(b"a", b"a2", b"b");
    let mut got = db.fetch(b"a");
    got.sort();
    assert_eq!(got, vec![b"c".to_vec(), b"d".to_vec()]);
    assert_eq!(db.fetch(b"a2"), vec![b"b".to_vec()]);

    db.delete(b"a", b"c");
    assert_eq!(db.fetch(b"a"), vec![b"d".to_vec()]);
}

#[test]
fn test_background_write_flushes_on_drop() {
    // Ensure that enqueued writes are flushed to the inner database
    // before the wrapper is dropped. Using an `Arc<InMemoryNativeDatabase>`
    // as the backing store lets us inspect state after the wrapper goes
    // away.
    use std::sync::Arc;
    let inner = Arc::new(InMemoryNativeDatabase::new());
    {
        let bg = BackgroundWriteNativeDatabase::new(inner.clone());
        bg.save(b"k", b"v1");
        bg.save(b"k", b"v2");
        // Do not call fetch — rely on Drop to flush.
    }
    let mut got = inner.fetch(b"k");
    got.sort();
    assert_eq!(got, vec![b"v1".to_vec(), b"v2".to_vec()]);
}

// ── Listener API ───────────────────────────────────────────────────────────
//
// Mirrors the listener tests in Hypothesis's `test_database_backend.py`:
// `test_can_remove_nonexistent_listener`, `test_readonly_listener`, and
// `test_start_end_listening`. The `_database_conforms_to_listener_api`
// state-machine test is tracked as a separate TODO.

use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicUsize, Ordering};

fn record_events() -> (Arc<StdMutex<Vec<ListenerEvent>>>, Listener) {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);
    let listener: Listener = Arc::new(move |event: &ListenerEvent| {
        events_clone.lock().unwrap().push(event.clone());
    });
    (events, listener)
}

#[test]
fn test_can_remove_nonexistent_listener() {
    // Hypothesis: `test_can_remove_nonexistent_listener`.
    let db = InMemoryNativeDatabase::new();
    let listener: Listener = Arc::new(|_event: &ListenerEvent| {});
    db.remove_listener(&listener); // must not panic
}

#[test]
fn test_in_memory_save_broadcasts_save_event() {
    let db = InMemoryNativeDatabase::new();
    let (events, listener) = record_events();
    db.add_listener(listener);
    db.save(b"k", b"v");
    let events = events.lock().unwrap();
    assert_eq!(
        *events,
        vec![ListenerEvent::Save {
            key: b"k".to_vec(),
            value: b"v".to_vec()
        }]
    );
}

#[test]
fn test_in_memory_idempotent_save_does_not_broadcast() {
    let db = InMemoryNativeDatabase::new();
    db.save(b"k", b"v");
    let (events, listener) = record_events();
    db.add_listener(listener);
    db.save(b"k", b"v"); // already present
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn test_in_memory_delete_broadcasts_delete_event() {
    let db = InMemoryNativeDatabase::new();
    db.save(b"k", b"v");
    let (events, listener) = record_events();
    db.add_listener(listener);
    db.delete(b"k", b"v");
    let events = events.lock().unwrap();
    assert_eq!(
        *events,
        vec![ListenerEvent::Delete {
            key: b"k".to_vec(),
            value: Some(b"v".to_vec())
        }]
    );
}

#[test]
fn test_in_memory_noop_delete_does_not_broadcast() {
    let db = InMemoryNativeDatabase::new();
    let (events, listener) = record_events();
    db.add_listener(listener);
    db.delete(b"k", b"absent");
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn test_in_memory_move_broadcasts_delete_then_save() {
    let db = InMemoryNativeDatabase::new();
    db.save(b"k1", b"v");
    let (events, listener) = record_events();
    db.add_listener(listener);
    db.move_value(b"k1", b"k2", b"v");
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], ListenerEvent::Delete { .. }));
    assert!(matches!(events[1], ListenerEvent::Save { .. }));
}

#[test]
fn test_in_memory_remove_listener_stops_events() {
    let db = InMemoryNativeDatabase::new();
    let (events, listener) = record_events();
    db.add_listener(Arc::clone(&listener));
    db.save(b"a", b"1");
    db.remove_listener(&listener);
    db.save(b"b", b"2");
    assert_eq!(events.lock().unwrap().len(), 1);
}

#[test]
fn test_in_memory_clear_listeners_removes_all() {
    let db = InMemoryNativeDatabase::new();
    let (events, l1) = record_events();
    let (_, l2) = record_events();
    db.add_listener(l1);
    db.add_listener(l2);
    db.clear_listeners();
    db.save(b"k", b"v");
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn test_in_memory_same_listener_twice_fires_twice_per_event() {
    let db = InMemoryNativeDatabase::new();
    let (events, listener) = record_events();
    db.add_listener(Arc::clone(&listener));
    db.add_listener(Arc::clone(&listener));
    db.save(b"k", b"v");
    assert_eq!(events.lock().unwrap().len(), 2);
}

#[test]
fn test_native_db_save_broadcasts() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    let (events, listener) = record_events();
    db.add_listener(listener);
    db.save(b"k", b"v");
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], ListenerEvent::Save { key, value }
        if key == b"k" && value == b"v"));
}

#[test]
fn test_native_db_delete_broadcasts() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"k", b"v");
    let (events, listener) = record_events();
    db.add_listener(listener);
    db.delete(b"k", b"v");
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0], ListenerEvent::Delete { key, value: Some(v) }
        if key == b"k" && v == b"v")
    );
}

#[test]
fn test_native_db_move_broadcasts_delete_then_save() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"k1", b"v");
    let (events, listener) = record_events();
    db.add_listener(listener);
    db.move_value(b"k1", b"k2", b"v");
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], ListenerEvent::Delete { .. }));
    assert!(matches!(events[1], ListenerEvent::Save { .. }));
}

#[test]
fn test_native_db_move_to_self_broadcasts_single_save() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    let (events, listener) = record_events();
    db.add_listener(listener);
    db.move_value(b"k", b"k", b"v");
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], ListenerEvent::Save { .. }));
}

#[test]
fn test_readonly_listener_never_fires() {
    // Hypothesis: `test_readonly_listener`.
    let inner = Arc::new(InMemoryNativeDatabase::new());
    let wrapped = ReadOnlyNativeDatabase::new(Arc::clone(&inner));
    let (events, listener) = record_events();
    wrapped.add_listener(Arc::clone(&listener));
    wrapped.save(b"a", b"a");
    wrapped.remove_listener(&listener);
    wrapped.save(b"b", b"b");
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn test_multiplexed_listener_fans_out_from_inner_writes() {
    let a = Arc::new(InMemoryNativeDatabase::new());
    let b = Arc::new(InMemoryNativeDatabase::new());
    let multi = MultiplexedNativeDatabase::new(vec![
        Arc::clone(&a) as Arc<dyn ExampleDatabase>,
        Arc::clone(&b) as Arc<dyn ExampleDatabase>,
    ]);
    let (events, listener) = record_events();
    multi.add_listener(listener);
    multi.save(b"k", b"v");
    // Each inner db fires its own event; the proxy re-broadcasts both.
    assert_eq!(events.lock().unwrap().len(), 2);
}

#[test]
fn test_multiplexed_remove_listener_unsubscribes_from_inner() {
    let inner = Arc::new(InMemoryNativeDatabase::new());
    let multi =
        MultiplexedNativeDatabase::new(vec![Arc::clone(&inner) as Arc<dyn ExampleDatabase>]);
    let (events, listener) = record_events();
    multi.add_listener(Arc::clone(&listener));
    multi.save(b"k", b"v");
    assert_eq!(events.lock().unwrap().len(), 1);
    multi.remove_listener(&listener);
    // After removal, writes going through the wrapper (or directly to the
    // inner db) must not reach our listener.
    multi.save(b"k", b"w");
    inner.save(b"k", b"x");
    assert_eq!(events.lock().unwrap().len(), 1);
}

#[test]
fn test_multiplexed_clear_listeners_unsubscribes_from_inner() {
    let inner = Arc::new(InMemoryNativeDatabase::new());
    let multi =
        MultiplexedNativeDatabase::new(vec![Arc::clone(&inner) as Arc<dyn ExampleDatabase>]);
    let (events, listener) = record_events();
    multi.add_listener(listener);
    multi.clear_listeners();
    inner.save(b"k", b"v");
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn test_background_write_listener_fires_on_inner_write() {
    let bg = BackgroundWriteNativeDatabase::new(InMemoryNativeDatabase::new());
    let (events, listener) = record_events();
    bg.add_listener(listener);
    bg.save(b"k", b"v");
    // fetch() drains the worker queue so all enqueued writes have
    // landed on the inner database (and thus fired listener events)
    // by the time it returns.
    bg.fetch(b"k");
    assert_eq!(events.lock().unwrap().len(), 1);
}

#[test]
fn test_background_write_remove_listener_unsubscribes_from_inner() {
    let bg = BackgroundWriteNativeDatabase::new(InMemoryNativeDatabase::new());
    let (events, listener) = record_events();
    bg.add_listener(Arc::clone(&listener));
    bg.save(b"k", b"v");
    bg.fetch(b"k");
    bg.remove_listener(&listener);
    bg.save(b"k2", b"v2");
    bg.fetch(b"k2");
    assert_eq!(events.lock().unwrap().len(), 1);
}

// Hypothesis: `test_start_end_listening`. We express the start/stop hooks
// via the transition return values of the `Listeners` helper.
struct TracksListens {
    starts: AtomicUsize,
    ends: AtomicUsize,
    listeners: Listeners,
}

impl TracksListens {
    fn new() -> Self {
        Self {
            starts: AtomicUsize::new(0),
            ends: AtomicUsize::new(0),
            listeners: Listeners::new(),
        }
    }
}

impl ExampleDatabase for TracksListens {
    fn fetch(&self, _key: &[u8]) -> Vec<Vec<u8>> {
        Vec::new()
    }
    fn save(&self, _key: &[u8], _value: &[u8]) {}
    fn delete(&self, _key: &[u8], _value: &[u8]) {}

    fn add_listener(&self, f: Listener) {
        if self.listeners.add(f) {
            self.starts.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn remove_listener(&self, f: &Listener) {
        let (removed, now_empty) = self.listeners.remove(f);
        if removed && now_empty {
            self.ends.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn clear_listeners(&self) {
        if self.listeners.clear() {
            self.ends.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[test]
fn test_start_end_listening() {
    let db = TracksListens::new();
    let l1: Listener = Arc::new(|_: &ListenerEvent| {});
    let l2: Listener = Arc::new(|_: &ListenerEvent| {});

    assert_eq!(db.starts.load(Ordering::SeqCst), 0);
    db.add_listener(Arc::clone(&l1));
    assert_eq!(db.starts.load(Ordering::SeqCst), 1);
    db.add_listener(Arc::clone(&l2));
    assert_eq!(db.starts.load(Ordering::SeqCst), 1);

    assert_eq!(db.ends.load(Ordering::SeqCst), 0);
    db.remove_listener(&l2);
    assert_eq!(db.ends.load(Ordering::SeqCst), 0);
    db.remove_listener(&l1);
    assert_eq!(db.ends.load(Ordering::SeqCst), 1);

    db.clear_listeners();
    assert_eq!(db.ends.load(Ordering::SeqCst), 1);
}

#[test]
fn test_start_end_listening_clear_triggers_stop() {
    // When listeners are non-empty, clear_listeners() fires the stop hook.
    let db = TracksListens::new();
    let l: Listener = Arc::new(|_: &ListenerEvent| {});
    db.add_listener(Arc::clone(&l));
    assert_eq!(db.starts.load(Ordering::SeqCst), 1);
    db.clear_listeners();
    assert_eq!(db.ends.load(Ordering::SeqCst), 1);
}

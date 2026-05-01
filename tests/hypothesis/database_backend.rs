//! Ported from hypothesis-python/tests/cover/test_database_backend.py
//!
//! Tests the portable, non-Python-specific parts of the database API:
//! multi-value save/fetch/delete/move semantics, the listener API, and the
//! wrapper databases (ReadOnly, Multiplexed, BackgroundWrite). Native-gated
//! because the database types live under `src/native/database.rs` and are
//! exposed only in native mode.
//!
//! Redundant with `tests/embedded/native/database_tests.rs` by policy: the
//! embedded file tests private internals directly; this file is a
//! line-by-line port of the upstream Python tests through the crate's
//! public test-internals surface. A later rationalisation pass may
//! deduplicate them.
//!
//! Skipped from this port (see SKIPPED.md for rationale):
//! - `test_ga_*`, `TestGADReads`, `test_gadb_coverage` — GitHubArtifactDatabase.
//! - `test_nodes_roundtrips`, `test_uleb_128_roundtrips` — Hypothesis wire format.
//! - `test_default_database_is_in_memory`, `test_default_on_disk_database_is_dir`,
//!   `test_database_directory_inaccessible`, `test_deprecated_example_database_*` —
//!   `ExampleDatabase()` zero-arg factory / `_db_for_path`.
//! - `test_warns_when_listening_not_supported` — HypothesisWarning (Python
//!   warning type); hegel-rust has no equivalent runtime warning surface.
#![cfg(feature = "native")]

use hegel::__native_test_internals::{
    BackgroundWriteNativeDatabase, ExampleDatabase, InMemoryNativeDatabase, Listener,
    ListenerEvent, METAKEYS_NAME, MultiplexedNativeDatabase, NativeDatabase,
    ReadOnlyNativeDatabase,
};
use hegel::TestCase;
use hegel::generators::{self as gs, Generator};
use hegel::stateful::{Rule, StateMachine, Variables, run as run_state_machine, variables};
use hegel::{Hegel, Settings};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

fn settings() -> Settings {
    Settings::new().test_cases(50).database(None)
}

// ── test_backend_returns_what_you_put_in ───────────────────────────────────

fn backend_returns_what_you_put_in(db: &dyn ExampleDatabase, pairs: &[(Vec<u8>, Vec<u8>)]) {
    let mut mapping: HashMap<Vec<u8>, HashSet<Vec<u8>>> = HashMap::new();
    for (k, v) in pairs {
        mapping.entry(k.clone()).or_default().insert(v.clone());
        db.save(k, v);
    }
    for (k, values) in &mapping {
        let contents = db.fetch(k);
        let distinct: HashSet<Vec<u8>> = contents.iter().cloned().collect();
        assert_eq!(contents.len(), distinct.len());
        assert_eq!(&distinct, values);
    }
}

#[test]
fn test_backend_returns_what_you_put_in_memory() {
    Hegel::new(|tc: TestCase| {
        let pairs: Vec<(Vec<u8>, Vec<u8>)> = tc.draw(gs::vecs(
            gs::tuples!(gs::binary(), gs::binary()).map(|(k, v)| (k, v)),
        ));
        let db = InMemoryNativeDatabase::new();
        backend_returns_what_you_put_in(&db, &pairs);
    })
    .settings(settings())
    .run();
}

#[test]
fn test_backend_returns_what_you_put_in_directory() {
    Hegel::new(|tc: TestCase| {
        let pairs: Vec<(Vec<u8>, Vec<u8>)> = tc.draw(gs::vecs(
            gs::tuples!(gs::binary(), gs::binary()).map(|(k, v)| (k, v)),
        ));
        let dir = tempfile::TempDir::new().unwrap();
        let db = NativeDatabase::new(dir.path().to_str().unwrap());
        backend_returns_what_you_put_in(&db, &pairs);
    })
    .settings(settings())
    .run();
}

// ── test_can_delete_keys ───────────────────────────────────────────────────

#[test]
fn test_can_delete_keys() {
    let backend = InMemoryNativeDatabase::new();
    backend.save(b"foo", b"bar");
    backend.save(b"foo", b"baz");
    backend.delete(b"foo", b"bar");
    assert_eq!(backend.fetch(b"foo"), vec![b"baz".to_vec()]);
}

// ── test_does_not_error_when_fetching_when_not_exist ───────────────────────

#[test]
fn test_does_not_error_when_fetching_when_not_exist() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().join("examples").to_str().unwrap());
    db.fetch(b"foo");
}

// ── fixture-parametrized tests (memory + directory) ────────────────────────

fn can_delete_a_key_that_is_not_present(db: &dyn ExampleDatabase) {
    db.delete(b"foo", b"bar");
}

fn can_fetch_a_key_that_is_not_present(db: &dyn ExampleDatabase) {
    assert!(db.fetch(b"foo").is_empty());
}

fn saving_a_key_twice_fetches_it_once(db: &dyn ExampleDatabase) {
    db.save(b"foo", b"bar");
    db.save(b"foo", b"bar");
    assert_eq!(db.fetch(b"foo"), vec![b"bar".to_vec()]);
}

fn can_close_a_database_after_saving(db: &dyn ExampleDatabase) {
    db.save(b"foo", b"bar");
}

fn an_absent_value_is_present_after_it_moves(db: &dyn ExampleDatabase) {
    db.move_value(b"a", b"b", b"c");
    assert_eq!(db.fetch(b"b"), vec![b"c".to_vec()]);
}

fn an_absent_value_is_present_after_it_moves_to_self(db: &dyn ExampleDatabase) {
    db.move_value(b"a", b"a", b"b");
    assert_eq!(db.fetch(b"a"), vec![b"b".to_vec()]);
}

fn run_parametrized(f: fn(&dyn ExampleDatabase)) {
    f(&InMemoryNativeDatabase::new());
    let dir = tempfile::TempDir::new().unwrap();
    f(&NativeDatabase::new(
        dir.path().join("examples").to_str().unwrap(),
    ));
}

#[test]
fn test_can_delete_a_key_that_is_not_present() {
    run_parametrized(can_delete_a_key_that_is_not_present);
}

#[test]
fn test_can_fetch_a_key_that_is_not_present() {
    run_parametrized(can_fetch_a_key_that_is_not_present);
}

#[test]
fn test_saving_a_key_twice_fetches_it_once() {
    run_parametrized(saving_a_key_twice_fetches_it_once);
}

#[test]
fn test_can_close_a_database_after_saving() {
    run_parametrized(can_close_a_database_after_saving);
}

#[test]
fn test_an_absent_value_is_present_after_it_moves() {
    run_parametrized(an_absent_value_is_present_after_it_moves);
}

#[test]
fn test_an_absent_value_is_present_after_it_moves_to_self() {
    run_parametrized(an_absent_value_is_present_after_it_moves_to_self);
}

// ── test_class_name_is_in_repr ─────────────────────────────────────────────
//
// Python asserts `type(db).__name__ in repr(db)`. Rust's closest analogue is
// `std::any::type_name::<T>()`, which always includes the type name.

#[test]
fn test_class_name_is_in_repr() {
    let in_mem = InMemoryNativeDatabase::new();
    let name = std::any::type_name_of_val(&in_mem);
    assert!(name.contains("InMemoryNativeDatabase"), "got {name}");

    let dir = tempfile::TempDir::new().unwrap();
    let on_disk = NativeDatabase::new(dir.path().to_str().unwrap());
    let name = std::any::type_name_of_val(&on_disk);
    assert!(name.contains("NativeDatabase"), "got {name}");
}

// ── test_two_directory_databases_can_interact ──────────────────────────────

#[test]
fn test_two_directory_databases_can_interact() {
    let dir = tempfile::TempDir::new().unwrap();
    let db1 = NativeDatabase::new(dir.path().to_str().unwrap());
    let db2 = NativeDatabase::new(dir.path().to_str().unwrap());
    db1.save(b"foo", b"bar");
    assert_eq!(db2.fetch(b"foo"), vec![b"bar".to_vec()]);
    db2.save(b"foo", b"bar");
    db2.save(b"foo", b"baz");
    let mut got = db1.fetch(b"foo");
    got.sort();
    assert_eq!(got, vec![b"bar".to_vec(), b"baz".to_vec()]);
}

// ── test_can_handle_disappearing_files ─────────────────────────────────────
//
// Python monkeypatches `os.listdir` to inject a phantom entry. In Rust we
// make the same scenario by creating a subdirectory inside the key
// directory — `fetch` must tolerate entries it cannot read and still
// surface the valid value.

#[test]
fn test_can_handle_disappearing_files() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"foo", b"bar");
    let key_dir = db.key_path(b"foo");
    std::fs::create_dir(key_dir.join("this-does-not-exist")).unwrap();
    assert_eq!(db.fetch(b"foo"), vec![b"bar".to_vec()]);
}

// ── test_readonly_db_is_not_writable ───────────────────────────────────────

#[test]
fn test_readonly_db_is_not_writable() {
    let inner = Arc::new(InMemoryNativeDatabase::new());
    inner.save(b"key", b"value");
    inner.save(b"key", b"value2");
    let wrapped = ReadOnlyNativeDatabase::new(Arc::clone(&inner));
    wrapped.delete(b"key", b"value");
    wrapped.move_value(b"key", b"key2", b"value2");
    wrapped.save(b"key", b"value3");
    let mut got = wrapped.fetch(b"key");
    got.sort();
    assert_eq!(got, vec![b"value".to_vec(), b"value2".to_vec()]);
    assert!(wrapped.fetch(b"key2").is_empty());
}

// ── test_multiplexed_dbs_read_and_write_all ────────────────────────────────

#[test]
fn test_multiplexed_dbs_read_and_write_all() {
    let a = Arc::new(InMemoryNativeDatabase::new());
    let b = Arc::new(InMemoryNativeDatabase::new());
    let multi = MultiplexedNativeDatabase::new(vec![
        Arc::clone(&a) as Arc<dyn ExampleDatabase>,
        Arc::clone(&b) as Arc<dyn ExampleDatabase>,
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

// ── test_background_write_database ─────────────────────────────────────────

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

// ── Listener tests ─────────────────────────────────────────────────────────

fn record_events() -> (Arc<Mutex<Vec<ListenerEvent>>>, Listener) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);
    let listener: Listener = Arc::new(move |event: &ListenerEvent| {
        events_clone.lock().unwrap().push(event.clone());
    });
    (events, listener)
}

#[test]
fn test_can_remove_nonexistent_listener() {
    let db = InMemoryNativeDatabase::new();
    let listener: Listener = Arc::new(|_event: &ListenerEvent| {});
    db.remove_listener(&listener);
}

#[test]
fn test_readonly_listener() {
    let inner = Arc::new(InMemoryNativeDatabase::new());
    let wrapped = ReadOnlyNativeDatabase::new(Arc::clone(&inner));
    let (events, listener) = record_events();
    wrapped.add_listener(Arc::clone(&listener));
    wrapped.save(b"a", b"a");
    wrapped.remove_listener(&listener);
    wrapped.save(b"b", b"b");
    assert!(events.lock().unwrap().is_empty());
}

// ── test_start_end_listening ───────────────────────────────────────────────
//
// Hypothesis tracks the _start_listening / _stop_listening hook calls by
// subclassing `ExampleDatabase`. We do the same: a custom `ExampleDatabase`
// that counts 0↔1 listener-count transitions via the standard `Listeners`
// helper.

use hegel::__native_test_internals::Listeners;
use std::sync::atomic::{AtomicUsize, Ordering};

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
    fn as_any(&self) -> &dyn std::any::Any {
        self
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

// ── metakeys tests (DirectoryBasedExampleDatabase-specific) ────────────────

#[test]
fn test_metakeys_move_into_existing_key() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"k1", b"v1");
    db.save(b"k1", b"v2");
    db.save(b"k2", b"v3");
    let got: HashSet<Vec<u8>> = db.fetch(METAKEYS_NAME).into_iter().collect();
    assert_eq!(got, [b"k1".to_vec(), b"k2".to_vec()].into_iter().collect());

    db.move_value(b"k1", b"k2", b"v2");
    let got: HashSet<Vec<u8>> = db.fetch(METAKEYS_NAME).into_iter().collect();
    assert_eq!(got, [b"k1".to_vec(), b"k2".to_vec()].into_iter().collect());
}

#[test]
fn test_metakeys_move_into_nonexistent_key() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"k1", b"v1");
    let got: HashSet<Vec<u8>> = db.fetch(METAKEYS_NAME).into_iter().collect();
    assert_eq!(got, [b"k1".to_vec()].into_iter().collect());

    db.move_value(b"k1", b"k2", b"v1");
    let got: HashSet<Vec<u8>> = db.fetch(METAKEYS_NAME).into_iter().collect();
    assert_eq!(got, [b"k1".to_vec(), b"k2".to_vec()].into_iter().collect());
}

#[test]
fn test_metakeys() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());

    db.save(b"k1", b"v1");
    let got: HashSet<Vec<u8>> = db.fetch(METAKEYS_NAME).into_iter().collect();
    assert_eq!(got, [b"k1".to_vec()].into_iter().collect());

    db.save(b"k1", b"v2");
    let got: HashSet<Vec<u8>> = db.fetch(METAKEYS_NAME).into_iter().collect();
    assert_eq!(got, [b"k1".to_vec()].into_iter().collect());

    db.delete(b"k1", b"v1");
    db.delete(b"k1", b"v2");
    assert!(db.fetch(METAKEYS_NAME).is_empty());

    db.save(b"k2", b"v1");
    let got: HashSet<Vec<u8>> = db.fetch(METAKEYS_NAME).into_iter().collect();
    assert_eq!(got, [b"k2".to_vec()].into_iter().collect());
}

// ── test_directory_db_removes_empty_dirs ───────────────────────────────────

#[test]
fn test_directory_db_removes_empty_dirs() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(dir.path().to_str().unwrap());
    db.save(b"k1", b"v1");
    db.save(b"k1", b"v2");
    assert!(db.key_path(b"k1").exists());
    let got: HashSet<Vec<u8>> = db.fetch(METAKEYS_NAME).into_iter().collect();
    assert_eq!(got, [b"k1".to_vec()].into_iter().collect());

    db.delete(b"k1", b"v1");
    assert!(db.key_path(b"k1").exists());
    let got: HashSet<Vec<u8>> = db.fetch(METAKEYS_NAME).into_iter().collect();
    assert_eq!(got, [b"k1".to_vec()].into_iter().collect());

    db.delete(b"k1", b"v2");
    assert!(!db.key_path(b"k1").exists());
    assert!(db.fetch(METAKEYS_NAME).is_empty());
}

// ── `_database_conforms_to_listener_api` state-machine test ────────────────
//
// Mirrors the Hypothesis state machine that stress-tests the listener
// contract: every save/delete/move that changes state fires exactly one
// listener event per registered listener. Hypothesis uses
// `Bundle("keys")` / `Bundle("values")` + `@precondition`; hegel-rust's
// `#[state_machine]` macro doesn't wire those in, so we implement
// `StateMachine` manually with `Variables<T>` as the bundle analogue and
// `tc.assume(...)` at the top of gated rules.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ExpectedEvent {
    Save(Vec<u8>, Vec<u8>),
    Delete(Vec<u8>, Option<Vec<u8>>),
}

fn to_expected(ev: &ListenerEvent) -> ExpectedEvent {
    match ev {
        ListenerEvent::Save { key, value } => ExpectedEvent::Save(key.clone(), value.clone()),
        ListenerEvent::Delete { key, value } => ExpectedEvent::Delete(key.clone(), value.clone()),
    }
}

fn event_counts(events: &[ExpectedEvent]) -> HashMap<ExpectedEvent, usize> {
    let mut out = HashMap::new();
    for ev in events {
        *out.entry(ev.clone()).or_insert(0) += 1;
    }
    out
}

struct DatabaseListenerMachine<D: ExampleDatabase> {
    db: D,
    keys: Variables<Vec<u8>>,
    values: Variables<Vec<u8>>,
    expected: Vec<ExpectedEvent>,
    actual: Arc<Mutex<Vec<ListenerEvent>>>,
    listener: Listener,
    active: bool,
    flush: Box<dyn Fn(&D)>,
}

impl<D: ExampleDatabase> DatabaseListenerMachine<D> {
    fn new(tc: &TestCase, db: D, flush: Box<dyn Fn(&D)>) -> Self {
        let actual = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&actual);
        let listener: Listener = Arc::new(move |ev: &ListenerEvent| {
            sink.lock().unwrap().push(ev.clone());
        });
        db.add_listener(Arc::clone(&listener));
        Self {
            db,
            keys: variables(tc),
            values: variables(tc),
            expected: Vec::new(),
            actual,
            listener,
            active: true,
            flush,
        }
    }

    fn expect_save(&mut self, k: Vec<u8>, v: Vec<u8>) {
        if self.active {
            self.expected.push(ExpectedEvent::Save(k, v));
        }
    }

    fn expect_delete(&mut self, k: Vec<u8>, v: Vec<u8>) {
        if self.active {
            self.expected.push(ExpectedEvent::Delete(k, Some(v)));
        }
    }

    fn rule_add_key(&mut self, tc: TestCase) {
        let k: Vec<u8> = tc.draw(gs::binary());
        self.keys.add(k);
    }

    fn rule_add_value(&mut self, tc: TestCase) {
        let v: Vec<u8> = tc.draw(gs::binary());
        self.values.add(v);
    }

    fn rule_add_listener(&mut self, tc: TestCase) {
        tc.assume(!self.active);
        self.db.add_listener(Arc::clone(&self.listener));
        self.active = true;
    }

    fn rule_remove_listener(&mut self, tc: TestCase) {
        tc.assume(self.active);
        self.db.remove_listener(&self.listener);
        self.active = false;
    }

    fn rule_clear_listeners(&mut self, _tc: TestCase) {
        self.db.clear_listeners();
        self.active = false;
    }

    fn rule_fetch(&mut self, _tc: TestCase) {
        let k = self.keys.draw().clone();
        let _ = self.db.fetch(&k);
    }

    fn rule_save(&mut self, _tc: TestCase) {
        let k = self.keys.draw().clone();
        let v = self.values.draw().clone();
        let changed = !self.db.fetch(&k).iter().any(|e| e == &v);
        self.db.save(&k, &v);
        if changed {
            self.expect_save(k, v);
        }
    }

    fn rule_delete(&mut self, _tc: TestCase) {
        let k = self.keys.draw().clone();
        let v = self.values.draw().clone();
        let changed = self.db.fetch(&k).iter().any(|e| e == &v);
        self.db.delete(&k, &v);
        if changed {
            self.expect_delete(k, v);
        }
    }

    fn rule_move(&mut self, _tc: TestCase) {
        let k1 = self.keys.draw().clone();
        let k2 = self.keys.draw().clone();
        let v = self.values.draw().clone();
        let in_k1 = self.db.fetch(&k1).iter().any(|e| e == &v);
        let save_changed = !self.db.fetch(&k2).iter().any(|e| e == &v);
        let delete_changed = k1 != k2 && in_k1;
        self.db.move_value(&k1, &k2, &v);
        if delete_changed {
            self.expect_delete(k1, v.clone());
        }
        if save_changed {
            self.expect_save(k2, v);
        }
    }

    fn invariant_events_agree(&mut self, _tc: TestCase) {
        (self.flush)(&self.db);
        let actual_raw = self.actual.lock().unwrap();
        let actual: Vec<ExpectedEvent> = actual_raw.iter().map(to_expected).collect();
        drop(actual_raw);
        assert_eq!(
            event_counts(&self.expected),
            event_counts(&actual),
            "listener events diverged from contract:\n  expected={:?}\n  actual={:?}",
            self.expected,
            actual,
        );
    }
}

impl<D: ExampleDatabase> StateMachine for DatabaseListenerMachine<D> {
    fn rules(&self) -> Vec<Rule<Self>> {
        vec![
            Rule::new("add_key", Self::rule_add_key),
            Rule::new("add_value", Self::rule_add_value),
            Rule::new("add_listener", Self::rule_add_listener),
            Rule::new("remove_listener", Self::rule_remove_listener),
            Rule::new("clear_listeners", Self::rule_clear_listeners),
            Rule::new("fetch", Self::rule_fetch),
            Rule::new("save", Self::rule_save),
            Rule::new("delete", Self::rule_delete),
            Rule::new("move", Self::rule_move),
        ]
    }
    fn invariants(&self) -> Vec<Rule<Self>> {
        vec![Rule::new("events_agree", Self::invariant_events_agree)]
    }
}

#[test]
fn test_database_listener_memory() {
    Hegel::new(|tc: TestCase| {
        let db = InMemoryNativeDatabase::new();
        let machine = DatabaseListenerMachine::new(&tc, db, Box::new(|_db| {}));
        run_state_machine(machine, tc);
    })
    .settings(Settings::new().test_cases(5).database(None))
    .run();
}

#[test]
fn test_database_listener_background_write() {
    Hegel::new(|tc: TestCase| {
        let db = BackgroundWriteNativeDatabase::new(InMemoryNativeDatabase::new());
        let flush: Box<dyn Fn(&BackgroundWriteNativeDatabase)> = Box::new(|db| {
            let _ = db.fetch(b"");
        });
        let machine = DatabaseListenerMachine::new(&tc, db, flush);
        run_state_machine(machine, tc);
    })
    .settings(Settings::new().test_cases(5).database(None))
    .run();
}

// ── test_database_equal / test_database_not_equal ──────────────────────────
//
// Upstream is a pytest-parametrized pair over (db1, db2) pairs. Rust's
// type system doesn't allow `==`/`!=` between values of different
// concrete types, so the cross-type cases from the Python file
// (`InMemoryExampleDatabase() != DirectoryBasedExampleDatabase("a")`,
// `BackgroundWriteDatabase(...) != InMemoryExampleDatabase()`) are not
// expressible and are omitted. `GitHubArtifactDatabase` has no Rust
// analog and is skipped. Each remaining (same-type) pair becomes an
// assertion below.

#[test]
fn test_database_equal() {
    // DirectoryBasedExampleDatabase("a") == DirectoryBasedExampleDatabase("a")
    assert!(NativeDatabase::new("a") == NativeDatabase::new("a"));

    // MultiplexedDatabase(Directory("a"), Directory("b")) == same, structurally.
    let m1 = MultiplexedNativeDatabase::new(vec![
        Arc::new(NativeDatabase::new("a")) as Arc<dyn ExampleDatabase>,
        Arc::new(NativeDatabase::new("b")) as Arc<dyn ExampleDatabase>,
    ]);
    let m2 = MultiplexedNativeDatabase::new(vec![
        Arc::new(NativeDatabase::new("a")) as Arc<dyn ExampleDatabase>,
        Arc::new(NativeDatabase::new("b")) as Arc<dyn ExampleDatabase>,
    ]);
    assert!(m1 == m2);

    // ReadOnlyDatabase(Directory("a")) == ReadOnlyDatabase(Directory("a"))
    let r1 = ReadOnlyNativeDatabase::new(NativeDatabase::new("a"));
    let r2 = ReadOnlyNativeDatabase::new(NativeDatabase::new("a"));
    assert!(r1 == r2);
}

#[test]
fn test_database_not_equal() {
    // Two InMemoryExampleDatabase() instances have distinct backing
    // dicts, so Python's `self.data is other.data` is false. Rust mirrors
    // that with pointer equality on the database object.
    let a = InMemoryNativeDatabase::new();
    let b = InMemoryNativeDatabase::new();
    assert!(a != b);

    // DirectoryBasedExampleDatabase("a") != DirectoryBasedExampleDatabase("b")
    assert!(NativeDatabase::new("a") != NativeDatabase::new("b"));

    // ReadOnlyDatabase(Directory("a")) != ReadOnlyDatabase(Directory("b"))
    let r1 = ReadOnlyNativeDatabase::new(NativeDatabase::new("a"));
    let r2 = ReadOnlyNativeDatabase::new(NativeDatabase::new("b"));
    assert!(r1 != r2);
}

// Direct exercise of the trait-level `db_eq` for paths that `PartialEq`
// can't reach: the default impl (used by types like `TracksListens` that
// define no equality relation) and the cross-type miss branch in
// `MultiplexedNativeDatabase::db_eq` (hit when `other` dispatches through
// `&dyn ExampleDatabase` to a non-Multiplexed concrete type).
#[test]
fn test_db_eq_default_and_cross_type() {
    let tracks = TracksListens::new();
    assert!(!tracks.db_eq(&tracks));

    let multi = MultiplexedNativeDatabase::new(vec![
        Arc::new(NativeDatabase::new("a")) as Arc<dyn ExampleDatabase>
    ]);
    let native = NativeDatabase::new("a");
    assert!(!multi.db_eq(&native));
}

// Beyond upstream: the Python test parametrizes
// `BackgroundWriteDatabase(InMemoryExampleDatabase()) != InMemoryExampleDatabase()`,
// which is a cross-type comparison that Rust's type system disallows.
// Cover the same-type side of `BackgroundWriteDatabase.__eq__` (structural
// through the wrapped database) instead.
#[test]
fn test_background_write_database_equality() {
    let inner = Arc::new(InMemoryNativeDatabase::new());
    let bg_a = BackgroundWriteNativeDatabase::new(Arc::clone(&inner));
    let bg_b = BackgroundWriteNativeDatabase::new(Arc::clone(&inner));
    // Same inner InMemory instance → structurally equal.
    assert!(bg_a == bg_b);

    let bg_c = BackgroundWriteNativeDatabase::new(InMemoryNativeDatabase::new());
    let bg_d = BackgroundWriteNativeDatabase::new(InMemoryNativeDatabase::new());
    // Distinct inner InMemory instances → not equal.
    assert!(bg_c != bg_d);
}

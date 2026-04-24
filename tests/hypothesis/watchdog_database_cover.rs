//! Ported from hypothesis-python/tests/watchdog/test_database_cover.py
//!
//! Trivial covering tests that exercise add_listener/remove_listener on
//! `MultiplexedDatabase`, `InMemoryExampleDatabase`, and
//! `DirectoryBasedExampleDatabase`. Native-gated because the database types
//! live under `src/native/database.rs` and are exposed only in native mode.
//!
//! The upstream `skipif_threading` guard (free-threaded Python only) has
//! no hegel-rust counterpart and is elided.
#![cfg(feature = "native")]

use hegel::__native_test_internals::{
    ExampleDatabase, InMemoryNativeDatabase, Listener, ListenerEvent, MultiplexedNativeDatabase,
    NativeDatabase,
};
use std::sync::Arc;

#[test]
fn test_start_stop_multiplexed_listener() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = MultiplexedNativeDatabase::new(vec![
        Arc::new(InMemoryNativeDatabase::new()),
        Arc::new(NativeDatabase::new(tmp.path().to_str().unwrap())),
    ]);
    let listener: Listener = Arc::new(|_: &ListenerEvent| {});
    db.add_listener(Arc::clone(&listener));
    db.remove_listener(&listener);
}

#[test]
fn test_start_stop_directory_listener() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = NativeDatabase::new(tmp.path().to_str().unwrap());
    let listener: Listener = Arc::new(|_: &ListenerEvent| {});
    db.add_listener(Arc::clone(&listener));
    db.remove_listener(&listener);
}

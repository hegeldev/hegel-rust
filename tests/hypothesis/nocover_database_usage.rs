//! Ported from hypothesis-python/tests/nocover/test_database_usage.py
//!
//! Only `test_database_not_created_when_not_used` ports at the public
//! surface — and only natively, since `NativeDatabase` is exposed via
//! `__native_test_internals`. The other six tests in the file drive
//! Python-specific or `find()`-surface behaviour with no Rust analog;
//! they are listed below and under `SKIPPED.md`.
//!
//! Individually-skipped tests:
//!
//! - `test_saves_incremental_steps_in_database`,
//!   `test_clears_out_database_as_things_get_boring`,
//!   `test_trashes_invalid_examples`,
//!   `test_respects_max_examples_in_database_usage` — all drive
//!   `find(strategy, predicate, settings=settings(database=...),
//!   database_key=b"...")` and assert on what `InMemoryExampleDatabase`
//!   accumulates across the search. hegel-rust has no `find()` public
//!   API (same gap noted by the `test_core.py::test_no_such_example`
//!   and `test_verbosity.py::test_prints_initial_attempts_on_find`
//!   skips), so the predicate-driven incremental-save / invalid-trash
//!   / max-examples behaviour these tests pin down isn't reachable
//!   from the Rust runner surface.
//!
//! - `test_does_not_use_database_when_seed_is_forced` — uses pytest's
//!   `monkeypatch` fixture to set `hypothesis.core.global_force_seed`
//!   (a Python module-level global) and then overrides
//!   `database.fetch = None` via dunder-attribute assignment to assert
//!   `fetch` was not called. Both facilities are Python-specific:
//!   hegel-rust has no `global_force_seed` equivalent (seeds go through
//!   `Settings::new().seed(Some(n))`) and no runtime-attribute-
//!   assignment surface on `NativeDatabase`.
//!
//! - `test_ga_database_not_created_when_not_used` — constructs
//!   `ReadOnlyDatabase(GitHubArtifactDatabase("mock", "mock", path=path))`.
//!   `GitHubArtifactDatabase` has no Rust counterpart (same gap
//!   documented for the `test_database_backend.py` `test_ga_*` skips).

#![cfg(feature = "native")]

use hegel::TestCase;
use hegel::__native_test_internals::{ExampleDatabase, NativeDatabase};
use hegel::generators as gs;
use hegel::{Hegel, Settings};

#[test]
fn test_database_not_created_when_not_used() {
    Hegel::new(|tc: TestCase| {
        let key: Vec<u8> = tc.draw(gs::binary());
        let value: Vec<u8> = tc.draw(gs::binary());
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("examples");
        assert!(!path.exists());
        let db = NativeDatabase::new(path.to_str().unwrap());
        assert!(db.fetch(&key).is_empty());
        assert!(!path.exists());
        db.save(&key, &value);
        assert!(path.exists());
        assert_eq!(db.fetch(&key), vec![value]);
    })
    .settings(Settings::new().test_cases(50).database(None))
    .run();
}

//! Ported from hypothesis-python/tests/nocover/test_database_usage.py
//!
//! Only `test_database_not_created_when_not_used` ports — natively,
//! since `NativeDatabase` is exposed via `__native_test_internals`.
//! The other six tests in the file all turn on engine database
//! accumulation behaviour that hegel-rust's native runner doesn't
//! produce: the upstream `find()` driver saves every distinct
//! interesting example reached during search and shrinking, plus
//! pareto-front entries; `NativeConjectureRunner::run()` only mutates
//! the database via the reuse phase (which deletes invalid entries
//! and replays existing ones), never auto-saving during generation
//! or shrinking, and `pareto_front()` is `todo!()`.
//!
//! Individually-skipped tests:
//!
//! - `test_saves_incremental_steps_in_database`,
//!   `test_clears_out_database_as_things_get_boring`,
//!   `test_trashes_invalid_examples` — assert on `all_values(db)` /
//!   `non_covering_examples(db)` accumulating multiple distinct
//!   entries (or shrinking back to zero) over the course of one or
//!   more `find()` runs. Even at the `NativeConjectureRunner` surface
//!   the engine doesn't save during generation/shrinking, so the
//!   accumulation these tests pin down isn't observable. They become
//!   portable once the native engine grows the auto-save side of
//!   `pareto_front` / interesting-example saves.
//!
//! - `test_respects_max_examples_in_database_usage` — counts
//!   predicate invocations against `max_examples=10`. Falls under the
//!   documented `find()` + predicate-call-count skip in
//!   `porting-tests/references/api-mapping.md`: native re-enters the
//!   test function for span-mutation attempts, so the predicate-call
//!   shape Python's `find()` pins down isn't reproducible.
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

use hegel::__native_test_internals::{ExampleDatabase, NativeDatabase};
use hegel::TestCase;
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

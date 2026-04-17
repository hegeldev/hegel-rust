//! Regression test for the shared cargo target-dir cache's sweep step.
//!
//! `TempRustProject` caches cargo build output per test binary in a directory
//! under `std::env::temp_dir()` keyed by the test executable's hash. A binary
//! only cleans its own hash bucket on startup, so buckets for binaries that
//! stop running would accumulate forever — each ~300–800 MB. On containers
//! with a small tmpfs this fills the disk and crashes anything that writes
//! to it (e.g. sqlite3 "database or disk is full").
//!
//! The sweep step runs before a binary creates its own bucket: it removes
//! every sibling `hegel-test-cargo-target-*` directory whose mtime is older
//! than a threshold (no live test binary touches its bucket less often than
//! that), leaving our own keep-path alone.

mod common;

use std::fs::{self, File, FileTimes};
use std::time::{Duration, SystemTime};

use common::project::sweep_stale_cargo_target_dirs;

fn set_mtime(path: &std::path::Path, mtime: SystemTime) {
    let times = FileTimes::new().set_modified(mtime);
    File::open(path).unwrap().set_times(times).unwrap();
}

#[test]
fn sweep_removes_old_sibling_target_dirs_and_preserves_fresh_and_keep() {
    let root = tempfile::tempdir().unwrap();
    let keep = root.path().join("hegel-test-cargo-target-aaaaaaaaaaaaaaaa");
    let stale = root.path().join("hegel-test-cargo-target-bbbbbbbbbbbbbbbb");
    let fresh = root.path().join("hegel-test-cargo-target-cccccccccccccccc");
    let unrelated = root.path().join("some-other-cache");
    for d in [&keep, &stale, &fresh, &unrelated] {
        fs::create_dir(d).unwrap();
    }
    // Drop a file inside each dir so we can confirm the whole tree is
    // removed, not just the top-level entry.
    fs::write(stale.join("marker"), b"x").unwrap();
    fs::write(fresh.join("marker"), b"x").unwrap();

    let threshold = Duration::from_secs(3600);
    let old = SystemTime::now() - Duration::from_secs(2 * 3600);
    set_mtime(&stale, old);

    sweep_stale_cargo_target_dirs(root.path(), &keep, threshold);

    assert!(keep.exists(), "keep dir must not be removed");
    assert!(!stale.exists(), "stale sibling should be swept");
    assert!(fresh.exists(), "fresh sibling should remain");
    assert!(unrelated.exists(), "non-matching dir should be untouched");
}

#[test]
fn sweep_is_safe_when_root_does_not_exist() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("does-not-exist");
    // Must not panic.
    sweep_stale_cargo_target_dirs(&missing, &missing.join("keep"), Duration::from_secs(60));
}

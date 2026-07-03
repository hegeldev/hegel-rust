//! Tests for the stale-artifact sweep that keeps the shared
//! `TempRustProject` target directory (`target/tmp/hegel-shared-target`)
//! from growing without bound: temp crate names embed the owning test
//! binary's PID, so artifacts from processes that no longer exist can be
//! reclaimed.

mod common;

use common::project::{pid_is_live, sweep_stale_temp_artifacts, temp_crate_pid};
use std::fs;
use std::path::Path;

#[test]
fn temp_crate_pid_parses_artifact_names() {
    assert_eq!(temp_crate_pid("temp_hegel_test_1234_0"), Some(1234));
    assert_eq!(temp_crate_pid("temp_hegel_test_1234_0.d"), Some(1234));
    assert_eq!(
        temp_crate_pid("temp_hegel_test_1234_10-8e544c98d66ada3e"),
        Some(1234)
    );
    assert_eq!(temp_crate_pid("temp_hegel_test_1234_0.exe"), Some(1234));
    assert_eq!(temp_crate_pid("libhegel-9d511e4f2cc47b64.rlib"), None);
    assert_eq!(temp_crate_pid("hegel_warmup-0a1b2c3d"), None);
    assert_eq!(temp_crate_pid("temp_hegel_test_"), None);
    assert_eq!(temp_crate_pid("temp_hegel_test_notapid_0"), None);
}

fn touch(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"x").unwrap();
}

fn dir_with_file(path: &Path) {
    fs::create_dir_all(path).unwrap();
    fs::write(path.join("data"), b"x").unwrap();
}

#[test]
fn sweep_removes_dead_pid_artifacts_and_keeps_everything_else() {
    let tmp = tempfile::TempDir::new().unwrap();
    let target = tmp.path();
    let dead = "temp_hegel_test_1234_0";
    let live = "temp_hegel_test_4242_1";
    let own = format!("temp_hegel_test_{}_2", std::process::id());

    let dead_artifacts = [
        target.join(format!("debug/{dead}")),
        target.join(format!("debug/{dead}.d")),
        target.join(format!("debug/deps/{dead}-abc123")),
        target.join(format!("debug/deps/{dead}-abc123.d")),
    ];
    for path in &dead_artifacts {
        touch(path);
    }
    let dead_dirs = [
        target.join(format!("debug/incremental/{dead}-xyz789")),
        target.join(format!("debug/.fingerprint/{dead}-abc123")),
    ];
    for path in &dead_dirs {
        dir_with_file(path);
    }

    let kept = [
        target.join(format!("debug/deps/{live}-abc123")),
        target.join(format!("debug/deps/{own}-abc123")),
        target.join("debug/deps/libhegel-9d511e4f2cc47b64.rlib"),
        target.join("debug/hegel_warmup.d"),
        target.join("warmup/src/lib.rs"),
    ];
    for path in &kept {
        touch(path);
    }
    let kept_dirs = [
        target.join("debug/incremental/hegeltest-3vt0bpp1ncnzx"),
        target.join(format!("debug/incremental/{live}-def456")),
    ];
    for path in &kept_dirs {
        dir_with_file(path);
    }

    // `is_live` claims only 4242 is running — including denying our own PID,
    // which the sweep must protect regardless of the liveness callback.
    sweep_stale_temp_artifacts(target, &|pid| pid == 4242);

    for path in dead_artifacts.iter().chain(&dead_dirs) {
        assert!(!path.exists(), "should have been swept: {}", path.display());
    }
    for path in kept.iter().chain(&kept_dirs) {
        assert!(path.exists(), "should have been kept: {}", path.display());
    }
}

#[test]
fn sweep_of_a_missing_target_dir_is_a_no_op() {
    let tmp = tempfile::TempDir::new().unwrap();
    sweep_stale_temp_artifacts(&tmp.path().join("does-not-exist"), &|_| false);
}

#[test]
fn own_pid_is_live() {
    assert!(pid_is_live(std::process::id()));
}

#[cfg(target_os = "linux")]
#[test]
fn nonexistent_pid_is_not_live() {
    // Far above any real pid_max, so no such process can exist.
    assert!(!pid_is_live(3_999_999_999));
}

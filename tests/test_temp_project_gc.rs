//! Tests for the garbage collection that keeps temp-project testing from
//! leaking disk space: temp crate names and scratch directory names embed
//! the owning test binary's PID, so artifacts in the shared target
//! directory (`target/tmp/hegel-shared-target`) and scratch dirs in the
//! system temp dir left behind by processes that no longer exist can be
//! reclaimed, and each `TempRustProject` removes its own shared-target
//! artifacts on drop.

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
    let tmp = crate::common::project::scratch_tempdir();
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
fn temp_project_dir_name_embeds_owning_pid() {
    let project = common::project::TempRustProject::new();
    let dir_name = project
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        dir_name.starts_with(&format!("hegel_rust_tmp_{}_", std::process::id())),
        "temp project dir {dir_name:?} should embed the owning PID so that \
         dirs left behind by killed runs can be attributed and swept"
    );
    assert_eq!(
        project.path().parent().unwrap(),
        std::env::temp_dir(),
        "temp project dirs should live directly in the system temp dir"
    );
}

#[test]
fn dropping_a_project_removes_its_artifacts_from_the_shared_target() {
    let project = common::project::TempRustProject::new();
    let crate_name = project.crate_name().to_owned();
    let target = common::project::shared_target_dir();

    // Plant the artifacts a `cargo run` of this crate would leave behind.
    let artifacts = [
        target.join(format!("debug/{crate_name}")),
        target.join(format!("debug/{crate_name}.d")),
        target.join(format!("debug/deps/{crate_name}-abc123")),
        target.join(format!("debug/deps/{crate_name}-abc123.d")),
    ];
    for path in &artifacts {
        touch(path);
    }
    let artifact_dirs = [
        target.join(format!("debug/incremental/{crate_name}-xyz789")),
        target.join(format!("debug/.fingerprint/{crate_name}-abc123")),
    ];
    for path in &artifact_dirs {
        dir_with_file(path);
    }

    // An artifact of a *different* temp crate whose name shares this one as a
    // prefix must survive the drop.
    let unrelated = target.join(format!("debug/deps/{crate_name}9-abc123"));
    touch(&unrelated);

    drop(project);

    for path in artifacts.iter().chain(&artifact_dirs) {
        assert!(
            !path.exists(),
            "dropping the project should remove its shared-target artifact: {}",
            path.display()
        );
    }
    assert!(unrelated.exists(), "prefix-sharing crate must be kept");
    fs::remove_file(&unrelated).unwrap();
}

#[test]
fn scratch_dir_pid_parses_scratch_dir_names() {
    use common::project::scratch_dir_pid;
    assert_eq!(scratch_dir_pid("hegel_rust_tmp_1234_Xy1Z9a"), Some(1234));
    assert_eq!(scratch_dir_pid("hegel_rust_tmp_1_a"), Some(1));
    assert_eq!(scratch_dir_pid(".tmpAbCdEf"), None);
    assert_eq!(scratch_dir_pid("hegel_rust_tmp_"), None);
    assert_eq!(scratch_dir_pid("hegel_rust_tmp_1234"), None);
    assert_eq!(scratch_dir_pid("hegel_rust_tmp_notapid_x"), None);
    assert_eq!(scratch_dir_pid("hegel-rust-test-Xy1Z9a"), None);
}

#[test]
fn sweep_removes_dead_scratch_dirs_and_keeps_everything_else() {
    use common::project::sweep_stale_scratch_dirs;

    let tmp = crate::common::project::scratch_tempdir();
    let parent = tmp.path();

    let dead = parent.join("hegel_rust_tmp_1234_aaaaaa");
    dir_with_file(&dead);

    let kept_dirs = [
        // A scratch dir whose owner is still running.
        parent.join("hegel_rust_tmp_4242_bbbbbb"),
        // Our own scratch dir is protected even when `is_live` denies our PID.
        parent.join(format!("hegel_rust_tmp_{}_cccccc", std::process::id())),
        // Not a scratch dir: someone else's temp dir.
        parent.join(".tmpAbCdEf"),
    ];
    for path in &kept_dirs {
        dir_with_file(path);
    }
    // A plain file that happens to share the naming scheme is not a scratch
    // dir and must not be touched.
    let kept_file = parent.join("hegel_rust_tmp_1234_ffffff");
    touch(&kept_file);

    sweep_stale_scratch_dirs(parent, &|pid| pid == 4242);

    assert!(!dead.exists(), "dead scratch dir should have been swept");
    for path in kept_dirs.iter().chain([&kept_file]) {
        assert!(path.exists(), "should have been kept: {}", path.display());
    }
}

#[test]
fn sweep_of_a_missing_target_dir_is_a_no_op() {
    let tmp = crate::common::project::scratch_tempdir();
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

use super::*;

#[test]
#[cfg(unix)]
#[should_panic(expected = "not executable")]
fn test_validate_executable_panics_for_non_executable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("not_exec");
    std::fs::write(&path, "").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    validate_executable(path.to_str().unwrap());
}

#[test]
fn test_is_hegel_file() {
    // absolute path within hegel crate src/ returns true
    assert!(is_hegel_file(&format!("{}/src/runner.rs", HEGEL_CRATE_DIR)));

    // relative path under src/ that exists returns true
    assert!(is_hegel_file("src/runner.rs"));
    assert!(is_hegel_file("src/server/utils.rs"));

    // absolute path outside hegel crate returns false
    assert!(!is_hegel_file("/tmp/user_project/src/main.rs"));
    // doesn't return true on a dir that happens to share a prefix
    assert!(!is_hegel_file(&format!(
        "{}-extra/src/lib.rs",
        HEGEL_CRATE_DIR
    )));

    // relative path that doesn't exist under the crate root returns false
    assert!(!is_hegel_file("src/nonexistent_file.rs"));
    assert!(!is_hegel_file("user_code/main.rs"));

    // test files are NOT hegel-internal, even though they exist in the crate
    assert!(!is_hegel_file("tests/common/utils.rs"));
    assert!(!is_hegel_file(&format!(
        "{}/tests/common/utils.rs",
        HEGEL_CRATE_DIR
    )));

    // paths with ".." that resolve to tests/ are NOT hegel-internal
    assert!(!is_hegel_file("tests/test_find_quality/../common/utils.rs"));

    // paths with "./" current-dir component are normalized correctly
    assert!(is_hegel_file("./src/runner.rs"));
}

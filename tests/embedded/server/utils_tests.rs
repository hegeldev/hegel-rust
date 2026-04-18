use super::*;

#[test]
#[cfg(unix)]
#[should_panic(expected = "not executable")]
fn test_validate_executable_panics_for_non_executable() {
    let dir = std::env::temp_dir().join("hegel_test_unit_exec");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("not_exec");
    std::fs::write(&path, "").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    validate_executable(path.to_str().unwrap());
}

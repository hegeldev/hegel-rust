use super::*;

#[test]
fn test_cache_dir_with_xdg() {
    let result = cache_dir_from(Some("/tmp/xdg".to_string()), None);
    assert_eq!(result, PathBuf::from("/tmp/xdg/hegel"));
}

#[test]
fn test_cache_dir_with_home() {
    let result = cache_dir_from(None, Some(PathBuf::from("/home/test")));
    assert_eq!(result, PathBuf::from("/home/test/.cache/hegel"));
}

#[test]
fn test_find_in_path_finds_known_binary() {
    assert!(find_in_path("sh").is_some());
}

#[test]
fn test_find_in_path_returns_none_for_missing() {
    assert!(find_in_path("definitely_not_a_real_binary_xyz").is_none());
}

#[test]
fn test_find_uv_impl_uses_path_uv_when_available() {
    let temp = tempfile::tempdir().unwrap();
    let fake_uv = temp.path().join("uv");
    std::fs::write(&fake_uv, b"fake uv").unwrap();
    let result = find_uv_impl(Some(fake_uv.clone()), PathBuf::from("/nonexistent"));
    assert_eq!(result, fake_uv.to_string_lossy());
}

#[test]
fn test_find_uv_impl_returns_cached_when_not_in_path() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().to_path_buf();
    let fake_uv = cache.join("uv");
    std::fs::write(&fake_uv, b"fake uv").unwrap();
    let result = find_uv_impl(None, cache);
    assert_eq!(result, fake_uv.to_string_lossy());
}

#[test]
#[should_panic(expected = "Failed to run uv installer")]
fn test_install_uv_fails_with_bad_sh_command() {
    let temp = tempfile::tempdir().unwrap();
    install_uv_with_sh(temp.path(), "definitely_not_a_real_shell_xyz");
}

/// Integration test: exercises the full install path using the embedded
/// installer script. Requires network access to download uv from GitHub.
#[test]
fn test_find_uv_impl_installs_when_missing() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().to_path_buf();
    let cached_uv = cache.join("uv");

    let result = find_uv_impl(None, cache);

    assert!(cached_uv.is_file());
    assert_eq!(result, cached_uv.to_string_lossy());
}

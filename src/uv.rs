use std::io::Write;
use std::path::{Path, PathBuf};

const UV_INSTALLER: &str = include_str!("uv-install.sh");

/// Returns the path to a `uv` binary.
///
/// Lookup order:
/// 1. `uv` found on `PATH`
/// 2. Cached binary at `~/.cache/hegel/uv`
/// 3. Installs uv to `~/.cache/hegel/uv` using the embedded installer script
///
/// Panics if uv cannot be found or installed.
pub fn find_uv() -> String {
    find_uv_impl(
        find_in_path("uv"),
        cache_dir_from(std::env::var("XDG_CACHE_HOME").ok(), std::env::home_dir()),
    )
}

fn find_uv_impl(uv_in_path: Option<PathBuf>, cache: PathBuf) -> String {
    if let Some(path) = uv_in_path {
        return path.to_string_lossy().into_owned();
    }
    let cached = cache.join("uv");
    if cached.is_file() {
        return cached.to_string_lossy().into_owned();
    }
    install_uv_to(&cache);
    cached.to_string_lossy().into_owned()
}

fn install_uv_to(cache: &Path) {
    install_uv_with_sh(cache, "sh")
}

fn install_uv_with_sh(cache: &Path, sh: &str) {
    std::fs::create_dir_all(cache)
        .unwrap_or_else(|e| panic!("Failed to create cache directory {}: {e}", cache.display()));
    let mut child = std::process::Command::new(sh)
        .stdin(std::process::Stdio::piped())
        .env("UV_UNMANAGED_INSTALL", cache)
        .spawn()
        .unwrap_or_else(|e| {
            panic!(
                "Failed to run uv installer: {e}. \
             Install uv manually: https://docs.astral.sh/uv/getting-started/installation/"
            )
        });
    child
        .stdin
        .take()
        .unwrap()
        .write_all(UV_INSTALLER.as_bytes())
        .unwrap_or_else(|e| panic!("Failed to write to uv installer stdin: {e}"));
    let status = child
        .wait()
        .unwrap_or_else(|e| panic!("Failed to wait for uv installer: {e}"));
    assert!(
        status.success(),
        "uv installer failed. \
         Install uv manually: https://docs.astral.sh/uv/getting-started/installation/"
    );
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

fn cache_dir_from(xdg_cache_home: Option<String>, home_dir: Option<PathBuf>) -> PathBuf {
    if let Some(xdg_cache) = xdg_cache_home {
        return PathBuf::from(xdg_cache).join("hegel");
    }
    let home = home_dir.expect("Could not determine home directory");
    home.join(".cache").join("hegel")
}

#[cfg(test)]
mod tests {
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
}

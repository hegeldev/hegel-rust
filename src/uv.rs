#[cfg(unix)]
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
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
        cache_dir_from(std::env::var("XDG_CACHE_HOME").ok(), home_dir()),
    )
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        #[allow(deprecated)]
        std::env::home_dir()
    }
}

fn find_uv_impl(uv_in_path: Option<PathBuf>, cache: PathBuf) -> String {
    if let Some(path) = uv_in_path {
        return path.to_string_lossy().into_owned();
    }
    let cached = cache.join(UV_BINARY_NAME);
    if cached.is_file() {
        return cached.to_string_lossy().into_owned();
    }
    install_uv_to(&cache);
    cached.to_string_lossy().into_owned()
}

#[cfg(windows)]
const UV_BINARY_NAME: &str = "uv.exe";

#[cfg(not(windows))]
const UV_BINARY_NAME: &str = "uv";

#[cfg(unix)]
fn install_uv_to(cache: &Path) {
    install_uv_with_sh(cache, "sh")
}

#[cfg(windows)]
fn install_uv_to(_cache: &Path) {
    panic!(
        "uv is required but was not found on PATH. \
         Install uv: https://docs.astral.sh/uv/getting-started/installation/"
    );
}

#[cfg(unix)]
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
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        for ext in crate::utils::executable_extensions() {
            let with_ext = dir.join(format!("{name}{ext}"));
            if with_ext.is_file() {
                return Some(with_ext);
            }
        }
    }
    None
}

fn cache_dir_from(xdg_cache_home: Option<String>, home_dir: Option<PathBuf>) -> PathBuf {
    if let Some(xdg_cache) = xdg_cache_home {
        return PathBuf::from(xdg_cache).join("hegel");
    }
    let home = home_dir.expect("Could not determine home directory");
    home.join(".cache").join("hegel")
}

#[cfg(test)]
#[path = "../tests/embedded/uv_tests.rs"]
mod tests;

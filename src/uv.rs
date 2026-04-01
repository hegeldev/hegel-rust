use std::path::PathBuf;

const UV_VERSION: &str = "0.11.2";

/// Returns the path to a `uv` binary.
///
/// Lookup order:
/// 1. `uv` found on `PATH`
/// 2. Cached binary at `~/.cache/hegel/uv`
/// 3. Downloads uv to `~/.cache/hegel/uv` and returns that path
///
/// Panics if uv cannot be found or downloaded.
pub fn find_uv() -> String {
    if let Some(path) = find_in_path("uv") {
        return path.to_string_lossy().into_owned();
    }

    let cached = cached_uv_path();
    if cached.is_file() {
        return cached.to_string_lossy().into_owned();
    }

    download_uv().unwrap_or_else(|e| panic!("{e}"));
    cached.to_string_lossy().into_owned()
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

fn cache_dir() -> PathBuf {
    if let Ok(xdg_cache) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg_cache).join("hegel");
    }
    let home = std::env::var("HOME").expect("HOME environment variable not set");
    PathBuf::from(home).join(".cache").join("hegel")
}

fn cached_uv_path() -> PathBuf {
    cache_dir().join("uv")
}

fn platform_archive_name() -> Result<String, String> {
    let triple = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "linux") => "aarch64-unknown-linux-musl",
        ("x86_64", "linux") => "x86_64-unknown-linux-musl",
        (arch, os) => {
            return Err(format!(
                "Unsupported platform: {arch}-{os}. \
                 Install uv manually: https://docs.astral.sh/uv/getting-started/installation/"
            ))
        }
    };
    Ok(format!("uv-{triple}.tar.gz"))
}

fn download_uv() -> Result<(), String> {
    let archive_name = platform_archive_name()?;
    let url = format!(
        "https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{archive_name}"
    );

    let cache = cache_dir();
    std::fs::create_dir_all(&cache)
        .map_err(|e| format!("Failed to create cache directory {}: {e}", cache.display()))?;

    // Use a per-process temp directory inside the cache dir so that:
    // 1. Concurrent downloads don't interfere with each other
    // 2. The final rename is atomic (same filesystem)
    let temp_dir = cache.join(format!(".uv-download-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temp directory: {e}"))?;
    let _cleanup = CleanupGuard(&temp_dir);

    let archive_path = temp_dir.join(&archive_name);

    let output = std::process::Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&archive_path)
        .arg(&url)
        .output()
        .map_err(|e| {
            format!(
                "Failed to run curl to download uv: {e}. \
                 Install uv manually: https://docs.astral.sh/uv/getting-started/installation/"
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Failed to download uv from {url}: {stderr}\n\
             Install uv manually: https://docs.astral.sh/uv/getting-started/installation/"
        ));
    }

    let output = std::process::Command::new("tar")
        .args(["xzf"])
        .arg(&archive_path)
        .args(["--strip-components", "1", "-C"])
        .arg(&temp_dir)
        .output()
        .map_err(|e| format!("Failed to extract uv archive: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to extract uv archive: {stderr}"));
    }

    let extracted_uv = temp_dir.join("uv");

    // Atomic rename — safe under concurrent downloads because rename on the
    // same filesystem is atomic on Unix, so the last writer wins with a
    // valid binary.
    let final_path = cached_uv_path();
    std::fs::rename(&extracted_uv, &final_path)
        .map_err(|e| format!("Failed to install uv to {}: {e}", final_path.display()))?;

    Ok(())
}

/// RAII guard that removes a directory on drop.
struct CleanupGuard<'a>(&'a std::path::Path);

impl Drop for CleanupGuard<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0);
    }
}

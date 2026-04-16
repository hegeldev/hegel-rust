#![cfg(not(feature = "native"))]
mod common;

use common::project::TempRustProject;

// Unix-only: exercises the sh-based uv auto-install path and uses Unix PATH
// separators (:) and bare binary names. On Windows, uv must be pre-installed.
#[test]
#[cfg(unix)]
fn test_missing_uv_error_message() {
    // When uv is not on PATH, not cached, and curl/wget are also missing,
    // the user should get a helpful error about installing uv manually.
    let code = r#"
fn main() {
    // Filter uv, curl, and wget out of PATH so download can't proceed
    let path = std::env::var("PATH").unwrap_or_default();
    let filtered: String = path
        .split(':')
        .filter(|dir| {
            !std::path::Path::new(&format!("{dir}/uv")).exists()
            && !std::path::Path::new(&format!("{dir}/curl")).exists()
            && !std::path::Path::new(&format!("{dir}/wget")).exists()
        })
        .collect::<Vec<_>>()
        .join(":");
    std::env::set_var("PATH", &filtered);

    hegel::hegel(|tc| {
        let _ = tc.draw(hegel::generators::booleans());
    });
}
"#;

    let cache_dir = tempfile::tempdir().unwrap();
    TempRustProject::new()
        .main_file(code)
        .env_remove("HEGEL_SERVER_COMMAND")
        .env("XDG_CACHE_HOME", cache_dir.path().to_str().unwrap())
        .expect_failure("Install uv manually")
        .cargo_run(&[]);
}

#[test]
#[cfg(unix)]
fn test_downloads_uv_when_not_on_path() {
    // When uv is not on PATH and not cached, hegel should download uv
    // and use it to run hegel-core successfully.
    let code = r#"
fn main() {
    let path = std::env::var("PATH").unwrap_or_default();
    let filtered: String = path
        .split(':')
        .filter(|dir| !std::path::Path::new(&format!("{dir}/uv")).exists())
        .collect::<Vec<_>>()
        .join(":");
    std::env::set_var("PATH", &filtered);

    hegel::hegel(|tc| {
        let _ = tc.draw(hegel::generators::booleans());
    });

    // Verify uv was downloaded to the cache
    let cache_home = std::env::var("XDG_CACHE_HOME").unwrap();
    let cached_uv = format!("{cache_home}/hegel/uv");
    assert!(std::path::Path::new(&cached_uv).is_file(), "uv should be cached at {cached_uv}");
}
"#;

    let cache_dir = tempfile::tempdir().unwrap();
    TempRustProject::new()
        .main_file(code)
        .env_remove("HEGEL_SERVER_COMMAND")
        .env("XDG_CACHE_HOME", cache_dir.path().to_str().unwrap())
        .cargo_run(&[]);
}

mod common;

use common::project::TempRustProject;

#[test]
fn test_missing_uv_error_message() {
    // The test binary filters uv out of its own PATH at runtime,
    // so cargo can still compile it with the full PATH.
    let code = r#"
fn main() {
    // Filter uv out of PATH so ensure_hegel_installed can't find it
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
}
"#;

    // Point XDG_CACHE_HOME at an empty temp dir so no cached install is found.
    let empty_cache = tempfile::tempdir().unwrap();

    TempRustProject::new()
        .main_file(code)
        .env_remove("HEGEL_SERVER_COMMAND")
        .env("XDG_CACHE_HOME", empty_cache.path().to_str().unwrap())
        .expect_failure("You are seeing this error message because hegel-rust tried to use `uv` to install hegel-core")
        .cargo_run(&[]);
}

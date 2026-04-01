mod common;

use common::project::TempRustProject;

#[test]
fn test_missing_uv_error_message() {
    // When uv is not on PATH, not cached, and curl is also missing,
    // the user should get a helpful error about installing uv manually.
    let code = r#"
fn main() {
    // Filter uv and curl out of PATH so download can't proceed
    let path = std::env::var("PATH").unwrap_or_default();
    let filtered: String = path
        .split(':')
        .filter(|dir| {
            !std::path::Path::new(&format!("{dir}/uv")).exists()
            && !std::path::Path::new(&format!("{dir}/curl")).exists()
        })
        .collect::<Vec<_>>()
        .join(":");
    std::env::set_var("PATH", &filtered);

    hegel::hegel(|tc| {
        let _ = tc.draw(hegel::generators::booleans());
    });
}
"#;

    TempRustProject::new()
        .main_file(code)
        .env_remove("HEGEL_SERVER_COMMAND")
        // Point XDG_CACHE_HOME to a temp dir so no cached uv is found
        .env("XDG_CACHE_HOME", "/tmp/hegel-test-no-cache")
        .expect_failure("Install uv manually")
        .cargo_run(&[]);
}

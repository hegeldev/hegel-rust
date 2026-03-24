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

    // Also remove any cached install so it actually tries uv
    let _ = std::fs::remove_dir_all(".hegel");

    hegel::hegel(|tc| {
        let _ = tc.draw(hegel::generators::booleans());
    });
}
"#;

    TempRustProject::new()
        .main_file(code)
        .env_remove("HEGEL_SERVER_COMMAND")
        .expect_failure("You are seeing this error message because hegel-rust tried to use `uv` to install hegel-core")
        .cargo_run(&[]);
}

mod common;

use common::project::TempRustProject;

const HEGEL_CODE: &str = r#"
fn main() {
    hegel::hegel(|tc| {
        let _ = tc.draw(hegel::generators::booleans());
    });
}
"#;

#[test]
fn test_non_hegel_command_gives_informative_error() {
    TempRustProject::new()
        .main_file(HEGEL_CODE)
        .env("HEGEL_SERVER_COMMAND", "false")
        .expect_failure(
            "failed to start.*Ensure HEGEL_SERVER_COMMAND points to a valid hegel-core binary",
        )
        .cargo_run(&[]);
}

#[test]
fn test_wrong_version_hegel_gives_informative_error() {
    // Create a script that pretends to be an old hegel version
    let script_dir = std::env::temp_dir().join("hegel_test_fake_binary");
    std::fs::create_dir_all(&script_dir).unwrap();
    let script_path = script_dir.join("fake_hegel");

    std::fs::write(
        &script_path,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'hegel (version 0.1.0)'; exit 0; fi\nexit 1\n",
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    TempRustProject::new()
        .main_file(HEGEL_CODE)
        .env("HEGEL_SERVER_COMMAND", script_path.to_str().unwrap())
        .expect_failure(
            "(?i)possibly wrong hegel-core version.*expected 0\\.2\\.\\d+.*got 0\\.1\\.0",
        )
        .cargo_run(&[]);
}

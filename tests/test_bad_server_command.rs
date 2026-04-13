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
        .expect_failure("(?s)failed during startup.*Is this a hegel binary")
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
            "(?s)failed during startup.*Version mismatch.*expected 'hegel \\(version 0\\.4\\.\\d+\\)'.*got 'hegel \\(version 0\\.1\\.0\\)'",
        )
        .cargo_run(&[]);
}

#[test]
fn test_nonexistent_binary_gives_informative_error() {
    TempRustProject::new()
        .main_file(HEGEL_CODE)
        .env("HEGEL_SERVER_COMMAND", "/nonexistent/path/to/hegel")
        .expect_failure("not found at '/nonexistent/path/to/hegel'")
        .cargo_run(&[]);
}

#[test]
fn test_bare_name_not_on_path_gives_informative_error() {
    TempRustProject::new()
        .main_file(HEGEL_CODE)
        .env("HEGEL_SERVER_COMMAND", "definitely_not_a_real_hegel_binary")
        .expect_failure("not found on PATH")
        .cargo_run(&[]);
}

#[test]
#[cfg(unix)]
fn test_not_executable_gives_informative_error() {
    let dir = std::env::temp_dir().join("hegel_test_not_executable");
    std::fs::create_dir_all(&dir).unwrap();
    let script_path = dir.join("not_executable_hegel");
    std::fs::write(&script_path, "#!/bin/sh\nexit 0\n").unwrap();

    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o644)).unwrap();

    TempRustProject::new()
        .main_file(HEGEL_CODE)
        .env("HEGEL_SERVER_COMMAND", script_path.to_str().unwrap())
        .expect_failure("not executable.*Check file permissions")
        .cargo_run(&[]);
}

#[test]
#[cfg(unix)]
fn test_server_hangs_gives_bad_virtualenv_message() {
    // Script that closes stdout (so handshake fails) but stays alive
    let dir = std::env::temp_dir().join("hegel_test_hanging");
    std::fs::create_dir_all(&dir).unwrap();
    let script_path = dir.join("hanging_hegel");
    std::fs::write(&script_path, "#!/bin/sh\nexec 1>&-\nsleep 10\n").unwrap();

    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();

    TempRustProject::new()
        .main_file(HEGEL_CODE)
        .env("HEGEL_SERVER_COMMAND", script_path.to_str().unwrap())
        .expect_failure("(?s)failed during startup.*Possibly bad virtualenv")
        .cargo_run(&[]);
}

#[test]
#[cfg(unix)]
fn test_server_log_included_in_error() {
    let dir = std::env::temp_dir().join("hegel_test_stderr");
    std::fs::create_dir_all(&dir).unwrap();
    let script_path = dir.join("stderr_hegel");
    std::fs::write(
        &script_path,
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then echo 'hegel (version 0.1.0)'; exit 0; fi\n\
         echo 'Error: startup failed' >&2\n\
         echo 'Detail line 2' >&2\n\
         echo 'Detail line 3' >&2\n\
         echo 'Detail line 4' >&2\n\
         exit 1\n",
    )
    .unwrap();

    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();

    TempRustProject::new()
        .main_file(HEGEL_CODE)
        .env("HEGEL_SERVER_COMMAND", script_path.to_str().unwrap())
        .expect_failure("(?s)Server log.*Error: startup failed.*see .* for full output")
        .cargo_run(&[]);
}

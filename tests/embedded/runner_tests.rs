use super::*;

fn exit_failure_status() -> std::process::ExitStatus {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;
    std::process::ExitStatus::from_raw(1)
}

fn spawn_exit_0() -> std::process::Child {
    #[cfg(unix)]
    return Command::new("true").spawn().unwrap();
    #[cfg(windows)]
    return Command::new("cmd").args(["/C", "exit 0"]).spawn().unwrap();
}

fn spawn_exit_1() -> std::process::Child {
    #[cfg(unix)]
    return Command::new("false").spawn().unwrap();
    #[cfg(windows)]
    return Command::new("cmd").args(["/C", "exit 1"]).spawn().unwrap();
}

fn spawn_long_running() -> std::process::Child {
    #[cfg(unix)]
    return Command::new("sleep").arg("100").spawn().unwrap();
    #[cfg(windows)]
    return Command::new("powershell")
        .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 100"])
        .spawn()
        .unwrap();
}

#[test]
fn test_settings_verbosity() {
    let _ = Settings::new().verbosity(Verbosity::Debug);
}

#[test]
fn test_is_in_ci_some_expected_variant() {
    // Removing "CI" (a None-type entry) forces the iterator to continue and
    // evaluate the Some("true") entries such as TF_BUILD and GITHUB_ACTIONS,
    // exercising the `Some(expected)` match arm in is_in_ci().
    let ci = std::env::var_os("CI");
    unsafe {
        std::env::remove_var("CI");
        std::env::set_var("TF_BUILD", "true");
    }
    let result = is_in_ci();
    unsafe {
        std::env::remove_var("TF_BUILD");
        if let Some(val) = ci {
            std::env::set_var("CI", val);
        }
    }
    assert!(
        result,
        "TF_BUILD=true should be detected as a CI environment"
    );
}

#[test]
fn test_settings_new_in_ci_disables_database() {
    // Temporarily set a CI env var so is_in_ci() returns true.
    // Using TEAMCITY_VERSION (checked with None, i.e. any value suffices).
    let key = "TEAMCITY_VERSION";
    let had_key = std::env::var_os(key).is_some();
    unsafe {
        std::env::set_var(key, "1");
    }
    let settings = Settings::new();
    if !had_key {
        unsafe {
            std::env::remove_var(key);
        }
    }
    assert_eq!(settings.database, Database::Disabled);
    assert!(settings.derandomize);
}

#[test]
fn test_wait_for_exit_child_exits() {
    let mut child = spawn_exit_0();
    let result = wait_for_exit(&mut child, Duration::from_secs(5));
    assert!(result.is_some());
}

#[test]
fn test_wait_for_exit_timeout() {
    let mut child = spawn_long_running();
    let result = wait_for_exit(&mut child, Duration::from_millis(50));
    assert!(result.is_none());
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_startup_error_message_version_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let script = {
        use std::io::Write;
        let s = dir.path().join("fake_version");
        let mut f = std::fs::File::create(&s).unwrap();
        f.write_all(b"#!/bin/sh\necho 'hegel (version 0.0.0)'\n")
            .unwrap();
        f.sync_all().unwrap();
        drop(f);
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&s, std::fs::Permissions::from_mode(0o755)).unwrap();
        s
    };
    #[cfg(windows)]
    let script = {
        let s = dir.path().join("fake_version.bat");
        std::fs::write(&s, "@echo off\r\necho hegel (version 0.0.0)\r\n").unwrap();
        s
    };
    let exit_status = exit_failure_status();
    let msg = startup_error_message(Some(script.to_str().unwrap()), exit_status);
    assert!(msg.contains("Version mismatch"), "Message: {msg}");
}

#[test]
fn test_startup_error_message_not_hegel() {
    let exit_status = exit_failure_status();
    #[cfg(unix)]
    let binary = "false";
    // Use a binary that exits with failure when given --version.
    // cmd.exe won't work because `cmd.exe --version` succeeds on Windows.
    #[cfg(windows)]
    let binary = "where.exe";
    let msg = startup_error_message(Some(binary), exit_status);
    assert!(msg.contains("Is this a hegel binary"), "Message: {msg}");
}

#[test]
fn test_startup_error_message_binary_not_found() {
    let exit_status = exit_failure_status();
    let msg = startup_error_message(Some("/nonexistent/path/hegel_xyz"), exit_status);
    assert!(msg.contains("Is this a hegel binary"), "Message: {msg}");
}

#[test]
fn test_startup_error_message_no_binary_path() {
    let exit_status = exit_failure_status();
    let msg = startup_error_message(None, exit_status);
    assert!(msg.contains("failed during startup"), "Message: {msg}");
    assert!(!msg.contains("hegel binary"), "Message: {msg}");
}

#[test]
fn test_startup_error_message_includes_server_log() {
    let _guard = LOG_TEST_LOCK.lock().unwrap();
    write_server_log("Error: startup failed\nDetail 1\nDetail 2\nDetail 3\n");

    let exit_status = exit_failure_status();
    #[cfg(unix)]
    let binary = "false";
    #[cfg(windows)]
    let binary = "cmd.exe";
    let msg = startup_error_message(Some(binary), exit_status);
    assert!(msg.contains("Server log"), "Message: {msg}");
    assert!(msg.contains("for full output"), "Message: {msg}");
    remove_server_log();
}

#[test]
#[cfg(unix)]
#[should_panic(expected = "not executable")]
fn test_validate_executable_panics_for_non_executable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("not_exec");
    std::fs::write(&path, "").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    crate::utils::validate_executable(path.to_str().unwrap());
}

#[test]
fn test_resolve_hegel_path_existing_executable() {
    #[cfg(unix)]
    {
        let result = resolve_hegel_path("/bin/sh");
        assert_eq!(result, "/bin/sh");
    }
    #[cfg(windows)]
    {
        let cmd_path = std::env::var("ComSpec").unwrap();
        let result = resolve_hegel_path(&cmd_path);
        assert_eq!(result, cmd_path);
    }
}

#[test]
fn test_resolve_hegel_path_bare_name_on_path() {
    #[cfg(unix)]
    {
        let result = resolve_hegel_path("sh");
        assert!(result.contains("sh"));
    }
    #[cfg(windows)]
    {
        let result = resolve_hegel_path("cmd");
        assert!(result.to_lowercase().contains("cmd"));
    }
}

#[test]
#[should_panic(expected = "not found on PATH")]
fn test_resolve_hegel_path_bare_name_not_on_path() {
    resolve_hegel_path("definitely_not_a_real_binary_xyz_123");
}

#[test]
#[should_panic(expected = "not found at")]
fn test_resolve_hegel_path_nonexistent_absolute() {
    resolve_hegel_path("/nonexistent/path/to/hegel");
}

#[test]
#[should_panic(expected = "failed during startup")]
fn test_handle_handshake_failure_child_exited() {
    let mut child = spawn_exit_1();
    // Wait for the child to fully exit. Without this, there's a race condition:
    // wait_for_exit inside handle_handshake_failure might not see the exit in
    // its 100ms window, hitting the "child still running" branch instead.
    let _ = child.wait();
    #[cfg(unix)]
    let binary = "false";
    #[cfg(windows)]
    let binary = "cmd.exe";
    handle_handshake_failure(&mut child, Some(binary), "test error");
}

#[test]
#[should_panic(expected = "Possibly bad virtualenv")]
fn test_handle_handshake_failure_child_hangs() {
    let mut child = spawn_long_running();
    handle_handshake_failure(&mut child, None, "test error");
}

#[test]
fn test_parse_version_valid() {
    assert!(parse_version("0.1") < parse_version("0.2"));
    assert!(parse_version("0.2") > parse_version("0.1"));
    assert_eq!(parse_version("1.0"), parse_version("1.0"));
    assert!(parse_version("2.0") > parse_version("1.9"));
    assert!(parse_version("1.9") < parse_version("2.0"));
}

#[test]
#[should_panic(expected = "expected 'major.minor' format")]
fn test_parse_version_no_dot() {
    parse_version("1");
}

#[test]
#[should_panic(expected = "expected 'major.minor' format")]
fn test_parse_version_too_many_parts() {
    parse_version("1.2.3");
}

#[test]
#[should_panic(expected = "invalid major version")]
fn test_parse_version_non_numeric_major() {
    parse_version("abc.1");
}

#[test]
#[should_panic(expected = "invalid minor version")]
fn test_parse_version_non_numeric_minor() {
    parse_version("1.abc");
}

#[test]
#[should_panic(expected = "expected 'major.minor' format")]
fn test_parse_version_empty_string() {
    parse_version("");
}

#[test]
fn test_protocol_debug_true_when_env_set() {
    // Set the env var BEFORE the LazyLock is first accessed in this binary.
    // No other test in the lib binary touches PROTOCOL_DEBUG, so this is the
    // first access and the closure evaluates with the env var present.
    // This exercises the "1" | "true" arm of the matches! macro.
    unsafe {
        std::env::set_var("HEGEL_PROTOCOL_DEBUG", "true");
    }
    assert!(*PROTOCOL_DEBUG);
    unsafe {
        std::env::remove_var("HEGEL_PROTOCOL_DEBUG");
    }
}

// Serialize tests that read/write the server log to prevent interference
// between parallel test threads.
static LOG_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Return the path that `server_log_excerpt()` reads from, updating
/// `SERVER_LOG_PATH` to point at a test-specific log file.
fn log_path() -> String {
    let path = format!("{HEGEL_SERVER_DIR}/server.test.log");
    *SERVER_LOG_PATH.lock().unwrap() = Some(path.clone());
    path
}

fn write_server_log(content: &str) {
    std::fs::create_dir_all(HEGEL_SERVER_DIR).ok();
    std::fs::write(log_path(), content).ok();
}

fn remove_server_log() {
    std::fs::remove_file(log_path()).ok();
}

#[test]
fn server_log_excerpt_no_file() {
    let _guard = LOG_TEST_LOCK.lock().unwrap();
    remove_server_log();
    assert!(server_log_excerpt().is_none());
}

#[test]
fn server_log_excerpt_empty_file() {
    let _guard = LOG_TEST_LOCK.lock().unwrap();
    write_server_log("");
    assert!(server_log_excerpt().is_none());
    remove_server_log();
}

#[test]
fn server_log_excerpt_non_empty_file() {
    let _guard = LOG_TEST_LOCK.lock().unwrap();
    write_server_log("Error: test crash\n");
    assert!(server_log_excerpt().is_some());
    remove_server_log();
}

#[test]
fn server_crash_message_includes_log_excerpt() {
    let _guard = LOG_TEST_LOCK.lock().unwrap();
    write_server_log("Error: test crash\n");
    let msg = server_crash_message();
    assert!(msg.contains("Error: test crash"), "got: {msg}");
    remove_server_log();
}

#[test]
fn handle_channel_error_connection_aborted() {
    let _guard = LOG_TEST_LOCK.lock().unwrap();
    remove_server_log();
    let err = std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "test");
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        handle_channel_error(err);
    }));
    let panic_val = result.expect_err("handle_channel_error should have panicked");
    let msg = panic_val
        .downcast_ref::<String>()
        .map(|s| s.as_str())
        .or_else(|| panic_val.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        msg.contains("hegel server process exited unexpectedly"),
        "unexpected panic message: {msg}"
    );
}

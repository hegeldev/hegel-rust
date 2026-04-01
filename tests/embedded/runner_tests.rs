use super::*;

#[test]
fn test_settings_verbosity() {
    let _ = Settings::new().verbosity(Verbosity::Debug);
}

#[test]
fn test_wait_for_exit_child_exits() {
    let mut child = Command::new("true").spawn().unwrap();
    let result = wait_for_exit(&mut child, Duration::from_secs(5));
    assert!(result.is_some());
}

#[test]
fn test_wait_for_exit_timeout() {
    let mut child = Command::new("sleep").arg("100").spawn().unwrap();
    let result = wait_for_exit(&mut child, Duration::from_millis(50));
    assert!(result.is_none());
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_startup_error_message_version_mismatch() {
    let dir = std::env::temp_dir().join("hegel_test_unit_version");
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("fake_version");
    std::fs::write(&script, "#!/bin/sh\necho 'hegel (version 0.0.0)'\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let exit_status = Command::new("false").status().unwrap();
    let msg = startup_error_message(Some(script.to_str().unwrap()), exit_status);
    assert!(msg.contains("Version mismatch"), "Message: {msg}");
}

#[test]
fn test_startup_error_message_not_hegel() {
    let exit_status = Command::new("false").status().unwrap();
    let msg = startup_error_message(Some("false"), exit_status);
    assert!(msg.contains("Is this a hegel binary"), "Message: {msg}");
}

#[test]
fn test_startup_error_message_binary_not_found() {
    let exit_status = Command::new("false").status().unwrap();
    let msg = startup_error_message(Some("/nonexistent/path/hegel_xyz"), exit_status);
    assert!(msg.contains("Is this a hegel binary"), "Message: {msg}");
}

#[test]
fn test_startup_error_message_no_binary_path() {
    let exit_status = Command::new("false").status().unwrap();
    let msg = startup_error_message(None, exit_status);
    assert!(msg.contains("failed during startup"), "Message: {msg}");
    assert!(!msg.contains("hegel binary"), "Message: {msg}");
}

#[test]
fn test_startup_error_message_includes_server_log() {
    let dir = std::env::temp_dir().join("hegel_test_unit_log");
    std::fs::create_dir_all(&dir).unwrap();
    let log_file = dir.join("server.log");
    std::fs::write(
        &log_file,
        "Error: startup failed\nDetail 1\nDetail 2\nDetail 3\n",
    )
    .unwrap();
    let log_path_str = log_file.to_string_lossy().to_string();
    let _ = SERVER_LOG_PATH.set(log_path_str.clone());

    let exit_status = Command::new("false").status().unwrap();
    let msg = startup_error_message(Some("false"), exit_status);
    // Only assert if we successfully set the path (OnceLock may already be set)
    if SERVER_LOG_PATH.get() == Some(&log_path_str) {
        assert!(msg.contains("Server log"), "Message: {msg}");
        assert!(msg.contains("for full output"), "Message: {msg}");
    }
}

#[test]
#[cfg(unix)]
#[should_panic(expected = "not executable")]
fn test_validate_executable_panics_for_non_executable() {
    let dir = std::env::temp_dir().join("hegel_test_unit_exec");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("not_exec");
    std::fs::write(&path, "").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    crate::utils::validate_executable(path.to_str().unwrap());
}

#[test]
fn test_resolve_hegel_path_existing_executable() {
    let result = resolve_hegel_path("/bin/sh");
    assert_eq!(result, "/bin/sh");
}

#[test]
fn test_resolve_hegel_path_bare_name_on_path() {
    let result = resolve_hegel_path("sh");
    assert!(result.contains("sh"));
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
    let mut child = Command::new("false").spawn().unwrap();
    // Wait for the child to fully exit. Without this, there's a race condition:
    // wait_for_exit inside handle_handshake_failure might not see the exit in
    // its 100ms window, hitting the "child still running" branch instead.
    let _ = child.wait();
    handle_handshake_failure(&mut child, Some("false"), "test error");
}

#[test]
#[should_panic(expected = "Possibly bad virtualenv")]
fn test_handle_handshake_failure_child_hangs() {
    let mut child = Command::new("sleep").arg("100").spawn().unwrap();
    handle_handshake_failure(&mut child, None, "test error");
}

// Serialize tests that read/write the server log to prevent interference
// between parallel test threads.
static LOG_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Return the path that `server_log_excerpt()` reads from, ensuring
/// `SERVER_LOG_PATH` is initialised.
fn log_path() -> &'static String {
    let _ = SERVER_LOG_PATH.set(format!("{HEGEL_SERVER_DIR}/server.test.log"));
    SERVER_LOG_PATH.get().unwrap()
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

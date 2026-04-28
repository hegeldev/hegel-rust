use super::*;
use std::panic::AssertUnwindSafe;
use std::process::Command;
use std::time::Duration;

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
    return Command::new("cmd")
        .args(["/C", "ping -n 100 127.0.0.1 >nul"])
        .spawn()
        .unwrap();
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

fn exit_success_status() -> std::process::ExitStatus {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;
    std::process::ExitStatus::from_raw(0)
}

fn fake_output(status: std::process::ExitStatus, stdout: &str) -> std::process::Output {
    std::process::Output {
        status,
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
    }
}

#[test]
fn test_version_check_message_mismatch() {
    let output = fake_output(exit_success_status(), "hegel (version 0.0.0)\n");
    let msg = version_check_message("/fake/path", Ok(output)).unwrap();
    assert!(msg.contains("Version mismatch"), "Message: {msg}");
}

#[test]
fn test_version_check_message_match() {
    let expected = format!("hegel (version {HEGEL_SERVER_VERSION})\n");
    let output = fake_output(exit_success_status(), &expected);
    assert!(version_check_message("/fake/path", Ok(output)).is_none());
}

#[test]
fn test_startup_error_message_version_mismatch() {
    let exit_status = exit_failure_status();
    let version_output = fake_output(exit_success_status(), "hegel (version 0.0.0)\n");
    let msg =
        startup_error_message_from_version(Some(("/fake/path", Ok(version_output))), exit_status);
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

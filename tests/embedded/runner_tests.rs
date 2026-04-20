use super::*;

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
    let _guard = LOG_TEST_LOCK.lock().unwrap();
    write_server_log("Error: startup failed\nDetail 1\nDetail 2\nDetail 3\n");

    let exit_status = Command::new("false").status().unwrap();
    let msg = startup_error_message(Some("false"), exit_status);
    assert!(msg.contains("Server log"), "Message: {msg}");
    assert!(msg.contains("for full output"), "Message: {msg}");
    remove_server_log();
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

#[test]
fn record_test_case_result_non_final_is_noop() {
    let mut count = 0u64;
    let mut msg: Option<String> = None;
    record_test_case_result(
        false,
        TestCaseResult::Interesting {
            panic_message: "ignored".into(),
        },
        &mut count,
        &mut msg,
    );
    assert_eq!(count, 0);
    assert!(msg.is_none());
}

#[test]
fn record_test_case_result_final_valid_counts_only() {
    let mut count = 0u64;
    let mut msg: Option<String> = None;
    record_test_case_result(true, TestCaseResult::Valid, &mut count, &mut msg);
    assert_eq!(count, 1);
    assert!(msg.is_none());
}

#[test]
fn record_test_case_result_final_invalid_counts_only() {
    let mut count = 0u64;
    let mut msg: Option<String> = None;
    record_test_case_result(true, TestCaseResult::Invalid, &mut count, &mut msg);
    assert_eq!(count, 1);
    assert!(msg.is_none());
}

#[test]
fn record_test_case_result_final_overrun_counts_only() {
    let mut count = 0u64;
    let mut msg: Option<String> = None;
    record_test_case_result(true, TestCaseResult::Overrun, &mut count, &mut msg);
    assert_eq!(count, 1);
    assert!(msg.is_none());
}

#[test]
fn record_test_case_result_final_interesting_captures_message() {
    let mut count = 0u64;
    let mut msg: Option<String> = None;
    record_test_case_result(
        true,
        TestCaseResult::Interesting {
            panic_message: "boom".into(),
        },
        &mut count,
        &mut msg,
    );
    assert_eq!(count, 1);
    assert_eq!(msg.as_deref(), Some("boom"));
}

#[test]
fn pinned_server_version_is_nonempty() {
    assert!(!pinned_server_version().is_empty());
}

#[test]
fn parse_semver_accepts_three_numeric_parts() {
    assert_eq!(parse_semver("0.4.5"), Some((0, 4, 5)));
    assert_eq!(parse_semver("1.20.300"), Some((1, 20, 300)));
}

#[test]
fn parse_semver_rejects_wrong_number_of_parts() {
    assert_eq!(parse_semver("0.4"), None);
    assert_eq!(parse_semver("0.4.5.6"), None);
    assert_eq!(parse_semver(""), None);
}

#[test]
fn parse_semver_rejects_non_numeric_parts() {
    assert_eq!(parse_semver("a.4.5"), None);
    assert_eq!(parse_semver("0.b.5"), None);
    assert_eq!(parse_semver("0.4.c"), None);
}

#[test]
fn supports_one_shot_at_min_version_is_true() {
    let (maj, min, patch) = ONE_SHOT_MIN_SERVER_VERSION;
    assert!(supports_one_shot(&format!("{maj}.{min}.{patch}")));
}

#[test]
fn supports_one_shot_below_min_version_is_false() {
    // Compute a version strictly below the minimum by decrementing patch, or
    // minor if patch is 0.
    let (maj, min, patch) = ONE_SHOT_MIN_SERVER_VERSION;
    let below = if patch > 0 {
        format!("{maj}.{min}.{}", patch - 1)
    } else if min > 0 {
        format!("{maj}.{}.{}", min - 1, u32::MAX)
    } else {
        format!("{}.{}.{}", maj - 1, u32::MAX, u32::MAX)
    };
    assert!(!supports_one_shot(&below));
}

#[test]
fn supports_one_shot_above_min_version_is_true() {
    let (maj, _, _) = ONE_SHOT_MIN_SERVER_VERSION;
    assert!(supports_one_shot(&format!("{}.0.0", maj + 1)));
}

#[test]
fn supports_one_shot_unparseable_is_false() {
    assert!(!supports_one_shot("not-a-version"));
}

#[test]
fn parse_version_output_extracts_semver() {
    assert_eq!(
        parse_version_output("hegel (version 1.2.3)\n").as_deref(),
        Some("1.2.3")
    );
    assert_eq!(
        parse_version_output("wrapper says: hegel (version 0.4.5) and then some").as_deref(),
        Some("0.4.5")
    );
}

#[test]
fn parse_version_output_returns_none_when_format_unexpected() {
    assert_eq!(parse_version_output("hegel 1.2.3"), None);
    assert_eq!(parse_version_output("version 1.2.3 no closing paren"), None);
    assert_eq!(parse_version_output(""), None);
}

#[test]
fn effective_server_version_falls_back_to_pinned_without_env() {
    // No env var lookup wrapper here — tests in this file don't set
    // HEGEL_SERVER_COMMAND by default. If another test has set it, this
    // assertion would still hold only when the pointed-at binary parses.
    let prior = std::env::var(HEGEL_SERVER_COMMAND_ENV).ok();
    unsafe {
        std::env::remove_var(HEGEL_SERVER_COMMAND_ENV);
    }
    let v = effective_server_version();
    if let Some(val) = prior {
        unsafe {
            std::env::set_var(HEGEL_SERVER_COMMAND_ENV, val);
        }
    }
    assert_eq!(v, pinned_server_version());
}

/// Lock around tests that manipulate HEGEL_SERVER_COMMAND so they don't race.
static SERVER_COMMAND_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn with_fake_server_command<R>(stdout: &str, exit_code: i32, f: impl FnOnce() -> R) -> R {
    let _guard = SERVER_COMMAND_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir =
        std::env::temp_dir().join(format!("hegel_test_fake_server_cmd_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fake_hegel");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo '{stdout}'; exit {exit_code}; fi\nexit {exit_code}\n",
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let prior = std::env::var(HEGEL_SERVER_COMMAND_ENV).ok();
    unsafe {
        std::env::set_var(HEGEL_SERVER_COMMAND_ENV, path.to_str().unwrap());
    }
    let result = f();
    unsafe {
        match prior {
            Some(v) => std::env::set_var(HEGEL_SERVER_COMMAND_ENV, v),
            None => std::env::remove_var(HEGEL_SERVER_COMMAND_ENV),
        }
    }
    result
}

#[test]
fn effective_server_version_reads_from_env_binary() {
    let v = with_fake_server_command("hegel (version 9.8.7)", 0, effective_server_version);
    assert_eq!(v, "9.8.7");
}

#[test]
fn effective_server_version_falls_back_when_env_binary_fails() {
    let v = with_fake_server_command("hegel (version 9.8.7)", 1, effective_server_version);
    assert_eq!(v, pinned_server_version());
}

#[test]
fn effective_server_version_falls_back_when_env_binary_output_unparseable() {
    let v = with_fake_server_command("not-a-hegel-binary", 0, effective_server_version);
    assert_eq!(v, pinned_server_version());
}

#[test]
fn effective_server_version_falls_back_when_env_binary_cannot_be_spawned() {
    let _guard = SERVER_COMMAND_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let prior = std::env::var(HEGEL_SERVER_COMMAND_ENV).ok();
    unsafe {
        std::env::set_var(
            HEGEL_SERVER_COMMAND_ENV,
            "/definitely/nonexistent/hegel_xyz",
        );
    }
    let v = effective_server_version();
    unsafe {
        match prior {
            Some(val) => std::env::set_var(HEGEL_SERVER_COMMAND_ENV, val),
            None => std::env::remove_var(HEGEL_SERVER_COMMAND_ENV),
        }
    }
    assert_eq!(v, pinned_server_version());
}

#[test]
#[should_panic(expected = "Settings::one_shot requires hegel-core")]
fn require_one_shot_support_panics_when_version_too_old() {
    with_fake_server_command("hegel (version 0.0.1)", 0, require_one_shot_support);
}

#[test]
fn require_one_shot_support_returns_when_version_new_enough() {
    let (maj, min, patch) = ONE_SHOT_MIN_SERVER_VERSION;
    let new_ver = format!("hegel (version {maj}.{min}.{patch})");
    with_fake_server_command(&new_ver, 0, require_one_shot_support);
}

#![cfg(not(feature = "native"))]
mod common;

use common::project::TempRustProject;
use common::utils::FindAny;
use hegel::generators as gs;

/// format_log_excerpt should return "(empty)" for empty input.
#[test]
fn format_log_excerpt_empty_content() {
    assert_eq!(hegel::format_log_excerpt(""), "(empty)");
}

/// format_log_excerpt should return all content when there are fewer than
/// MAX_UNINDENTED (5) unindented lines.
#[test]
fn format_log_excerpt_short_log() {
    let content = "Error: something went wrong\nDetails here";
    let result = hegel::format_log_excerpt(content);
    assert_eq!(result, content);
}

/// format_log_excerpt should only show the last 5 unindented lines (and
/// everything between them).
#[test]
fn format_log_excerpt_takes_last_n_unindented() {
    // 10 unindented lines — only the last 5 should appear
    let lines: Vec<String> = (0..10).map(|i| format!("line {}", i)).collect();
    let content = lines.join("\n");
    let result = hegel::format_log_excerpt(&content);
    assert!(result.contains("line 5"), "should include line 5: {result}");
    assert!(result.contains("line 9"), "should include line 9: {result}");
    assert!(
        !result.contains("line 4"),
        "should not include line 4: {result}"
    );
}

/// Long runs of indented lines (>10) should be truncated in the middle.
#[test]
fn format_log_excerpt_truncates_long_indent_runs() {
    let mut lines = vec!["Error start".to_string()];
    for i in 0..20 {
        lines.push(format!("  frame {}", i));
    }
    lines.push("Error end".to_string());
    let content = lines.join("\n");
    let result = hegel::format_log_excerpt(&content);
    assert!(
        result.contains("[..."),
        "should contain truncation marker: {result}"
    );
    // First and last few frames should still be present
    assert!(
        result.contains("frame 0"),
        "should show first frame: {result}"
    );
    assert!(
        result.contains("frame 19"),
        "should show last frame: {result}"
    );
    // Middle frames should be gone
    assert!(
        !result.contains("frame 10"),
        "should not show middle frame: {result}"
    );
}

/// Short indented runs (≤10) should not be truncated.
#[test]
fn format_log_excerpt_keeps_short_indent_runs() {
    let mut lines = vec!["Error".to_string()];
    for i in 0..8 {
        lines.push(format!("  frame {}", i));
    }
    lines.push("End".to_string());
    let content = lines.join("\n");
    let result = hegel::format_log_excerpt(&content);
    assert!(
        !result.contains("[..."),
        "should not truncate short run: {result}"
    );
    assert!(
        result.contains("frame 7"),
        "all frames should be present: {result}"
    );
}

/// find_any should re-propagate panics from inside the condition (which are
/// surfaced as "Property test failed: ...") instead of swallowing them and
/// reporting "Could not find any examples".
#[test]
fn find_any_propagates_panics_from_condition() {
    let result = std::panic::catch_unwind(|| {
        FindAny::new(gs::booleans(), |_| -> bool {
            panic!("deliberate_condition_panic");
        })
        .max_attempts(10)
        .run()
    });

    let err = result.expect_err("expected panic, but find_any returned normally");
    let msg = err
        .downcast_ref::<String>()
        .map(|s| s.as_str())
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or_default();

    assert!(
        msg.contains("deliberate_condition_panic"),
        "expected panic message to mention 'deliberate_condition_panic', got: {msg}"
    );
}

const HEGEL_TEST_CODE: &str = r#"
fn main() {
    hegel::hegel(|tc| {
        let _ = tc.draw(hegel::generators::booleans());
    });
}
"#;

/// Returns true if the currently-configured hegel server supports protocol test
/// modes like `crash_after_handshake`. These modes were added in hegel-core 0.4.2.
///
/// When `HEGEL_SERVER_COMMAND` is not set, the library spawns the server via
/// `uv tool run --from hegel-core=={pinned}`, which is always 0.4.2+ — crash
/// modes are always available. When it IS set (e.g. the `test-min-protocol` CI
/// job installs an older release), we query the binary's version to decide.
fn hegel_supports_crash_modes() -> bool {
    let Ok(cmd) = std::env::var("HEGEL_SERVER_COMMAND") else {
        return true;
    };
    let Ok(out) = std::process::Command::new(&cmd).arg("--version").output() else {
        return false;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    // Expected format: "hegel (version X.Y.Z)"
    if let Some(start) = text.find("version ") {
        let rest = text[start + 8..].trim_start();
        let ver = rest.split(')').next().unwrap_or("").trim();
        let parts: Vec<u32> = ver.split('.').filter_map(|p| p.parse().ok()).collect();
        if parts.len() == 3 {
            return (parts[0], parts[1], parts[2]) >= (0, 4, 2);
        }
    }
    false
}

/// When the server writes to its log before crashing, the error message should
/// include the log content so the user can diagnose the crash.
#[test]
fn test_server_crash_includes_log_content() {
    if !hegel_supports_crash_modes() {
        return;
    }
    TempRustProject::new()
        .main_file(HEGEL_TEST_CODE)
        .env(
            "HEGEL_PROTOCOL_TEST_MODE",
            "crash_after_handshake_with_stderr",
        )
        .expect_failure("FakeServerError: intentional crash for testing")
        .cargo_run(&[]);
}

/// When the server log is empty (server crashed before writing anything), the
/// error message should still explain that the server crashed.
#[test]
fn test_server_crash_empty_log() {
    if !hegel_supports_crash_modes() {
        return;
    }
    TempRustProject::new()
        .main_file(HEGEL_TEST_CODE)
        .env("HEGEL_PROTOCOL_TEST_MODE", "crash_after_handshake")
        .expect_failure("The hegel server process exited unexpectedly")
        .cargo_run(&[]);
}

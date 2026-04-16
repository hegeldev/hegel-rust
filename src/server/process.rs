use std::fs::{File, OpenOptions};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::session::{HEGEL_SERVER_VERSION, SESSION};

pub(super) const HEGEL_SERVER_COMMAND_ENV: &str = "HEGEL_SERVER_COMMAND";
const HEGEL_SERVER_DIR: &str = ".hegel";
pub(super) static SERVER_LOG_PATH: Mutex<Option<String>> = Mutex::new(None);
static LOG_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) fn hegel_command() -> Command {
    if let Ok(override_path) = std::env::var(HEGEL_SERVER_COMMAND_ENV) {
        return Command::new(resolve_hegel_path(&override_path)); // nocov
    }
    let uv_path = crate::server::uv::find_uv();
    let mut cmd = Command::new(uv_path);
    cmd.args([
        "tool",
        "run",
        "--from",
        &format!("hegel-core=={HEGEL_SERVER_VERSION}"),
        "hegel",
    ]);
    cmd
}

pub(super) fn server_log_file() -> File {
    std::fs::create_dir_all(HEGEL_SERVER_DIR).ok();
    let pid = std::process::id();
    let ix = LOG_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = format!("{HEGEL_SERVER_DIR}/server.{pid}-{ix}.log");
    *SERVER_LOG_PATH.lock().unwrap() = Some(path.clone());
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .expect("Failed to open server log file")
}

fn wait_for_exit(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Option<std::process::ExitStatus> {
    let start = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if start.elapsed() >= timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn handle_handshake_failure(
    child: &mut std::process::Child,
    binary_path: Option<&str>,
    handshake_err: impl std::fmt::Display,
) -> ! {
    let exit_status = wait_for_exit(child, Duration::from_millis(100));
    let child_still_running = exit_status.is_none();
    if child_still_running {
        let _ = child.kill();
        let _ = child.wait();
        panic!(
            "The hegel server failed during startup handshake: {handshake_err}\n\n\
             The server process did not exit. Possibly bad virtualenv?"
        );
    }
    panic!(
        "{}",
        startup_error_message(binary_path, exit_status.unwrap())
    );
}

fn startup_error_message(
    binary_path: Option<&str>,
    exit_status: std::process::ExitStatus,
) -> String {
    let mut parts = Vec::new();

    parts.push("The hegel server failed during startup handshake.".to_string());
    parts.push(format!("The server process exited with {}.", exit_status));

    // Version detection via --version (only when we have a binary path to check)
    if let Some(binary_path) = binary_path {
        let expected_version_string = format!("hegel (version {})", HEGEL_SERVER_VERSION);
        match Command::new(binary_path).arg("--version").output() {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if stdout != expected_version_string {
                    parts.push(format!(
                        "Version mismatch: expected '{}', got '{}'.",
                        expected_version_string, stdout
                    ));
                }
            }
            Ok(_) => {
                parts.push(format!(
                    "'{}' --version exited unsuccessfully. Is this a hegel binary?",
                    binary_path
                ));
            }
            Err(e) => {
                parts.push(format!(
                    "Could not run '{}' --version: {}. Is this a hegel binary?",
                    binary_path, e
                ));
            }
        }
    }

    // Include server log contents
    if let Some(log_path) = SERVER_LOG_PATH.lock().unwrap().clone() {
        if let Ok(contents) = std::fs::read_to_string(&log_path) {
            if !contents.trim().is_empty() {
                let lines: Vec<&str> = contents.lines().collect();
                let display_lines: Vec<&str> = lines.iter().take(3).copied().collect();
                let mut log_section =
                    format!("Server log ({}):\n{}", log_path, display_lines.join("\n"));
                if lines.len() > 3 {
                    log_section.push_str(&format!("\n... (see {} for full output)", log_path));
                }
                parts.push(log_section);
            }
        }
    }

    parts.join("\n\n")
}

fn resolve_hegel_path(path: &str) -> String {
    let p = std::path::Path::new(path);
    if p.exists() {
        crate::server::utils::validate_executable(path);
        return path.to_string();
    }

    // Bare name (no path separator) — try PATH lookup
    if !path.chars().any(std::path::is_separator) {
        if let Some(resolved) = crate::server::utils::which(path) {
            crate::server::utils::validate_executable(&resolved);
            return resolved;
        }
        panic!(
            "Hegel server binary '{}' not found on PATH. \
             Check that {} is set correctly, or install hegel-core.",
            path, HEGEL_SERVER_COMMAND_ENV
        );
    }

    panic!(
        "Hegel server binary not found at '{}'. \
         Check that {} is set correctly.",
        path, HEGEL_SERVER_COMMAND_ENV
    );
}

/// Format a server log excerpt for inclusion in error messages.
///
/// Returns the last 5 unindented lines and the content between them. Runs of
/// more than 10 consecutive indented lines are truncated with a summary.
pub fn format_log_excerpt(content: &str) -> String {
    const MAX_UNINDENTED: usize = 5;
    const INDENT_THRESHOLD: usize = 10;
    const INDENT_CONTEXT: usize = 3;

    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return "(empty)".to_string();
    }

    // Find start: walk backwards until we've seen MAX_UNINDENTED unindented lines
    let mut unindented_seen = 0;
    let mut start_idx = 0;
    for (i, line) in lines.iter().enumerate().rev() {
        if is_log_unindented(line) {
            unindented_seen += 1;
            if unindented_seen >= MAX_UNINDENTED {
                start_idx = i;
                break;
            }
        }
    }

    // Process the relevant section, truncating long indented runs
    let relevant = &lines[start_idx..];
    let mut output: Vec<String> = Vec::new();
    let mut indent_run: Vec<&str> = Vec::new();

    for &line in relevant {
        if is_log_unindented(line) {
            flush_log_indent_run(
                &mut indent_run,
                &mut output,
                INDENT_THRESHOLD,
                INDENT_CONTEXT,
            );
            output.push(line.to_string());
        } else {
            indent_run.push(line);
        }
    }
    flush_log_indent_run(
        &mut indent_run,
        &mut output,
        INDENT_THRESHOLD,
        INDENT_CONTEXT,
    );

    output.join("\n")
}

fn is_log_unindented(line: &str) -> bool {
    !line.is_empty() && !line.starts_with(' ') && !line.starts_with('\t')
}

fn flush_log_indent_run(
    run: &mut Vec<&str>,
    output: &mut Vec<String>,
    threshold: usize,
    context: usize,
) {
    if run.is_empty() {
        return;
    }
    if run.len() > threshold {
        let keep = context.min(run.len() / 2);
        for &line in &run[..keep] {
            output.push(line.to_string());
        }
        let hidden = run.len() - 2 * keep;
        output.push(format!("  [...{hidden} lines...]"));
        for &line in &run[run.len() - keep..] {
            output.push(line.to_string());
        }
    } else {
        for &line in run.iter() {
            output.push(line.to_string());
        }
    }
    run.clear();
}

fn server_log_excerpt() -> Option<String> {
    let log_path = SERVER_LOG_PATH.lock().unwrap().clone()?;
    let content = std::fs::read_to_string(log_path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(format_log_excerpt(trimmed))
}

pub(super) fn server_crash_message() -> String {
    const BASE: &str = "The hegel server process exited unexpectedly.";
    let log_path_owned = SERVER_LOG_PATH.lock().unwrap().clone();
    let log_path = log_path_owned.as_deref().unwrap_or(".hegel/server.log");
    match server_log_excerpt() {
        Some(excerpt) => format!("{BASE}\n\nLast server log entries:\n{excerpt}"),
        None => format!("{BASE}\n\n(No entries found in {log_path})"),
    }
}

pub(super) fn handle_channel_error(e: std::io::Error) -> ! {
    if e.kind() == std::io::ErrorKind::ConnectionAborted {
        panic!("{}", server_crash_message());
    }
    unreachable!("unexpected channel error: {e}")
}

/// Kill the hegel server process and wait until the connection detects that it
/// has exited.  Only for use in tests — not part of the public API.
#[doc(hidden)]
pub fn __test_kill_server() {
    let guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(session) = guard.as_ref() {
        let child_arc = Arc::clone(&session.child);
        let conn = Arc::clone(&session.connection);
        drop(guard);
        let _ = child_arc.lock().unwrap().kill();
        while !conn.server_has_exited() {
            std::thread::yield_now();
        }
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/server/process_tests.rs"]
mod tests;

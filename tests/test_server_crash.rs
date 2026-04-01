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

// A fake server script that:
// 1. Writes a specific error message to stderr (which goes to server.log)
// 2. Completes the version handshake
// 3. Exits before responding to run_test (simulating a server crash)
//
// The binary protocol uses 20-byte packets: magic(4) + crc32(4) + channel(4) +
// message_id(4) + payload_len(4), followed by payload bytes and a 0x0A terminator.
const FAKE_SERVER_WITH_LOG: &str = r#"#!/usr/bin/env python3
import sys
import struct
import binascii

MAGIC = 0x4845474C
REPLY_BIT = 0x80000000
TERM = b'\x0a'

def read_exact(n):
    data = b''
    while len(data) < n:
        chunk = sys.stdin.buffer.read(n - len(data))
        if not chunk:
            return None
        data += chunk
    return data

def read_packet():
    hdr = read_exact(20)
    if hdr is None:
        return None
    channel = struct.unpack('>I', hdr[8:12])[0]
    mid_raw = struct.unpack('>I', hdr[12:16])[0]
    length = struct.unpack('>I', hdr[16:20])[0]
    message_id = mid_raw & ~REPLY_BIT
    payload = read_exact(length)
    if payload is None:
        return None
    read_exact(1)  # terminator
    return channel, message_id, payload

def write_packet(channel, message_id, payload):
    if isinstance(payload, str):
        payload = payload.encode()
    mid = message_id | REPLY_BIT
    hdr = struct.pack('>IIIII', MAGIC, 0, channel, mid, len(payload))
    csum = binascii.crc32(hdr + payload) & 0xFFFFFFFF
    hdr = hdr[:4] + struct.pack('>I', csum) + hdr[8:]
    sys.stdout.buffer.write(hdr + payload + TERM)
    sys.stdout.buffer.flush()

# Write a distinctive error to stderr (piped to server.log)
sys.stderr.write("FakeServerError: intentional crash for testing\n")
sys.stderr.flush()

# Complete the version handshake
pkt = read_packet()
if pkt is None:
    sys.exit(1)
channel, message_id, _ = pkt
write_packet(channel, message_id, b"Hegel/0.8")

# Exit immediately — client's run_test receive_reply will fail with ConnectionAborted
sys.exit(1)
"#;

// Same as above but writes nothing to stderr, so server.log is empty.
const FAKE_SERVER_NO_LOG: &str = r#"#!/usr/bin/env python3
import sys
import struct
import binascii

MAGIC = 0x4845474C
REPLY_BIT = 0x80000000
TERM = b'\x0a'

def read_exact(n):
    data = b''
    while len(data) < n:
        chunk = sys.stdin.buffer.read(n - len(data))
        if not chunk:
            return None
        data += chunk
    return data

def read_packet():
    hdr = read_exact(20)
    if hdr is None:
        return None
    channel = struct.unpack('>I', hdr[8:12])[0]
    mid_raw = struct.unpack('>I', hdr[12:16])[0]
    length = struct.unpack('>I', hdr[16:20])[0]
    message_id = mid_raw & ~REPLY_BIT
    payload = read_exact(length)
    if payload is None:
        return None
    read_exact(1)
    return channel, message_id, payload

def write_packet(channel, message_id, payload):
    if isinstance(payload, str):
        payload = payload.encode()
    mid = message_id | REPLY_BIT
    hdr = struct.pack('>IIIII', MAGIC, 0, channel, mid, len(payload))
    csum = binascii.crc32(hdr + payload) & 0xFFFFFFFF
    hdr = hdr[:4] + struct.pack('>I', csum) + hdr[8:]
    sys.stdout.buffer.write(hdr + payload + TERM)
    sys.stdout.buffer.flush()

# No stderr writes — server.log will be empty

# Complete the version handshake
pkt = read_packet()
if pkt is None:
    sys.exit(1)
channel, message_id, _ = pkt
write_packet(channel, message_id, b"Hegel/0.8")

# Exit immediately — client's run_test receive_reply will fail with ConnectionAborted
sys.exit(1)
"#;

const HEGEL_TEST_CODE: &str = r#"
fn main() {
    hegel::hegel(|tc| {
        let _ = tc.draw(hegel::generators::booleans());
    });
}
"#;

fn make_fake_server(script: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let script_path = dir.path().join("fake_server.py");
    std::fs::write(&script_path, script).unwrap();

    // Create a wrapper shell script so we can pass it as HEGEL_SERVER_COMMAND
    let wrapper_path = dir.path().join("server");
    std::fs::write(
        &wrapper_path,
        format!("#!/bin/sh\npython3 {} \"$@\"\n", script_path.display()),
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&wrapper_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&wrapper_path, perms).unwrap();
    }

    (dir, wrapper_path)
}

/// When the server writes to its log before crashing, the error message should
/// include the log content so the user can diagnose the crash.
#[test]
fn test_server_crash_includes_log_content() {
    let (_dir, server_path) = make_fake_server(FAKE_SERVER_WITH_LOG);
    TempRustProject::new()
        .main_file(HEGEL_TEST_CODE)
        .env("HEGEL_SERVER_COMMAND", server_path.to_str().unwrap())
        .expect_failure("FakeServerError: intentional crash for testing")
        .cargo_run(&[]);
}

/// When the server log is empty (server crashed before writing anything), the
/// error message should still explain that the server crashed.
#[test]
fn test_server_crash_empty_log() {
    let (_dir, server_path) = make_fake_server(FAKE_SERVER_NO_LOG);
    TempRustProject::new()
        .main_file(HEGEL_TEST_CODE)
        .env("HEGEL_SERVER_COMMAND", server_path.to_str().unwrap())
        .expect_failure("The hegel server process exited unexpectedly")
        .cargo_run(&[]);
}

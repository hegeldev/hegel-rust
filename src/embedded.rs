use crate::gen::{
    clear_embedded_connection, set_embedded_connection, set_is_last_run, take_generated_values,
};
use serde_json::{json, Value};
use std::backtrace::{Backtrace, BacktraceStatus};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::panic::{self, catch_unwind, AssertUnwindSafe};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Once;
use std::time::Duration;
use tempfile::TempDir;

static PANIC_HOOK_INIT: Once = Once::new();

thread_local! {
    /// Stores panic info captured by our panic hook: (thread_name, thread_id, location, backtrace)
    static LAST_PANIC_INFO: RefCell<Option<(String, String, String, Backtrace)>> = const { RefCell::new(None) };
}

/// Get and clear the last panic info (thread_name, thread_id, location, backtrace).
fn take_panic_info() -> Option<(String, String, String, Backtrace)> {
    LAST_PANIC_INFO.with(|info| info.borrow_mut().take())
}

/// Format a backtrace, optionally filtering to "short" format.
///
/// Short format shows only frames between `__rust_end_short_backtrace` and
/// `__rust_begin_short_backtrace` markers, matching the default Rust panic handler.
/// Frame numbers are renumbered to start at 0.
fn format_backtrace(bt: &Backtrace, full: bool) -> String {
    let backtrace_str = format!("{}", bt);

    if full {
        return backtrace_str;
    }

    // Filter to short backtrace: keep lines between the markers
    // Frame groups look like:
    //    N: function::name
    //              at /path/to/file.rs:123:45
    let lines: Vec<&str> = backtrace_str.lines().collect();
    let mut start_idx = 0;
    let mut end_idx = lines.len();

    for (i, line) in lines.iter().enumerate() {
        if line.contains("__rust_end_short_backtrace") {
            // Skip past this frame (find the next frame number)
            for (j, next_line) in lines.iter().enumerate().skip(i + 1) {
                if next_line
                    .trim_start()
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                {
                    start_idx = j;
                    break;
                }
            }
        }
        if line.contains("__rust_begin_short_backtrace") {
            // Find the start of this frame (the line with the frame number)
            for (j, prev_line) in lines
                .iter()
                .enumerate()
                .take(i + 1)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                if prev_line
                    .trim_start()
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                {
                    end_idx = j;
                    break;
                }
            }
            break;
        }
    }

    // Renumber frames starting at 0
    let filtered: Vec<&str> = lines[start_idx..end_idx].to_vec();
    let mut new_frame_num = 0usize;
    let mut result = Vec::new();

    for line in filtered {
        let trimmed = line.trim_start();
        if trimmed
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            // This is a frame number line like "   8: function_name"
            // Find where the number ends (at the colon)
            if let Some(colon_pos) = trimmed.find(':') {
                let rest = &trimmed[colon_pos..];
                // Preserve original indentation style (right-aligned numbers)
                result.push(format!("{:>4}{}", new_frame_num, rest));
                new_frame_num += 1;
            } else {
                result.push(line.to_string());
            }
        } else {
            result.push(line.to_string());
        }
    }

    result.join("\n")
}

// Panic unconditionally prints to stderr, even if it's caught later. This results in
// messy output during shrinking. To avoid this, we replace the panic hook with our
// own that suppresses the printing except for the final replay.
//
// This is called once per process, the first time any hegel test runs.
fn init_panic_hook() {
    PANIC_HOOK_INIT.call_once(|| {
        panic::set_hook(Box::new(|info| {
            // Capture thread name, ID, and location for later use
            let current = std::thread::current();
            let thread_name = current.name().unwrap_or("<unnamed>").to_string();
            // ThreadId's Debug format is "ThreadId(N)" - extract just the number
            let thread_id = format!("{:?}", current.id())
                .trim_start_matches("ThreadId(")
                .trim_end_matches(')')
                .to_string();
            let location = info
                .location()
                .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
                .unwrap_or_else(|| "<unknown>".to_string());

            // Capture backtrace - will have status Disabled if RUST_BACKTRACE not set
            let backtrace = Backtrace::capture();

            LAST_PANIC_INFO
                .with(|l| *l.borrow_mut() = Some((thread_name, thread_id, location, backtrace)));
            // Don't print anything - we'll format the output ourselves
        }));
    });
}

/// Path to the hegel binary, determined at compile time by build.rs.
/// This will be either a system hegel found on PATH, or one installed
/// into the build directory's cache.
const HEGEL_BINARY_PATH: &str = env!("HEGEL_BINARY_PATH");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verbosity {
    Quiet,
    #[default]
    Normal,
    Verbose,
    Debug,
}

impl Verbosity {
    fn as_str(&self) -> &'static str {
        match self {
            Verbosity::Quiet => "quiet",
            Verbosity::Normal => "normal",
            Verbosity::Verbose => "verbose",
            Verbosity::Debug => "debug",
        }
    }
}

/// Special marker used to identify assume(false) panics.
const REJECT_MARKER: &str = "HEGEL_REJECT";

/// Run property-based tests using Hegel in embedded mode with default options.
///
/// This is a convenience function for simple cases. For configuration options,
/// use [`Hegel::new`] with the builder pattern.
///
/// # Example
///
/// ```no_run
/// use hegel::gen::{self, Generate};
///
/// hegel::hegel(|| {
///     let n = gen::integers::<i32>().generate();
///     assert!(n + 0 == n); // Identity property
/// });
/// ```
pub fn hegel<F>(test_fn: F)
where
    F: FnMut(),
{
    Hegel::new(test_fn).run();
}

/// Builder for running property-based tests with Hegel.
///
/// Use [`Hegel::new`] to create a builder, configure it with method chains,
/// then call [`run`](Hegel::run) to execute the tests.
///
/// # Example
///
/// ```no_run
/// use hegel::{Hegel, Verbosity, gen::{self, Generate}};
///
/// Hegel::new(|| {
///     let n = gen::integers::<i32>().generate();
///     assert!(n + 0 == n);
/// })
/// .test_cases(500)
/// .verbosity(Verbosity::Verbose)
/// .run();
/// ```
pub struct Hegel<F> {
    test_fn: F,
    test_cases: Option<u64>,
    verbosity: Verbosity,
    hegel_path: Option<String>,
}

impl<F> Hegel<F>
where
    F: FnMut(),
{
    /// Create a new Hegel test runner with the given test function.
    pub fn new(test_fn: F) -> Self {
        Self {
            test_fn,
            test_cases: None,
            verbosity: Verbosity::Normal,
            hegel_path: None,
        }
    }

    /// Set the number of test cases to run. Default: 100.
    pub fn test_cases(mut self, n: u64) -> Self {
        self.test_cases = Some(n);
        self
    }

    /// Set the verbosity level. Default: Normal.
    pub fn verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = verbosity;
        self
    }

    /// Set the path to the hegel binary. Default: auto-detected at compile time.
    pub fn hegel_path(mut self, path: impl Into<String>) -> Self {
        self.hegel_path = Some(path.into());
        self
    }

    /// Run the property-based tests.
    ///
    /// This function:
    /// 1. Creates a Unix socket server
    /// 2. Spawns the hegel CLI as a subprocess
    /// 3. Accepts connections from hegel (one per test case)
    /// 4. Runs the test function for each test case
    /// 5. Reports results back to hegel
    /// 6. Panics if any test case fails
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - Failed to create socket or spawn hegel
    /// - Any test case fails (after shrinking)
    /// - Socket communication errors
    pub fn run(self) {
        init_panic_hook();

        // Create temp directory with socket
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let socket_path = temp_dir.path().join("hegel.sock");

        // Create Unix socket server
        let listener = UnixListener::bind(&socket_path).expect("Failed to bind socket");

        // Set non-blocking so we can check if hegel exited
        listener
            .set_nonblocking(true)
            .expect("Failed to set non-blocking");

        // Build hegel command
        let hegel_path = self.hegel_path.as_deref().unwrap_or(HEGEL_BINARY_PATH);
        let mut cmd = Command::new(hegel_path);
        cmd.arg("--client-mode")
            .arg(&socket_path)
            .arg("--no-tui")
            .arg("--verbosity")
            .arg(self.verbosity.as_str());

        let test_cases = self.test_cases.unwrap_or(100);
        cmd.arg("--test-cases").arg(test_cases.to_string());

        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());

        let mut child = cmd.spawn().expect("Failed to spawn hegel");

        // Accept connections until hegel exits
        let mut test_fn = self.test_fn;
        let verbosity = self.verbosity;

        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    handle_connection(stream, &mut test_fn, verbosity);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No connection ready, check if hegel exited
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            handle_exit(status);
                            break;
                        }
                        Ok(None) => {
                            // Hegel still running, wait a bit
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(e) => panic!("Error waiting for hegel: {}", e),
                    }
                }
                Err(e) => panic!("Accept failed: {}", e),
            }
        }
    }
}

/// Handle the final exit status from hegel.
fn handle_exit(status: ExitStatus) {
    if !status.success() {
        // Hegel found a failure
        if let Some(code) = status.code() {
            panic!("Hegel test failed (exit code {})", code);
        } else {
            panic!("Hegel terminated by signal");
        }
    }
}

/// Handle a single connection from hegel (one test case).
fn handle_connection<F: FnMut()>(stream: UnixStream, test_fn: &mut F, verbosity: Verbosity) {
    // Stream accepted from non-blocking listener may inherit non-blocking mode on macOS.
    // Set it back to blocking for reliable reads.
    stream
        .set_nonblocking(false)
        .expect("Failed to set stream to blocking mode");

    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream.try_clone().unwrap();

    // Read handshake
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return; // Connection closed
    }

    let handshake: Value = match serde_json::from_str(&line) {
        Ok(v) => v,
        Err(_) => return, // Invalid handshake
    };

    let is_last = handshake
        .get("is_last_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if verbosity == Verbosity::Debug {
        eprintln!("Handshake received: is_last_run={}", is_last);
    }

    // Set thread-local state
    set_is_last_run(is_last);
    set_embedded_connection(stream);

    // Send handshake_ack
    let ack = json!({"type": "handshake_ack"});
    if writeln!(writer, "{}", ack).is_err() {
        clear_embedded_connection();
        return;
    }
    let _ = writer.flush();

    // Run test in catch_unwind
    let result = catch_unwind(AssertUnwindSafe(test_fn));

    // Clear connection before sending result (test is done generating)
    clear_embedded_connection();

    // Send test result
    let result_msg = match result {
        Ok(()) => json!({"type": "test_result", "result": "pass"}),
        Err(e) => {
            // Check if this is an assume(false) panic
            let msg = panic_message(&e);
            if msg == REJECT_MARKER {
                json!({
                    "type": "test_result",
                    "result": "reject"
                })
            } else {
                if is_last {
                    let (thread_name, thread_id, location, backtrace) =
                        take_panic_info().expect("panic hook should have captured info");
                    eprintln!(
                        "thread '{}' ({}) panicked at {}:",
                        thread_name, thread_id, location
                    );
                    eprintln!("{}", msg);

                    for value in take_generated_values() {
                        eprintln!("{}", value);
                    }

                    if backtrace.status() == BacktraceStatus::Captured {
                        let is_full = std::env::var("RUST_BACKTRACE")
                            .map(|v| v == "full")
                            .unwrap_or(false);
                        let formatted = format_backtrace(&backtrace, is_full);
                        eprintln!("stack backtrace:\n{}", formatted);
                        if !is_full {
                            eprintln!("note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.");
                        }
                    }
                }

                json!({
                    "type": "test_result",
                    "result": "fail",
                    "message": msg
                })
            }
        }
    };

    if verbosity == Verbosity::Debug {
        eprintln!("Sending result: {}", result_msg);
    }

    let _ = writeln!(writer, "{}", result_msg);
    let _ = writer.flush();
}

/// Extract a message from a panic payload.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}

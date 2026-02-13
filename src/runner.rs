use crate::gen::{
    clear_connection, set_connection, set_is_last_run, take_generated_values, CONNECTION,
    TEST_ABORTED,
};
use crate::protocol::{Channel, Connection, VERSION_NEGOTIATION_MESSAGE, VERSION_NEGOTIATION_OK};
use ciborium::Value;

use crate::cbor_helpers::{as_bool, as_text, as_u64, cbor_map, map_get};
use std::backtrace::{Backtrace, BacktraceStatus};
use std::cell::RefCell;
use std::os::unix::net::UnixStream;
use std::panic::{self, catch_unwind, AssertUnwindSafe};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once};
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

/// Run property-based tests using Hegel with default options.
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
    /// 2. Spawns the hegeld as a subprocess
    /// 3. Accepts the connection from hegeld
    /// 4. Handles test case events from hegeld
    /// 5. Runs the test function for each test case
    /// 6. Reports results back to hegeld
    /// 7. Panics if any test case fails
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - Failed to create socket or spawn hegel
    /// - Any test case fails (after shrinking)
    /// - Socket communication errors
    pub fn run(self) {
        // Create temp directory with socket path
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let socket_path = temp_dir.path().join("hegel.sock");

        // Build hegel command - hegeld will bind to the socket and listen
        let hegel_path = self.hegel_path.as_deref().unwrap_or(HEGEL_BINARY_PATH);
        let mut cmd = Command::new(hegel_path);
        cmd.arg(&socket_path)
            .arg("--verbosity")
            .arg(self.verbosity.as_str());

        let test_cases = self.test_cases.unwrap_or(100);
        cmd.arg("--test-cases").arg(test_cases.to_string());

        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());

        if self.verbosity == Verbosity::Debug {
            eprintln!("Starting hegeld: {:?}", cmd);
        }

        // Nonsense warning: string allocation is insignificant next to spawning a process
        #[allow(clippy::expect_fun_call)]
        let mut child = cmd
            .spawn()
            .expect(format!("Failed to spawn hegel at path {}", hegel_path).as_str());

        init_panic_hook();

        // Wait for hegeld to create the socket and start listening
        let mut attempts = 0;
        let stream = loop {
            if socket_path.exists() {
                match UnixStream::connect(&socket_path) {
                    Ok(stream) => break stream,
                    Err(_) if attempts < 50 => {
                        // Socket exists but not yet listening
                        std::thread::sleep(Duration::from_millis(100));
                        attempts += 1;
                        continue;
                    }
                    Err(e) => {
                        let _ = child.kill();
                        panic!("Failed to connect to hegeld socket: {}", e);
                    }
                }
            }
            if attempts >= 50 {
                let _ = child.kill();
                panic!("Timeout waiting for hegeld to create socket");
            }
            std::thread::sleep(Duration::from_millis(100));
            attempts += 1;
        };

        // Set a read timeout so the SDK doesn't hang forever if the server
        // crashes without sending a response (e.g. Python IndexError in
        // send_response_error for UnsatisfiedAssumption).
        stream.set_read_timeout(Some(Duration::from_secs(120))).ok();

        // Create connection and perform version negotiation
        let connection = Connection::new(stream);

        // Initiate version negotiation (SDK is the client)
        let control = connection.control_channel();
        let req_id = control
            .send_request(VERSION_NEGOTIATION_MESSAGE.to_vec())
            .expect("Failed to send version negotiation");
        let response = control
            .receive_response(req_id)
            .expect("Failed to receive version response");

        if response != VERSION_NEGOTIATION_OK {
            let _ = child.kill();
            panic!(
                "Version negotiation failed: {:?}",
                String::from_utf8_lossy(&response)
            );
        }

        if self.verbosity == Verbosity::Debug {
            eprintln!("Version negotiation complete");
        }

        // Run the test
        let mut test_fn = self.test_fn;
        let verbosity = self.verbosity;
        let got_interesting = Arc::new(AtomicBool::new(false));

        // Create a test channel for receiving test_case/test_done events
        let test_channel = connection.new_channel();

        // Send run_test request with the test channel ID
        let run_test_msg = cbor_map! {
            "command" => "run_test",
            "name" => "test",
            "test_cases" => test_cases,
            "channel" => test_channel.channel_id
        };

        let run_test_id = control
            .send_request(cbor_encode(&run_test_msg))
            .expect("Failed to send run_test");

        // Wait for run_test response on control channel (just True, verifies no error)
        let run_test_response = control
            .receive_response(run_test_id)
            .expect("Failed to receive run_test response");
        let _run_test_result: Value = cbor_decode(&run_test_response);

        if verbosity == Verbosity::Debug {
            eprintln!("run_test response received");
        }

        // Handle test_case events on the test channel until test_done
        let result_data: Value;
        let ack_null = cbor_map! {"result" => Value::Null};
        loop {
            let (event_id, event_payload) = test_channel
                .receive_request()
                .expect("Failed to receive event");

            let event: Value = cbor_decode(&event_payload);
            let event_type = map_get(&event, "event").and_then(as_text);

            if verbosity == Verbosity::Debug {
                eprintln!("Received event: {:?}", event);
            }

            match event_type {
                Some("test_case") => {
                    let channel_id = map_get(&event, "channel")
                        .and_then(as_u64)
                        .expect("Missing channel id") as u32;

                    let test_case_channel = connection.connect_channel(channel_id);

                    // Ack the test_case event BEFORE running the test (prevents deadlock)
                    test_channel
                        .send_response(event_id, cbor_encode(&ack_null))
                        .expect("Failed to ack test_case");

                    run_test_case(
                        &connection,
                        test_case_channel,
                        &mut test_fn,
                        false,
                        verbosity,
                        &got_interesting,
                    );
                }
                Some("test_done") => {
                    // Ack the test_done event
                    let ack_true = cbor_map! {"result" => true};
                    test_channel
                        .send_response(event_id, cbor_encode(&ack_true))
                        .expect("Failed to ack test_done");
                    result_data = map_get(&event, "results").cloned().unwrap_or(Value::Null);
                    break;
                }
                _ => {
                    // Unknown event, just ack it
                    test_channel
                        .send_response(event_id, cbor_encode(&ack_null))
                        .expect("Failed to ack event");
                }
            }
        }

        let n_interesting = map_get(&result_data, "interesting_test_cases")
            .and_then(as_u64)
            .unwrap_or(0);

        if verbosity == Verbosity::Debug {
            eprintln!("Test done. interesting_test_cases={}", n_interesting);
        }

        // Process final replay test cases (one per interesting example)
        for _ in 0..n_interesting {
            let (event_id, event_payload) = test_channel
                .receive_request()
                .expect("Failed to receive final test_case");

            let event: Value = cbor_decode(&event_payload);
            let event_type = map_get(&event, "event").and_then(as_text);
            assert_eq!(event_type, Some("test_case"));

            let channel_id = map_get(&event, "channel")
                .and_then(as_u64)
                .expect("Missing channel id") as u32;

            let test_case_channel = connection.connect_channel(channel_id);

            // Ack before running
            test_channel
                .send_response(event_id, cbor_encode(&ack_null))
                .expect("Failed to ack final test_case");

            run_test_case(
                &connection,
                test_case_channel,
                &mut test_fn,
                true,
                verbosity,
                &got_interesting,
            );
        }

        let passed = map_get(&result_data, "passed")
            .and_then(as_bool)
            .unwrap_or(true);

        // Close the connection so hegeld can exit gracefully
        drop(test_channel);
        drop(control);
        let _ = connection.close();
        drop(connection);

        // Wait for hegeld to exit
        let _ = child.wait().expect("Failed to wait for hegel");

        if !passed || got_interesting.load(Ordering::SeqCst) {
            panic!("Property test failed");
        }
    }
}

/// Run a single test case.
fn run_test_case<F: FnMut()>(
    connection: &Arc<Connection>,
    test_channel: Channel,
    test_fn: &mut F,
    is_final: bool,
    _verbosity: Verbosity,
    got_interesting: &Arc<AtomicBool>,
) {
    // Set thread-local state for this test case
    // Note: we pass the channel directly (not cloned) so generators and mark_complete
    // share the same message ID sequence.
    set_is_last_run(is_final);
    set_connection(Arc::clone(connection), test_channel);

    // Run test in catch_unwind
    let result = catch_unwind(AssertUnwindSafe(test_fn));

    // Determine status and origin from result
    let (status, origin) = match &result {
        Ok(()) => ("VALID".to_string(), None),
        Err(e) => {
            // Check if this is an assume(false) panic
            let msg = panic_message(e);
            if msg == REJECT_MARKER {
                ("INVALID".to_string(), None)
            } else {
                got_interesting.store(true, Ordering::SeqCst);

                // Take panic info - we need location for origin, and print details on final
                let (thread_name, thread_id, location, backtrace) = take_panic_info()
                    .unwrap_or_else(|| {
                        (
                            "<unknown>".to_string(),
                            "?".to_string(),
                            "<unknown>".to_string(),
                            Backtrace::disabled(),
                        )
                    });

                if is_final {
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

                // Origin is a string matching Python's _extract_origin format
                let origin = format!("Panic at {}", location);

                ("INTERESTING".to_string(), Some(origin))
            }
        }
    };

    // Send mark_complete using the same channel that generators used.
    // Skip if test was aborted (StopTest) - server already closed the channel.
    let was_aborted = TEST_ABORTED.with(|aborted| aborted.replace(false));
    if !was_aborted {
        CONNECTION.with(|conn| {
            if let Some(state) = conn.borrow_mut().as_mut() {
                let origin_value = match &origin {
                    Some(s) => Value::Text(s.clone()),
                    None => Value::Null,
                };
                let mark_complete = cbor_map! {
                    "command" => "mark_complete",
                    "status" => status.as_str(),
                    "origin" => origin_value
                };
                // Wait for server to acknowledge mark_complete before closing
                let _ = state.channel.request_cbor(&mark_complete);
                // Close the test case channel
                let _ = state.channel.close();
            }
        });
    }

    // Clear connection after mark_complete is sent (or skipped)
    clear_connection();
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

/// Encode a ciborium::Value to CBOR bytes.
fn cbor_encode(value: &Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).expect("CBOR encoding failed");
    bytes
}

/// Decode CBOR bytes to a ciborium::Value.
fn cbor_decode(bytes: &[u8]) -> Value {
    ciborium::from_reader(bytes).expect("CBOR decoding failed")
}

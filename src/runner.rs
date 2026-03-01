use crate::generators::{TestCaseData, TEST_CASE_DATA};
use crate::protocol::{Channel, Connection, HANDSHAKE_STRING, SUPPORTED_PROTOCOL_VERSIONS};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Quiet,
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

pub(crate) const ASSUME_FAIL_STRING: &str = "__HEGEL_ASSUME_FAIL";

/// Run property-based tests using Hegel with default options.
///
/// This is a convenience function for simple cases. For configuration options,
/// use [`Hegel::new`] with the builder pattern.
///
/// # Example
///
/// ```no_run
/// use hegel::generators;
///
/// hegel::hegel(|| {
///     let n = hegel::draw(&generators::integers::<i32>());
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
/// use hegel::{Hegel, Verbosity, generators};
///
/// Hegel::new(|| {
///     let n = hegel::draw(&generators::integers::<i32>());
///     assert!(n + 0 == n);
/// })
/// .test_cases(500)
/// .verbosity(Verbosity::Verbose)
/// .run();
/// ```
pub struct Hegel<F> {
    test_fn: F,
    test_cases: u64,
    verbosity: Verbosity,
    seed: Option<u64>,
}

impl<F> Hegel<F>
where
    F: FnMut(),
{
    /// Create a new Hegel test runner with the given test function.
    pub fn new(test_fn: F) -> Self {
        Self {
            test_fn,
            test_cases: 100,
            verbosity: Verbosity::Normal,
            seed: None,
        }
    }

    /// Set the number of test cases to run. Default: 100.
    pub fn test_cases(mut self, n: u64) -> Self {
        self.test_cases = n;
        self
    }

    /// Set the verbosity level. Default: Normal.
    pub fn verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = verbosity;
        self
    }

    pub fn seed(mut self, seed: Option<u64>) -> Self {
        self.seed = seed;
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

        let mut cmd = Command::new(HEGEL_BINARY_PATH);
        cmd.arg(&socket_path)
            .arg("--verbosity")
            .arg(self.verbosity.as_str());

        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());

        if self.verbosity == Verbosity::Debug {
            eprintln!("Starting hegeld: {:?}", cmd);
        }

        // Nonsense warning: string allocation is insignificant next to spawning a process
        #[allow(clippy::expect_fun_call)]
        let mut child = cmd
            .spawn()
            .expect(format!("Failed to spawn hegel at path {}", HEGEL_BINARY_PATH).as_str());

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
        let (lo, hi) = SUPPORTED_PROTOCOL_VERSIONS;
        let control = connection.control_channel();
        let req_id = control
            .send_request(HANDSHAKE_STRING.to_vec())
            .expect("Failed to send version negotiation");
        let response = control
            .receive_reply(req_id)
            .expect("Failed to receive version response");

        let decoded = String::from_utf8_lossy(&response);
        let server_version = match decoded.strip_prefix("Hegel/") {
            Some(v) => v,
            None => {
                let _ = child.kill();
                panic!("Bad handshake response: {decoded:?}");
            }
        };
        let v: f64 = server_version.parse().unwrap_or_else(|_| {
            let _ = child.kill();
            panic!("Bad version number: {server_version}");
        });
        if !(lo <= v && v <= hi) {
            let _ = child.kill();
            panic!(
                "hegel-rust supports protocol versions {lo} through {hi}, but \
                 got server version {v}. Upgrading hegel-rust or downgrading \
                 your hegel cli might help."
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
            "test_cases" => self.test_cases,
            "seed" => self.seed.map_or(Value::Null, Value::from),
            "channel_id" => test_channel.channel_id
        };

        let run_test_id = control
            .send_request(cbor_encode(&run_test_msg))
            .expect("Failed to send run_test");

        // Wait for run_test response on control channel (just True, verifies no error)
        let run_test_response = control
            .receive_reply(run_test_id)
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
                    let channel_id = map_get(&event, "channel_id")
                        .and_then(as_u64)
                        .expect("Missing channel id") as u32;

                    let test_case_channel = connection.connect_channel(channel_id);

                    // Ack the test_case event BEFORE running the test (prevents deadlock)
                    test_channel
                        .write_reply(event_id, cbor_encode(&ack_null))
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
                        .write_reply(event_id, cbor_encode(&ack_true))
                        .expect("Failed to ack test_done");
                    result_data = map_get(&event, "results").cloned().unwrap_or(Value::Null);
                    break;
                }
                _ => {
                    // Unknown event, just ack it
                    test_channel
                        .write_reply(event_id, cbor_encode(&ack_null))
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

            let channel_id = map_get(&event, "channel_id")
                .and_then(as_u64)
                .expect("Missing channel id") as u32;

            let test_case_channel = connection.connect_channel(channel_id);

            // Ack before running
            test_channel
                .write_reply(event_id, cbor_encode(&ack_null))
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
    verbosity: Verbosity,
    got_interesting: &Arc<AtomicBool>,
) {
    // Create TestCaseData on the stack and set thread-local pointer.
    // Note: we pass the channel directly (not cloned) so generators and mark_complete
    // share the same message ID sequence.
    let data = TestCaseData::new(Arc::clone(connection), test_channel, verbosity, is_final);
    TEST_CASE_DATA.with(|c| c.set(&data as *const TestCaseData));

    // Run test in catch_unwind
    let result = catch_unwind(AssertUnwindSafe(test_fn));

    // Determine status and origin from result
    let (status, origin) = match &result {
        Ok(()) => ("VALID".to_string(), None),
        Err(e) => {
            let msg = panic_message(e);
            if msg == ASSUME_FAIL_STRING {
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

                    for value in std::mem::take(&mut *data.output.borrow_mut()) {
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
    let was_aborted = data.test_aborted();
    if !was_aborted {
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
        let _ = data.channel().request_cbor(&mark_complete);
        // Close the test case channel
        let _ = data.channel().close();
    }

    // Clear thread-local pointer
    TEST_CASE_DATA.with(|c| c.set(std::ptr::null()));
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

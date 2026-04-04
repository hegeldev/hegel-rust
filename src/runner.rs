use crate::antithesis::{TestLocation, is_running_in_antithesis};
use crate::control::{currently_in_test_context, with_test_context};
use crate::protocol::{Connection, HANDSHAKE_STRING, SERVER_CRASHED_MESSAGE, Stream};
use crate::settings::{Database, Settings, Verbosity};
use crate::test_case::{ASSUME_FAIL_STRING, STOP_TEST_STRING, TestCase};
use ciborium::Value;

use crate::cbor_utils::{as_bool, as_text, as_u64, cbor_map, map_get};
use std::backtrace::{Backtrace, BacktraceStatus};
use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

const SUPPORTED_PROTOCOL_VERSIONS: (f64, f64) = (0.8, 0.8);
const HEGEL_SERVER_VERSION: &str = "0.3.0";
const HEGEL_SERVER_COMMAND_ENV: &str = "HEGEL_SERVER_COMMAND";
const HEGEL_SERVER_DIR: &str = ".hegel";
static SERVER_LOG_PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static LOG_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
static SESSION: std::sync::OnceLock<HegelSession> = std::sync::OnceLock::new();

static PANIC_HOOK_INIT: Once = Once::new();

/// A persistent connection to the hegel server subprocess.
///
/// Created once per process on first use. The subprocess and connection
/// are reused across all `Hegel::run()` calls. The Python server supports
/// multiple sequential `run_test` commands over a single connection.
struct HegelSession {
    connection: Arc<Connection>,
    /// The control stream is shared across threads, so it's behind a Mutex
    /// because Stream is not thread-safe. The lock is only held for the
    /// brief run_test send/receive; test execution runs concurrently on
    /// per-test streams.
    control: Mutex<Stream>,
}

impl HegelSession {
    fn get() -> &'static HegelSession {
        SESSION.get_or_init(|| {
            init_panic_hook();
            HegelSession::init()
        })
    }

    fn init() -> HegelSession {
        let mut cmd = hegel_command();
        cmd.arg("--stdio").arg("--verbosity").arg("normal");

        cmd.env("PYTHONUNBUFFERED", "1");
        let log_file = server_log_file();
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::from(log_file));

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => panic!("Failed to spawn hegel server: {e}"), // nocov
        };

        let child_stdin = child.stdin.take().expect("Failed to take child stdin");
        let child_stdout = child.stdout.take().expect("Failed to take child stdout");

        let connection = Connection::new(Box::new(child_stdout), Box::new(child_stdin));
        let mut control = connection.control_stream();

        // Derive the binary path before the handshake so it's available for error messages.
        let binary_path = std::env::var(HEGEL_SERVER_COMMAND_ENV).ok();

        // Handshake
        let handshake_result = control
            .send_request(HANDSHAKE_STRING.to_vec())
            .and_then(|req_id| control.receive_reply(req_id));

        let response = match handshake_result {
            Ok(r) => r,
            Err(e) => handle_handshake_failure(&mut child, binary_path.as_deref(), e), // nocov
        };

        let decoded = String::from_utf8_lossy(&response);
        let server_version = match decoded.strip_prefix("Hegel/") {
            Some(v) => v,
            None => {
                let _ = child.kill(); // nocov
                panic!("Bad handshake response: {decoded:?}"); // nocov
            }
        };
        let version: f64 = server_version.parse().unwrap_or_else(|_| {
            let _ = child.kill(); // nocov
            panic!("Bad version number: {server_version}"); // nocov
        });

        let (lo, hi) = SUPPORTED_PROTOCOL_VERSIONS;
        // nocov start
        if !(lo <= version && version <= hi) {
            let _ = child.kill();
            panic!(
                "hegel-rust supports protocol versions {lo} through {hi}, but \
                 the connected server is using protocol version {version}. Upgrading \
                 hegel-rust or downgrading hegel-core might help."
            );
            // nocov end
        }

        // Monitor thread: detects server crash. The pipe close from
        // the child exiting will unblock any pending reads.
        let conn_for_monitor = Arc::clone(&connection);
        std::thread::spawn(move || {
            let _ = child.wait();
            conn_for_monitor.mark_server_exited();
        });

        HegelSession {
            connection,
            control: Mutex::new(control),
        }
    }
}

struct PanicInfo {
    thread_name: String,
    thread_id: String,
    file: String,
    line: u32,
    column: u32,
    backtrace: Backtrace,
}

impl PanicInfo {
    fn location(&self) -> String {
        format!("{}:{}:{}", self.file, self.line, self.column)
    }
}

thread_local! {
    static LAST_PANIC_INFO: RefCell<Option<PanicInfo>> = const { RefCell::new(None) };
}

fn take_panic_info() -> Option<PanicInfo> {
    LAST_PANIC_INFO.with(|info| info.borrow_mut().take())
}

/// Format a backtrace, optionally filtering to "short" format.
///
/// Short format shows only frames between `__rust_end_short_backtrace` and
/// `__rust_begin_short_backtrace` markers, matching the default Rust panic handler.
/// Frame numbers are renumbered to start at 0.
// nocov start
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
// nocov end

// Panic unconditionally prints to stderr, even if it's caught later. This results in
// messy output during shrinking. To avoid this, we replace the panic hook with our
// own that suppresses the printing except for the final replay.
//
// This is called once per process, the first time any hegel test runs.
fn init_panic_hook() {
    PANIC_HOOK_INIT.call_once(|| {
        let prev_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !currently_in_test_context() {
                // use actual panic hook outside of tests
                prev_hook(info);
                return;
            }

            let thread = std::thread::current();
            let thread_name = thread.name().unwrap_or("<unnamed>").to_string();
            // ThreadId's debug output is ThreadId(N)
            let thread_id = format!("{:?}", thread.id())
                .trim_start_matches("ThreadId(")
                .trim_end_matches(')')
                .to_string();
            let loc = info.location().expect(
                "PanicHookInfo.location() returned None. This should never happen - please open an issue!"
            );
            let file = loc.file().to_string();
            let line = loc.line();
            let column = loc.column();
            let backtrace = Backtrace::capture();

            LAST_PANIC_INFO.with(|l| {
                *l.borrow_mut() = Some(PanicInfo {
                    thread_name,
                    thread_id,
                    file,
                    line,
                    column,
                    backtrace,
                })
            });
        }));
    });
}

fn hegel_command() -> Command {
    if let Ok(override_path) = std::env::var(HEGEL_SERVER_COMMAND_ENV) {
        return Command::new(resolve_hegel_path(&override_path)); // nocov
    }
    let uv_path = crate::uv::find_uv();
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

fn server_log_file() -> File {
    std::fs::create_dir_all(HEGEL_SERVER_DIR).ok();
    let pid = std::process::id();
    let ix = LOG_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = format!("{HEGEL_SERVER_DIR}/server.{pid}-{ix}.log");
    SERVER_LOG_PATH.set(path.clone()).ok();
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

fn handle_handshake_failure(
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
    if let Some(log_path) = SERVER_LOG_PATH.get() {
        if let Ok(contents) = std::fs::read_to_string(log_path) {
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
        crate::utils::validate_executable(path);
        return path.to_string();
    }

    // Bare name (no '/') — try PATH lookup
    if !path.contains('/') {
        if let Some(resolved) = crate::utils::which(path) {
            crate::utils::validate_executable(&resolved);
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

// internal use only
#[doc(hidden)]
pub fn hegel<F>(test_fn: F)
where
    F: FnMut(TestCase),
{
    Hegel::new(test_fn).run();
}

// internal use only
#[doc(hidden)]
pub struct Hegel<F> {
    test_fn: F,
    database_key: Option<String>,
    test_location: Option<TestLocation>,
    settings: Settings,
}

impl<F> Hegel<F>
where
    F: FnMut(TestCase),
{
    /// Create a new test builder with default settings.
    pub fn new(test_fn: F) -> Self {
        Self {
            test_fn,
            database_key: None,
            settings: Settings::new(),
            test_location: None,
        }
    }

    /// Override the default settings.
    pub fn settings(mut self, settings: Settings) -> Self {
        self.settings = settings;
        self
    }

    #[doc(hidden)]
    pub fn __database_key(mut self, key: String) -> Self {
        self.database_key = Some(key);
        self
    }

    #[doc(hidden)]
    pub fn test_location(mut self, location: TestLocation) -> Self {
        self.test_location = Some(location);
        self
    }

    /// Run the property-based tests.
    ///
    /// Connects to the shared hegel server (spawning it on first use),
    /// sends a `run_test` command, processes test cases, and reports results.
    /// Panics if any test case fails.
    pub fn run(self) {
        let session = HegelSession::get();
        let connection = &session.connection;

        let mut test_fn = self.test_fn;
        let verbosity = self.settings.verbosity;
        let got_interesting = Arc::new(AtomicBool::new(false));
        let mut test_stream = connection.new_stream();

        let suppress_names: Vec<Value> = self
            .settings
            .suppress_health_check
            .iter()
            .map(|c| Value::Text(c.as_str().to_string()))
            .collect();

        let database_key_bytes = self
            .database_key
            .map_or(Value::Null, |k| Value::Bytes(k.into_bytes()));

        let mut run_test_msg = cbor_map! {
            "command" => "run_test",
            "test_cases" => self.settings.test_cases,
            "seed" => self.settings.seed.map_or(Value::Null, Value::from),
            "stream_id" => test_stream.stream_id,
            "database_key" => database_key_bytes,
            "derandomize" => self.settings.derandomize
        };
        let db_value = match &self.settings.database {
            Database::Unset => Option::None, // nocov
            Database::Disabled => Some(Value::Null),
            Database::Path(s) => Some(Value::Text(s.clone())),
        };
        if let Some(db) = db_value {
            if let Value::Map(ref mut map) = run_test_msg {
                map.push((Value::Text("database".to_string()), db));
            }
        }
        if !suppress_names.is_empty() {
            if let Value::Map(ref mut map) = run_test_msg {
                map.push((
                    Value::Text("suppress_health_check".to_string()),
                    Value::Array(suppress_names),
                ));
            }
        }

        // The control stream is behind a Mutex because Stream requires &mut self.
        // This only serializes the brief run_test send/receive — actual test
        // execution happens on per-test streams without holding this lock.
        {
            let mut control = session.control.lock().unwrap();
            let run_test_id = control
                .send_request(cbor_encode(&run_test_msg))
                .expect("Failed to send run_test");

            let run_test_response = control
                .receive_reply(run_test_id)
                .expect("Failed to receive run_test response");
            let _run_test_result: Value = cbor_decode(&run_test_response);
        }

        if verbosity == Verbosity::Debug {
            eprintln!("run_test response received"); // nocov
        }

        let result_data: Value;
        let ack_null = cbor_map! {"result" => Value::Null};
        loop {
            // Handle the server dying between events: receive_request will
            // fail with RecvError once the background reader clears the senders.
            let (event_id, event_payload) = match test_stream.receive_request() {
                Ok(event) => event,
                // nocov start
                Err(_) if connection.server_has_exited() => {
                    panic!("{}", SERVER_CRASHED_MESSAGE);
                    // nocov end
                }
                Err(e) => unreachable!("Failed to receive event (server still running): {}", e),
            };

            let event: Value = cbor_decode(&event_payload);
            let event_type = map_get(&event, "event")
                .and_then(as_text)
                .expect("Expected event in payload");

            if verbosity == Verbosity::Debug {
                eprintln!("Received event: {:?}", event); // nocov
            }

            match event_type {
                "test_case" => {
                    let stream_id = map_get(&event, "stream_id")
                        .and_then(as_u64)
                        .expect("Missing stream id") as u32;

                    let test_case_stream = connection.connect_stream(stream_id);

                    // Ack the test_case event BEFORE running the test (prevents deadlock)
                    test_stream
                        .write_reply(event_id, cbor_encode(&ack_null))
                        .expect("Failed to ack test_case");

                    let tc_result = run_test_case(
                        connection,
                        test_case_stream,
                        &mut test_fn,
                        false,
                        verbosity,
                        &got_interesting,
                    );
                    if let TestCaseResult::InternalError {
                        panic_message,
                        panic_info,
                    } = tc_result
                    {
                        let mut msg = format!(
                            "hegel internal error at {}:\n{}\n",
                            panic_info.location(),
                            panic_message,
                        );
                        if panic_info.backtrace.status() == BacktraceStatus::Captured {
                            let is_full = std::env::var("RUST_BACKTRACE")
                                .map(|v| v == "full")
                                .unwrap_or(false);
                            msg.push_str(&format!(
                                "\noriginal backtrace:\n{}\n",
                                format_backtrace(&panic_info.backtrace, is_full),
                            ));
                        }
                        panic!("{}", msg);
                    }
                }
                "test_done" => {
                    let ack_true = cbor_map! {"result" => true};
                    test_stream
                        .write_reply(event_id, cbor_encode(&ack_true))
                        .expect("Failed to ack test_done");
                    result_data = map_get(&event, "results").cloned().unwrap_or(Value::Null);
                    break;
                }
                _ => {
                    panic!("unknown event: {}", event_type); // nocov
                }
            }
        }

        // Check for server-side errors before processing results
        if let Some(error_msg) = map_get(&result_data, "error").and_then(as_text) {
            panic!("Server error: {}", error_msg); // nocov
        }

        // Check for health check failure before processing results
        if let Some(failure_msg) = map_get(&result_data, "health_check_failure").and_then(as_text) {
            panic!("Health check failure:\n{}", failure_msg); // nocov
        }

        // Check for flaky test detection
        if let Some(flaky_msg) = map_get(&result_data, "flaky").and_then(as_text) {
            panic!("Flaky test detected: {}", flaky_msg); // nocov
        }

        let n_interesting = map_get(&result_data, "interesting_test_cases")
            .and_then(as_u64)
            .unwrap_or(0);

        if verbosity == Verbosity::Debug {
            eprintln!("Test done. interesting_test_cases={}", n_interesting); // nocov
        }

        // Process final replay test cases (one per interesting example)
        let mut final_result: Option<TestCaseResult> = None;
        for _ in 0..n_interesting {
            let (event_id, event_payload) = test_stream
                .receive_request()
                .expect("Failed to receive final test_case");

            let event: Value = cbor_decode(&event_payload);
            let event_type = map_get(&event, "event").and_then(as_text);
            assert_eq!(event_type, Some("test_case"));

            let stream_id = map_get(&event, "stream_id")
                .and_then(as_u64)
                .expect("Missing stream id") as u32;

            let test_case_stream = connection.connect_stream(stream_id);

            test_stream
                .write_reply(event_id, cbor_encode(&ack_null))
                .expect("Failed to ack final test_case");

            let tc_result = run_test_case(
                connection,
                test_case_stream,
                &mut test_fn,
                true,
                verbosity,
                &got_interesting,
            );

            if matches!(&tc_result, TestCaseResult::Interesting { .. }) {
                final_result = Some(tc_result);
            }

            if connection.server_has_exited() {
                panic!("{}", SERVER_CRASHED_MESSAGE); // nocov
            }
        }

        let passed = map_get(&result_data, "passed")
            .and_then(as_bool)
            .unwrap_or(true);

        let test_failed = !passed || got_interesting.load(Ordering::SeqCst);

        if is_running_in_antithesis() {
            #[cfg(not(feature = "antithesis"))]
            panic!(
                // nocov
                "When Hegel is run inside of Antithesis, it requires the `antithesis` feature. \
                You can add it with {{ features = [\"antithesis\"] }}."
            );

            #[cfg(feature = "antithesis")]
            // nocov start
            if let Some(ref loc) = self.test_location {
                crate::antithesis::emit_assertion(loc, !test_failed);
                // nocov end
            }
        }

        if test_failed {
            let msg = match &final_result {
                Some(TestCaseResult::Interesting { panic_message }) => panic_message.as_str(),
                _ => "unknown", // nocov
            };
            panic!("Property test failed: {}", msg);
        }
    }
}

enum TestCaseResult {
    Valid,
    Invalid,
    Interesting {
        panic_message: String,
    },
    InternalError {
        panic_message: String,
        panic_info: PanicInfo,
    },
}

fn run_test_case<F: FnMut(TestCase)>(
    connection: &Arc<Connection>,
    test_stream: Stream,
    test_fn: &mut F,
    is_final: bool,
    verbosity: Verbosity,
    got_interesting: &Arc<AtomicBool>,
) -> TestCaseResult {
    // Create TestCase. The test function gets a clone (cheap Rc bump),
    // so we retain access to the same underlying TestCaseData after the test runs.
    let tc = TestCase::new(Arc::clone(connection), test_stream, verbosity, is_final);

    let result = with_test_context(|| catch_unwind(AssertUnwindSafe(|| test_fn(tc.clone()))));

    let (tc_result, origin) = match &result {
        Ok(()) => (TestCaseResult::Valid, None),
        Err(e) => {
            let msg = panic_message(e);
            if msg == ASSUME_FAIL_STRING || msg == STOP_TEST_STRING {
                (TestCaseResult::Invalid, None)
            } else {
                let panic_info = take_panic_info()
                    // nocov start
                    .expect(
                        "Expected panic info, but got None. This should never happen - please open an issue!"
                    );
                // nocov end

                // immediately propagate internal errors
                if crate::utils::is_hegel_file(&panic_info.file) {
                    return TestCaseResult::InternalError {
                        panic_message: msg,
                        panic_info,
                    };
                }

                got_interesting.store(true, Ordering::SeqCst);

                if is_final {
                    eprintln!(
                        "thread '{}' ({}) panicked at {}:",
                        panic_info.thread_name,
                        panic_info.thread_id,
                        panic_info.location()
                    );
                    eprintln!("{}", msg);

                    // nocov start
                    if panic_info.backtrace.status() == BacktraceStatus::Captured {
                        let is_full = std::env::var("RUST_BACKTRACE")
                            .map(|v| v == "full")
                            .unwrap_or(false);
                        let formatted = format_backtrace(&panic_info.backtrace, is_full);
                        eprintln!("stack backtrace:\n{}", formatted);
                        if !is_full {
                            eprintln!(
                                "note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace."
                            );
                        }
                    }
                    // nocov end
                }

                let origin = format!("Panic at {}", panic_info.location());
                (
                    TestCaseResult::Interesting { panic_message: msg },
                    Some(origin),
                )
            }
        }
    };

    // Send mark_complete using the same stream that generators used.
    // Skip if test was aborted (StopTest) - server already closed the stream.
    if !tc.test_aborted() {
        let status = match &tc_result {
            TestCaseResult::Valid => "VALID",
            TestCaseResult::Invalid => "INVALID",
            TestCaseResult::Interesting { .. } => "INTERESTING",
            TestCaseResult::InternalError { .. } => unreachable!(),
        };
        let origin_value = match &origin {
            Some(s) => Value::Text(s.clone()),
            None => Value::Null,
        };
        let mark_complete = cbor_map! {
            "command" => "mark_complete",
            "status" => status,
            "origin" => origin_value
        };
        tc.send_mark_complete(&mark_complete);
    }

    tc_result
}

/// Extract a message from a panic payload.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string() // nocov
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

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;

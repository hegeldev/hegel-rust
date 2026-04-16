use crate::antithesis::{TestLocation, is_running_in_antithesis};
use crate::backend::{DataSource, DataSourceError, TestCaseResult, TestRunResult, TestRunner};
use crate::cbor_utils::{as_bool, as_text, as_u64, cbor_map, map_get, map_insert};
use crate::control::{currently_in_test_context, with_test_context};
use crate::protocol::{Connection, HANDSHAKE_STRING, Stream};
use crate::settings::{Database, Settings, Verbosity};
use crate::test_case::{ASSUME_FAIL_STRING, STOP_TEST_STRING, TestCase};
use ciborium::Value;

use std::backtrace::{Backtrace, BacktraceStatus};
use std::cell::{Cell, RefCell};
use std::fs::{File, OpenOptions};
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Once};
use std::time::{Duration, Instant};

const SUPPORTED_PROTOCOL_VERSIONS: (&str, &str) = ("0.10", "0.10");
const HEGEL_SERVER_VERSION: &str = "0.4.2";
const HEGEL_SERVER_COMMAND_ENV: &str = "HEGEL_SERVER_COMMAND";
const HEGEL_SERVER_DIR: &str = ".hegel";
static SERVER_LOG_PATH: Mutex<Option<String>> = Mutex::new(None);
static LOG_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
static SESSION: Mutex<Option<Arc<HegelSession>>> = Mutex::new(None);

static PANIC_HOOK_INIT: Once = Once::new();

// ─── ServerDataSource ──────────────────────────────────────────────────────────

static PROTOCOL_DEBUG: LazyLock<bool> = LazyLock::new(|| {
    matches!(
        std::env::var("HEGEL_PROTOCOL_DEBUG")
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "1" | "true"
    )
});

/// Backend implementation that communicates with the hegel-core server
/// over a multiplexed stream.
pub(crate) struct ServerDataSource {
    connection: Arc<Connection>,
    stream: RefCell<Stream>,
    aborted: Cell<bool>,
    verbosity: Verbosity,
}

impl ServerDataSource {
    pub(crate) fn new(connection: Arc<Connection>, stream: Stream, verbosity: Verbosity) -> Self {
        ServerDataSource {
            connection,
            stream: RefCell::new(stream),
            aborted: Cell::new(false),
            verbosity,
        }
    }

    fn send_request(&self, command: &str, payload: &Value) -> Result<Value, DataSourceError> {
        if self.aborted.get() {
            return Err(DataSourceError::StopTest);
        }
        let debug = *PROTOCOL_DEBUG || self.verbosity == Verbosity::Debug;

        let mut entries = vec![(
            Value::Text("command".to_string()),
            Value::Text(command.to_string()),
        )];

        if let Value::Map(map) = payload {
            for (k, v) in map {
                entries.push((k.clone(), v.clone()));
            }
        }

        let request = Value::Map(entries);

        if debug {
            eprintln!("REQUEST: {:?}", request);
        }

        let result = self.stream.borrow_mut().request_cbor(&request);

        match result {
            Ok(response) => {
                if debug {
                    eprintln!("RESPONSE: {:?}", response);
                }
                Ok(response)
            }
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("UnsatisfiedAssumption") {
                    // nocov start
                    if debug {
                        eprintln!("RESPONSE: UnsatisfiedAssumption");
                    }
                    Err(DataSourceError::Assume)
                    // nocov end
                } else if error_msg.contains("overflow")
                    || error_msg.contains("StopTest")
                    || error_msg.contains("stream is closed")
                {
                    if debug {
                        eprintln!("RESPONSE: StopTest/overflow"); // nocov
                    }
                    self.stream.borrow_mut().mark_closed();
                    self.aborted.set(true);
                    Err(DataSourceError::StopTest)
                // nocov start
                } else if error_msg.contains("FlakyStrategyDefinition")
                    || error_msg.contains("FlakyReplay")
                // nocov end
                {
                    self.stream.borrow_mut().mark_closed();
                    self.aborted.set(true);
                    Err(DataSourceError::StopTest)
                } else if self.connection.server_has_exited() {
                    panic!("{}", server_crash_message()); // nocov
                } else {
                    Err(DataSourceError::ServerError(e.to_string()))
                }
            }
        }
    }
}

impl DataSource for ServerDataSource {
    fn generate(&self, schema: &Value) -> Result<Value, DataSourceError> {
        self.send_request("generate", &cbor_map! {"schema" => schema.clone()})
    }

    fn start_span(&self, label: u64) -> Result<(), DataSourceError> {
        self.send_request("start_span", &cbor_map! {"label" => label})?;
        Ok(())
    }

    fn stop_span(&self, discard: bool) -> Result<(), DataSourceError> {
        self.send_request("stop_span", &cbor_map! {"discard" => discard})?;
        Ok(())
    }

    fn new_collection(
        &self,
        min_size: u64,
        max_size: Option<u64>,
    ) -> Result<String, DataSourceError> {
        let mut payload = cbor_map! {
            "min_size" => min_size
        };
        if let Some(max) = max_size {
            map_insert(&mut payload, "max_size", max);
        }
        let response = self.send_request("new_collection", &payload)?;
        match response {
            Value::Integer(i) => {
                let n: i128 = i.into();
                Ok(n.to_string())
            }
            // nocov start
            _ => panic!(
                "Expected integer response from new_collection, got {:?}",
                response
            ),
            // nocov end
        }
    }

    fn collection_more(&self, collection: &str) -> Result<bool, DataSourceError> {
        let collection_id: i64 = collection.parse().unwrap();
        let response = self.send_request(
            "collection_more",
            &cbor_map! { "collection_id" => collection_id },
        )?;
        match response {
            Value::Bool(b) => Ok(b),
            _ => panic!("Expected bool from collection_more, got {:?}", response), // nocov
        }
    }

    // nocov start
    fn collection_reject(
        &self,
        collection: &str,
        why: Option<&str>,
    ) -> Result<(), DataSourceError> {
        let collection_id: i64 = collection.parse().unwrap();
        let mut payload = cbor_map! {
            "collection_id" => collection_id
        };
        if let Some(reason) = why {
            map_insert(&mut payload, "why", reason.to_string());
        }
        self.send_request("collection_reject", &payload)?;
        Ok(())
        // nocov end
    }

    fn new_pool(&self) -> Result<i128, DataSourceError> {
        let response = self.send_request("new_pool", &cbor_map! {})?;
        match response {
            Value::Integer(i) => Ok(i.into()),
            other => panic!("Expected integer response for pool id, got {:?}", other), // nocov
        }
    }

    fn pool_add(&self, pool_id: i128) -> Result<i128, DataSourceError> {
        let response = self.send_request("pool_add", &cbor_map! {"pool_id" => pool_id})?;
        match response {
            Value::Integer(i) => Ok(i.into()),
            other => panic!("Expected integer response for variable id, got {:?}", other), // nocov
        }
    }

    fn pool_generate(&self, pool_id: i128, consume: bool) -> Result<i128, DataSourceError> {
        let response = self.send_request(
            "pool_generate",
            &cbor_map! {
                "pool_id" => pool_id,
                "consume" => consume,
            },
        )?;
        match response {
            Value::Integer(i) => Ok(i.into()),
            other => panic!("Expected integer response for variable id, got {:?}", other), // nocov
        }
    }

    fn mark_complete(&self, status: &str, origin: Option<&str>) {
        let origin_value = match origin {
            Some(s) => Value::Text(s.to_string()),
            None => Value::Null,
        };
        let mark_complete = cbor_map! {
            "command" => "mark_complete",
            "status" => status,
            "origin" => origin_value
        };
        let mut stream = self.stream.borrow_mut();
        let _ = stream.request_cbor(&mark_complete);
        let _ = stream.close();
    }

    fn test_aborted(&self) -> bool {
        self.aborted.get()
    }
}

// ─── HegelSession ───────────────────────────────────────────────────────────

/// Parse a "major.minor" version string into a comparable tuple.
fn parse_version(s: &str) -> (u32, u32) {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 2 {
        panic!("invalid version string '{s}': expected 'major.minor' format");
    }
    let major = parts[0]
        .parse()
        .unwrap_or_else(|_| panic!("invalid major version in '{s}'"));
    let minor = parts[1]
        .parse()
        .unwrap_or_else(|_| panic!("invalid minor version in '{s}'"));
    (major, minor)
}

/// A persistent connection to the hegel server subprocess.
///
/// A new session is created on first use and whenever the previous server
/// process has exited (crash or explicit kill). The Python server supports
/// multiple sequential `run_test` commands over a single connection.
struct HegelSession {
    connection: Arc<Connection>,
    /// The control stream is shared across threads, so it's behind a Mutex
    /// because Stream is not thread-safe. The lock is only held for the
    /// brief run_test send/receive; test execution runs concurrently on
    /// per-test streams.
    control: Mutex<Stream>,
    /// The server subprocess. Shared with the monitor thread so that
    /// `__test_kill_server` can call `child.kill()` directly rather than
    /// shelling out to the OS `kill` command.
    child: Arc<Mutex<std::process::Child>>,
}

impl HegelSession {
    /// Return the current live session, or create a new one if the server has
    /// exited (either crashed or been killed since the last call).
    fn get() -> Arc<HegelSession> {
        let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref s) = *guard {
            if !s.connection.server_has_exited() {
                return Arc::clone(s);
            }
        }
        init_panic_hook();
        let session = Arc::new(HegelSession::init());
        *guard = Some(Arc::clone(&session));
        session
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
        let (lo, hi) = SUPPORTED_PROTOCOL_VERSIONS;
        let version = parse_version(server_version);
        if version < parse_version(lo) || version > parse_version(hi) {
            // nocov start
            let _ = child.kill();
            panic!(
                "hegel-rust supports protocol versions {lo} through {hi}, but \
                 the connected server is using protocol version {server_version}. Upgrading \
                 hegel-rust or downgrading hegel-core might help."
            );
            // nocov end
        }

        let child_arc = Arc::new(Mutex::new(child));
        let child_for_monitor = Arc::clone(&child_arc);

        // Monitor thread: reaps the subprocess when it exits and notifies the
        // connection. Polls try_wait() so the lock is not held while waiting,
        // leaving it available for __test_kill_server to call kill().
        let conn_for_monitor = Arc::clone(&connection);
        std::thread::spawn(move || {
            loop {
                {
                    let mut guard = child_for_monitor.lock().unwrap();
                    if matches!(guard.try_wait(), Ok(Some(_))) {
                        drop(guard);
                        conn_for_monitor.mark_server_exited();
                        return;
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });

        HegelSession {
            connection,
            control: Mutex::new(control),
            child: child_arc,
        }
    }
}

// ─── ServerTestRunner ───────────────────────────────────────────────────────

/// Test runner that communicates with the hegel-core server.
pub(crate) struct ServerTestRunner;

impl TestRunner for ServerTestRunner {
    fn run(
        &self,
        settings: &Settings,
        database_key: Option<&str>,
        run_case: &mut dyn FnMut(Box<dyn DataSource>, bool) -> TestCaseResult,
    ) -> TestRunResult {
        let session = HegelSession::get();
        let connection = &session.connection;
        let verbosity = settings.verbosity;

        let mut test_stream = connection.new_stream();

        let suppress_names: Vec<Value> = settings
            .suppress_health_check
            .iter()
            .map(|c| Value::Text(c.as_str().to_string()))
            .collect();

        let database_key_bytes =
            database_key.map_or(Value::Null, |k| Value::Bytes(k.as_bytes().to_vec()));

        let mut run_test_msg = cbor_map! {
            "command" => "run_test",
            "test_cases" => settings.test_cases,
            "seed" => settings.seed.map_or(Value::Null, Value::from),
            "stream_id" => test_stream.stream_id,
            "database_key" => database_key_bytes,
            "derandomize" => settings.derandomize
        };
        let db_value = match &settings.database {
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
        // The lock is released before any error handling so the mutex is never
        // poisoned by a server crash on one thread affecting other threads.
        let run_test_response = {
            let mut control = session.control.lock().unwrap_or_else(|e| e.into_inner());
            let send_id = control.send_request(cbor_encode(&run_test_msg));
            send_id.and_then(|id| control.receive_reply(id))
        }
        .unwrap_or_else(|e| handle_channel_error(e));
        let _run_test_result: Value = cbor_decode(&run_test_response);

        if verbosity == Verbosity::Debug {
            eprintln!("run_test response received");
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
                    panic!("{}", server_crash_message());
                    // nocov end
                }
                Err(e) => unreachable!("Failed to receive event (server still running): {}", e),
            };

            let event: Value = cbor_decode(&event_payload);
            let event_type = map_get(&event, "event")
                .and_then(as_text)
                .expect("Expected event in payload");

            if verbosity == Verbosity::Debug {
                eprintln!("Received event: {:?}", event);
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

                    let backend = Box::new(ServerDataSource::new(
                        Arc::clone(connection),
                        test_case_stream,
                        verbosity,
                    ));
                    run_case(backend, false);
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
            panic!("Flaky test detected: {}", flaky_msg);
        }

        let n_interesting = map_get(&result_data, "interesting_test_cases")
            .and_then(as_u64)
            .unwrap_or(0);

        if verbosity == Verbosity::Debug {
            eprintln!("Test done. interesting_test_cases={}", n_interesting);
        }

        // Process final replay test cases (one per interesting example)
        let mut failure_message: Option<String> = None;
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

            let backend = Box::new(ServerDataSource::new(
                Arc::clone(connection),
                test_case_stream,
                verbosity,
            ));
            let tc_result = run_case(backend, true);

            if let TestCaseResult::Interesting { panic_message } = tc_result {
                failure_message = Some(panic_message);
            }

            if connection.server_has_exited() {
                panic!("{}", server_crash_message()); // nocov
            }
        }

        let passed = map_get(&result_data, "passed")
            .and_then(as_bool)
            .unwrap_or(true);

        TestRunResult {
            passed,
            failure_message,
        }
    }
}

// ─── Panic hook and backtrace ───────────────────────────────────────────────

#[doc(hidden)]
#[derive(Debug)]
pub struct PanicInfo {
    pub thread_name: String,
    pub thread_id: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub backtrace: Backtrace,
}

impl PanicInfo {
    pub(crate) fn location(&self) -> String {
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

fn server_crash_message() -> String {
    const BASE: &str = "The hegel server process exited unexpectedly.";
    let log_path_owned = SERVER_LOG_PATH.lock().unwrap().clone();
    let log_path = log_path_owned.as_deref().unwrap_or(".hegel/server.log");
    match server_log_excerpt() {
        Some(excerpt) => format!("{BASE}\n\nLast server log entries:\n{excerpt}"),
        None => format!("{BASE}\n\n(No entries found in {log_path})"),
    }
}

fn handle_channel_error(e: std::io::Error) -> ! {
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

// ─── Hegel test builder ─────────────────────────────────────────────────────

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
    /// Panics if any test case fails.
    pub fn run(self) {
        init_panic_hook();

        let runner = ServerTestRunner;
        let mut test_fn = self.test_fn;
        let got_interesting = AtomicBool::new(false);

        let result = runner.run(
            &self.settings,
            self.database_key.as_deref(),
            &mut |backend, is_final| {
                let tc_result = run_test_case(backend, &mut test_fn, is_final);
                if let TestCaseResult::InternalError {
                    panic_message,
                    panic_info,
                } = &tc_result
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
                if matches!(&tc_result, TestCaseResult::Interesting { .. }) {
                    got_interesting.store(true, Ordering::SeqCst);
                }
                tc_result
            },
        );

        let test_failed = !result.passed || got_interesting.load(Ordering::SeqCst);

        if is_running_in_antithesis() {
            #[cfg(not(feature = "antithesis"))]
            panic!(
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
            let msg = result.failure_message.as_deref().unwrap_or("unknown");
            panic!("Property test failed: {}", msg);
        }
    }
}

// ─── Generic test case execution ────────────────────────────────────────────

fn run_test_case(
    data_source: Box<dyn DataSource>,
    test_fn: &mut dyn FnMut(TestCase),
    is_final: bool,
) -> TestCaseResult {
    let tc = TestCase::new(data_source, is_final);

    let result = with_test_context(|| catch_unwind(AssertUnwindSafe(|| test_fn(tc.clone()))));

    let (tc_result, origin) = match &result {
        Ok(()) => (TestCaseResult::Valid, None),
        Err(e) => {
            let msg = panic_message(e);
            if msg == ASSUME_FAIL_STRING {
                (TestCaseResult::Invalid, None)
            } else if msg == STOP_TEST_STRING {
                (TestCaseResult::Overrun, None)
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

    // Send mark_complete via the data source.
    // Skip if test was aborted (StopTest) - the data source already closed.
    if !tc.data_source().test_aborted() {
        let status = match &tc_result {
            TestCaseResult::Valid => "VALID",
            TestCaseResult::Invalid | TestCaseResult::Overrun => "INVALID",
            TestCaseResult::Interesting { .. } => "INTERESTING",
            TestCaseResult::InternalError { .. } => unreachable!(),
        };
        tc.data_source().mark_complete(status, origin.as_deref());
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
#[path = "../tests/embedded/runner_tests.rs"]
mod tests;

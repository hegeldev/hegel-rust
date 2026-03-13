use crate::antithesis::{TestLocation, is_running_in_antithesis};
use crate::control::{currently_in_test_context, set_in_test_context};
use crate::protocol::{Channel, Connection, HANDSHAKE_STRING};
use crate::test_case::{ASSUME_FAIL_STRING, TestCase};
use ciborium::Value;

use crate::cbor_utils::{as_bool, as_text, as_u64, cbor_map, map_get};
use std::backtrace::{Backtrace, BacktraceStatus};
use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::os::unix::net::UnixStream;
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;
use tempfile::TempDir;

const SUPPORTED_PROTOCOL_VERSIONS: (f64, f64) = (0.1, 0.4);
const HEGEL_SERVER_VERSION: &str = "v0.4.0";
const HEGEL_SERVER_COMMAND_ENV: &str = "HEGEL_SERVER_COMMAND";
const HEGEL_SERVER_DIR: &str = ".hegel";
static HEGEL_SERVER_COMMAND: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static SERVER_LOG_FILE: std::sync::OnceLock<Mutex<File>> = std::sync::OnceLock::new();

static PANIC_HOOK_INIT: Once = Once::new();

thread_local! {
    /// (thread_name, thread_id, location, backtrace)
    static LAST_PANIC_INFO: RefCell<Option<(String, String, String, Backtrace)>> = const { RefCell::new(None) };
}

/// (thread_name, thread_id, location, backtrace).
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
            let location = info
                .location()
                .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
                .unwrap_or_else(|| "<unknown>".to_string());

            let backtrace = Backtrace::capture();

            LAST_PANIC_INFO
                .with(|l| *l.borrow_mut() = Some((thread_name, thread_id, location, backtrace)));
        }));
    });
}

fn ensure_hegel_installed() -> Result<String, String> {
    let venv_dir = format!("{HEGEL_SERVER_DIR}/venv");
    let version_file = format!("{venv_dir}/hegel-version");
    let hegel_bin = format!("{venv_dir}/bin/hegel");
    let install_log = format!("{HEGEL_SERVER_DIR}/install.log");

    // Check cached version
    if let Ok(cached) = std::fs::read_to_string(&version_file) {
        if cached.trim() == HEGEL_SERVER_VERSION && std::path::Path::new(&hegel_bin).is_file() {
            return Ok(hegel_bin);
        }
    }

    std::fs::create_dir_all(HEGEL_SERVER_DIR)
        .map_err(|e| format!("Failed to create {HEGEL_SERVER_DIR}: {e}"))?;

    let log_file = std::fs::File::create(&install_log)
        .map_err(|e| format!("Failed to create install log: {e}"))?;

    let status = std::process::Command::new("uv")
        .args(["venv", "--clear", &venv_dir])
        .stderr(log_file.try_clone().unwrap())
        .stdout(log_file.try_clone().unwrap())
        .status()
        .map_err(|e| format!("Failed to run uv venv: {e}"))?;
    if !status.success() {
        let log = std::fs::read_to_string(&install_log).unwrap_or_default();
        return Err(format!("uv venv failed. Install log:\n{log}"));
    }

    let python_path = format!("{venv_dir}/bin/python");
    let status = std::process::Command::new("uv")
        .args([
            "pip",
            "install",
            "--python",
            &python_path,
            &format!("hegel @ git+https://github.com/hegeldev/hegel-core@{HEGEL_SERVER_VERSION}"),
        ])
        .stderr(log_file.try_clone().unwrap())
        .stdout(log_file)
        .status()
        .map_err(|e| format!("Failed to run uv pip install: {e}"))?;
    if !status.success() {
        let log = std::fs::read_to_string(&install_log).unwrap_or_default();
        return Err(format!(
            "Failed to install hegel (version: {HEGEL_SERVER_VERSION}). \
             Set {HEGEL_SERVER_COMMAND_ENV} to a hegel binary path to skip installation.\n\
             Install log:\n{log}"
        ));
    }

    if !std::path::Path::new(&hegel_bin).is_file() {
        return Err(format!("hegel not found at {hegel_bin} after installation"));
    }

    std::fs::write(&version_file, HEGEL_SERVER_VERSION)
        .map_err(|e| format!("Failed to write version file: {e}"))?;

    Ok(hegel_bin)
}

fn server_log_file() -> File {
    let file = SERVER_LOG_FILE.get_or_init(|| {
        std::fs::create_dir_all(HEGEL_SERVER_DIR).ok();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("{HEGEL_SERVER_DIR}/server.log"))
            .expect("Failed to open server log file");
        Mutex::new(file)
    });
    file.lock()
        .unwrap()
        .try_clone()
        .expect("Failed to clone server log file handle")
}

fn find_hegel() -> String {
    if let Ok(override_path) = std::env::var(HEGEL_SERVER_COMMAND_ENV) {
        return override_path;
    }
    HEGEL_SERVER_COMMAND
        .get_or_init(|| {
            ensure_hegel_installed().unwrap_or_else(|e| panic!("Failed to ensure hegel: {e}"))
        })
        .clone()
}

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
/// #[hegel::test]
/// fn test_identity(tc: hegel::TestCase) {
///     let n = tc.draw(generators::integers::<i32>());
///     assert!(n + 0 == n); // Identity property
/// }
/// ```
pub fn hegel<F>(test_fn: F)
where
    F: FnMut(TestCase),
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
/// use hegel::Verbosity;
/// use hegel::generators;
///
/// #[hegel::test(test_cases = 500, verbosity = Verbosity::Verbose)]
/// fn test_with_options(tc: hegel::TestCase) {
///     let n = tc.draw(generators::integers::<i32>());
///     assert!(n + 0 == n);
/// }
/// ```
pub struct Hegel<F> {
    test_fn: F,
    test_cases: u64,
    verbosity: Verbosity,
    seed: Option<u64>,
    test_location: Option<TestLocation>,
}

impl<F> Hegel<F>
where
    F: FnMut(TestCase),
{
    pub fn new(test_fn: F) -> Self {
        Self {
            test_fn,
            test_cases: 100,
            verbosity: Verbosity::Normal,
            seed: None,
            test_location: None,
        }
    }

    pub fn test_cases(mut self, n: u64) -> Self {
        self.test_cases = n;
        self
    }

    pub fn verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = verbosity;
        self
    }

    pub fn seed(mut self, seed: Option<u64>) -> Self {
        self.seed = seed;
        self
    }

    #[doc(hidden)]
    pub fn test_location(mut self, location: TestLocation) -> Self {
        self.test_location = Some(location);
        self
    }

    /// Run the property-based tests.
    ///
    /// This function:
    /// 1. Creates a Unix socket server
    /// 2. Spawns the hegel server as a subprocess
    /// 3. Accepts the connection from the hegel server
    /// 4. Handles test case events from the hegel server
    /// 5. Runs the test function for each test case
    /// 6. Reports results back to the hegel server
    /// 7. Panics if any test case fails
    pub fn run(self) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let socket_path = temp_dir.path().join("hegel.sock");

        let hegel_binary_path = find_hegel();
        let mut cmd = Command::new(&hegel_binary_path);
        cmd.arg(&socket_path)
            .arg("--verbosity")
            .arg(self.verbosity.as_str());

        cmd.env("PYTHONUNBUFFERED", "1");
        let log_file = server_log_file();
        let log_file2 = log_file
            .try_clone()
            .expect("Failed to clone log file handle");
        cmd.stdout(Stdio::from(log_file));
        cmd.stderr(Stdio::from(log_file2));

        if self.verbosity == Verbosity::Debug {
            eprintln!("Starting hegel server: {:?}", cmd);
        }

        #[allow(clippy::expect_fun_call)]
        let mut child = cmd
            .spawn()
            .expect(format!("Failed to spawn hegel at path {}", hegel_binary_path).as_str());

        init_panic_hook();

        let mut attempts = 0;
        // wait for socket initialization
        let stream = loop {
            if socket_path.exists() {
                match UnixStream::connect(&socket_path) {
                    Ok(stream) => break stream,
                    Err(_) if attempts < 50 => {
                        std::thread::sleep(Duration::from_millis(100));
                        attempts += 1;
                        continue;
                    }
                    Err(e) => {
                        let _ = child.kill();
                        panic!("Failed to connect to hegel server socket: {}", e);
                    }
                }
            }
            if attempts >= 50 {
                let _ = child.kill();
                panic!("Timeout waiting for hegel server to create socket");
            }
            std::thread::sleep(Duration::from_millis(100));
            attempts += 1;
        };

        // set a read timeout so we don't hang if the server crashes
        stream.set_read_timeout(Some(Duration::from_secs(120))).ok();

        let connection = Connection::new(stream);
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
        let version: f64 = server_version.parse().unwrap_or_else(|_| {
            let _ = child.kill();
            panic!("Bad version number: {server_version}");
        });

        let (lo, hi) = SUPPORTED_PROTOCOL_VERSIONS;
        if !(lo <= version && version <= hi) {
            let _ = child.kill();
            panic!(
                "hegel-rust supports protocol versions {lo} through {hi}, but \
                 got server version {version}. Upgrading hegel-rust or downgrading \
                 your hegel server might help."
            );
        }

        if self.verbosity == Verbosity::Debug {
            eprintln!("Version negotiation complete");
        }

        let mut test_fn = self.test_fn;
        let verbosity = self.verbosity;
        let got_interesting = Arc::new(AtomicBool::new(false));
        let test_channel = connection.new_channel();

        let run_test_msg = cbor_map! {
            "command" => "run_test",
            "test_cases" => self.test_cases,
            "seed" => self.seed.map_or(Value::Null, Value::from),
            "channel_id" => test_channel.channel_id
        };

        let run_test_id = control
            .send_request(cbor_encode(&run_test_msg))
            .expect("Failed to send run_test");

        let run_test_response = control
            .receive_reply(run_test_id)
            .expect("Failed to receive run_test response");
        let _run_test_result: Value = cbor_decode(&run_test_response);

        if verbosity == Verbosity::Debug {
            eprintln!("run_test response received");
        }

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

        // clean up so the server can exit gracefully
        drop(test_channel);
        drop(control);
        let _ = connection.close();
        drop(connection);

        let _ = child.wait().expect("Failed to wait for hegel server");

        let test_failed = !passed || got_interesting.load(Ordering::SeqCst);

        if let Some(ref loc) = self.test_location
        {
            if is_running_in_antithesis() {
                crate::antithesis::emit_assertion(loc, !test_failed);
            }
            
        }

        if test_failed {
            panic!("Property test failed");
        }
    }
}

/// Run a single test case.
fn run_test_case<F: FnMut(TestCase)>(
    connection: &Arc<Connection>,
    test_channel: Channel,
    test_fn: &mut F,
    is_final: bool,
    verbosity: Verbosity,
    got_interesting: &Arc<AtomicBool>,
) {
    // Create TestCase. The test function gets a clone (cheap Rc bump),
    // so we retain access to the same underlying TestCaseData after the test runs.
    let tc = TestCase::new(Arc::clone(connection), test_channel, verbosity, is_final);
    set_in_test_context(true);

    let result = catch_unwind(AssertUnwindSafe(|| test_fn(tc.clone())));

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

                    for value in tc.take_output() {
                        eprintln!("{}", value);
                    }

                    if backtrace.status() == BacktraceStatus::Captured {
                        let is_full = std::env::var("RUST_BACKTRACE")
                            .map(|v| v == "full")
                            .unwrap_or(false);
                        let formatted = format_backtrace(&backtrace, is_full);
                        eprintln!("stack backtrace:\n{}", formatted);
                        if !is_full {
                            eprintln!(
                                "note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace."
                            );
                        }
                    }
                }

                let origin = format!("Panic at {}", location);
                ("INTERESTING".to_string(), Some(origin))
            }
        }
    };

    // Send mark_complete using the same channel that generators used.
    // Skip if test was aborted (StopTest) - server already closed the channel.
    if !tc.test_aborted() {
        let origin_value = match &origin {
            Some(s) => Value::Text(s.clone()),
            None => Value::Null,
        };
        let mark_complete = cbor_map! {
            "command" => "mark_complete",
            "status" => status.as_str(),
            "origin" => origin_value
        };
        tc.send_mark_complete(&mark_complete);
    }

    set_in_test_context(false);
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

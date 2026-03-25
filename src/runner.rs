use crate::antithesis::{TestLocation, is_running_in_antithesis};
use crate::control::{currently_in_test_context, with_test_context};
use crate::protocol::{Channel, Connection, HANDSHAKE_STRING, SERVER_CRASHED_MESSAGE};
use crate::test_case::{ASSUME_FAIL_STRING, STOP_TEST_STRING, TestCase};
use ciborium::Value;

use crate::cbor_utils::{as_bool, as_text, as_u64, cbor_map, map_get};
use std::backtrace::{Backtrace, BacktraceStatus};
use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};

const SUPPORTED_PROTOCOL_VERSIONS: (f64, f64) = (0.6, 0.7);
const HEGEL_SERVER_VERSION: &str = "0.2.3";
const HEGEL_SERVER_COMMAND_ENV: &str = "HEGEL_SERVER_COMMAND";
const HEGEL_SERVER_DIR: &str = ".hegel";
const UV_NOT_FOUND_MESSAGE: &str = "\
You are seeing this error message because hegel-rust tried to use `uv` to install \
hegel-core, but could not find uv on the PATH.

Hegel uses a Python server component called `hegel-core` to share core property-based \
testing functionality across languages. There are two ways for Hegel to get hegel-core:

* By default, Hegel looks for uv (https://docs.astral.sh/uv/) on the PATH, and \
  uses uv to install hegel-core to a local `.hegel/venv` directory. We recommend this \
  option. To continue, install uv: https://docs.astral.sh/uv/getting-started/installation/.
* Alternatively, you can manage the installation of hegel-core yourself. After installing, \
  setting the HEGEL_SERVER_COMMAND environment variable to your hegel-core binary path tells \
  hegel-rust to use that hegel-core instead.

See https://hegel.dev/reference/installation for more details.";
static HEGEL_SERVER_COMMAND: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static SERVER_LOG_FILE: std::sync::OnceLock<Mutex<File>> = std::sync::OnceLock::new();
static SESSION: std::sync::OnceLock<HegelSession> = std::sync::OnceLock::new();

static PANIC_HOOK_INIT: Once = Once::new();

/// A persistent connection to the hegel server subprocess.
///
/// Created once per process on first use. The subprocess and connection
/// are reused across all `Hegel::run()` calls. The Python server supports
/// multiple sequential `run_test` commands over a single connection.
struct HegelSession {
    connection: Arc<Connection>,
    /// The control channel is shared across threads, so it's behind a Mutex
    /// because Channel is not thread-safe. The lock is only held for the
    /// brief run_test send/receive; test execution runs concurrently on
    /// per-test channels.
    control: Mutex<Channel>,
}

impl HegelSession {
    fn get() -> &'static HegelSession {
        SESSION.get_or_init(|| {
            init_panic_hook();
            HegelSession::init()
        })
    }

    fn init() -> HegelSession {
        let hegel_binary_path = find_hegel();
        let mut cmd = Command::new(&hegel_binary_path);
        cmd.arg("--stdio").arg("--verbosity").arg("normal");

        cmd.env("PYTHONUNBUFFERED", "1");
        let log_file = server_log_file();
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::from(log_file));

        #[allow(clippy::expect_fun_call)]
        let mut child = cmd
            .spawn()
            .expect(format!("Failed to spawn hegel at path {}", hegel_binary_path).as_str());

        let child_stdin = child.stdin.take().expect("Failed to take child stdin");
        let child_stdout = child.stdout.take().expect("Failed to take child stdout");

        let connection = Connection::new(Box::new(child_stdout), Box::new(child_stdin));
        let mut control = connection.control_channel();

        // Handshake
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
                 the connected server is using protocol version {version}. Upgrading \
                 hegel-rust or downgrading hegel-core might help."
            );
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
        .status();
    match &status {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(UV_NOT_FOUND_MESSAGE.to_string());
        }
        Err(e) => {
            return Err(format!("Failed to run `uv venv`: {e}"));
        }
        Ok(s) if !s.success() => {
            let log = std::fs::read_to_string(&install_log).unwrap_or_default();
            return Err(format!("uv venv failed. Install log:\n{log}"));
        }
        Ok(_) => {}
    }

    let python_path = format!("{venv_dir}/bin/python");
    let status = std::process::Command::new("uv")
        .args([
            "pip",
            "install",
            "--python",
            &python_path,
            &format!("hegel-core=={HEGEL_SERVER_VERSION}"),
        ])
        .stderr(log_file.try_clone().unwrap())
        .stdout(log_file)
        .status()
        .map_err(|e| format!("Failed to run `uv pip install`: {e}"))?;
    if !status.success() {
        let log = std::fs::read_to_string(&install_log).unwrap_or_default();
        return Err(format!(
            "Failed to install hegel-core (version: {HEGEL_SERVER_VERSION}). \
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
        .get_or_init(|| ensure_hegel_installed().unwrap_or_else(|e| panic!("{e}")))
        .clone()
}

/// Health checks that can be suppressed during test execution.
///
/// Health checks detect common issues with test configuration that would
/// otherwise cause tests to run inefficiently or not at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HealthCheck {
    /// Too many test cases are being filtered out via `assume()`.
    FilterTooMuch,
    /// Test execution is too slow.
    TooSlow,
    /// Generated test cases are too large.
    TestCasesTooLarge,
    /// The smallest natural input is very large.
    LargeInitialTestCase,
}

impl HealthCheck {
    /// Returns all health check variants.
    ///
    /// Useful for suppressing all health checks at once:
    ///
    /// ```no_run
    /// use hegel::HealthCheck;
    ///
    /// #[hegel::test(suppress_health_check = HealthCheck::all())]
    /// fn my_test(tc: hegel::TestCase) {
    ///     // ...
    /// }
    /// ```
    pub const fn all() -> [HealthCheck; 4] {
        [
            HealthCheck::FilterTooMuch,
            HealthCheck::TooSlow,
            HealthCheck::TestCasesTooLarge,
            HealthCheck::LargeInitialTestCase,
        ]
    }

    fn as_str(&self) -> &'static str {
        match self {
            HealthCheck::FilterTooMuch => "filter_too_much",
            HealthCheck::TooSlow => "too_slow",
            HealthCheck::TestCasesTooLarge => "test_cases_too_large",
            HealthCheck::LargeInitialTestCase => "large_initial_test_case",
        }
    }
}

/// Controls how much output Hegel produces during test runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    /// Suppress all output.
    Quiet,
    /// Default output level.
    Normal,
    /// Show more detail about the test run.
    Verbose,
    /// Show protocol-level debug information.
    Debug,
}

impl Verbosity {}

// internal use only
#[doc(hidden)]
pub fn hegel<F>(test_fn: F)
where
    F: FnMut(TestCase),
{
    Hegel::new(test_fn).run();
}

fn is_in_ci() -> bool {
    const CI_VARS: &[(&str, Option<&str>)] = &[
        ("CI", None),
        ("TF_BUILD", Some("true")),
        ("BUILDKITE", Some("true")),
        ("CIRCLECI", Some("true")),
        ("CIRRUS_CI", Some("true")),
        ("CODEBUILD_BUILD_ID", None),
        ("GITHUB_ACTIONS", Some("true")),
        ("GITLAB_CI", None),
        ("HEROKU_TEST_RUN_ID", None),
        ("TEAMCITY_VERSION", None),
        ("bamboo.buildKey", None),
    ];

    CI_VARS.iter().any(|(key, value)| match value {
        None => std::env::var_os(key).is_some(),
        Some(expected) => std::env::var(key).ok().as_deref() == Some(expected),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Database {
    Unset,
    Disabled,
    Path(String),
}

/// Configuration for a Hegel test run.
///
/// Use builder methods to customize, then pass to [`Hegel::settings`] or
/// the `settings` parameter of `#[hegel::test]`.
///
/// In CI environments (detected automatically), the database is disabled
/// and tests are derandomized by default.
#[derive(Debug, Clone)]
pub struct Settings {
    test_cases: u64,
    verbosity: Verbosity,
    seed: Option<u64>,
    derandomize: bool,
    database: Database,
    suppress_health_check: Vec<HealthCheck>,
}

impl Settings {
    /// Create settings with defaults. Detects CI environments automatically.
    pub fn new() -> Self {
        let in_ci = is_in_ci();
        Self {
            test_cases: 100,
            verbosity: Verbosity::Normal,
            seed: None,
            derandomize: in_ci,
            database: if in_ci {
                Database::Disabled
            } else {
                Database::Unset
            },
            suppress_health_check: Vec::new(),
        }
    }

    /// Set the number of test cases to run (default: 100).
    pub fn test_cases(mut self, n: u64) -> Self {
        self.test_cases = n;
        self
    }

    /// Set the verbosity level.
    pub fn verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = verbosity;
        self
    }

    /// Set a fixed seed for reproducibility, or `None` for random.
    pub fn seed(mut self, seed: Option<u64>) -> Self {
        self.seed = seed;
        self
    }

    /// When true, use a fixed seed derived from the test name. Enabled by default in CI.
    pub fn derandomize(mut self, derandomize: bool) -> Self {
        self.derandomize = derandomize;
        self
    }

    /// Set the database path for storing failing examples, or `None` to disable.
    pub fn database(mut self, database: Option<String>) -> Self {
        self.database = match database {
            None => Database::Disabled,
            Some(path) => Database::Path(path),
        };
        self
    }

    /// Suppress one or more health checks so they do not cause test failure.
    ///
    /// Health checks detect common issues like excessive filtering or slow
    /// tests. Use this to suppress specific checks when they are expected.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::{HealthCheck, Verbosity};
    /// use hegel::generators;
    ///
    /// #[hegel::test(suppress_health_check = [HealthCheck::FilterTooMuch, HealthCheck::TooSlow])]
    /// fn my_test(tc: hegel::TestCase) {
    ///     let n: i32 = tc.draw(generators::integers());
    ///     tc.assume(n > 0);
    /// }
    /// ```
    pub fn suppress_health_check(mut self, checks: impl IntoIterator<Item = HealthCheck>) -> Self {
        self.suppress_health_check.extend(checks);
        self
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
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
        let mut test_channel = connection.new_channel();

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
            "channel_id" => test_channel.channel_id,
            "database_key" => database_key_bytes,
            "derandomize" => self.settings.derandomize
        };
        let db_value = match &self.settings.database {
            Database::Unset => Option::None,
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

        // The control channel is behind a Mutex because Channel requires &mut self.
        // This only serializes the brief run_test send/receive — actual test
        // execution happens on per-test channels without holding this lock.
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
            eprintln!("run_test response received");
        }

        let result_data: Value;
        let ack_null = cbor_map! {"result" => Value::Null};
        loop {
            let (event_id, event_payload) = test_channel
                .receive_request()
                .expect("Failed to receive event");

            let event: Value = cbor_decode(&event_payload);
            let event_type = map_get(&event, "event")
                .and_then(as_text)
                .expect("Expected event in payload");

            if verbosity == Verbosity::Debug {
                eprintln!("Received event: {:?}", event);
            }

            match event_type {
                "test_case" => {
                    let channel_id = map_get(&event, "channel_id")
                        .and_then(as_u64)
                        .expect("Missing channel id") as u32;

                    let test_case_channel = connection.connect_channel(channel_id);

                    // Ack the test_case event BEFORE running the test (prevents deadlock)
                    test_channel
                        .write_reply(event_id, cbor_encode(&ack_null))
                        .expect("Failed to ack test_case");

                    run_test_case(
                        connection,
                        test_case_channel,
                        &mut test_fn,
                        false,
                        verbosity,
                        &got_interesting,
                    );

                    if connection.server_has_exited() {
                        panic!("{}", SERVER_CRASHED_MESSAGE);
                    }
                }
                "test_done" => {
                    let ack_true = cbor_map! {"result" => true};
                    test_channel
                        .write_reply(event_id, cbor_encode(&ack_true))
                        .expect("Failed to ack test_done");
                    result_data = map_get(&event, "results").cloned().unwrap_or(Value::Null);
                    break;
                }
                _ => {
                    panic!("unknown event: {}", event_type);
                }
            }
        }

        // Check for server-side errors before processing results
        if let Some(error_msg) = map_get(&result_data, "error").and_then(as_text) {
            panic!("Server error: {}", error_msg);
        }

        // Check for health check failure before processing results
        if let Some(failure_msg) = map_get(&result_data, "health_check_failure").and_then(as_text) {
            panic!("Health check failure:\n{}", failure_msg);
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
        let mut final_result: Option<TestCaseResult> = None;
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

            let tc_result = run_test_case(
                connection,
                test_case_channel,
                &mut test_fn,
                true,
                verbosity,
                &got_interesting,
            );

            if matches!(&tc_result, TestCaseResult::Interesting { .. }) {
                final_result = Some(tc_result);
            }

            if connection.server_has_exited() {
                panic!("{}", SERVER_CRASHED_MESSAGE);
            }
        }

        let passed = map_get(&result_data, "passed")
            .and_then(as_bool)
            .unwrap_or(true);

        let test_failed = !passed || got_interesting.load(Ordering::SeqCst);

        if is_running_in_antithesis() {
            #[cfg(not(feature = "antithesis"))]
            panic!(
                "When Hegel is run inside of Antithesis, it requires the `antithesis` feature. \
                You can add it with {{ features = [\"antithesis\"] }}."
            );

            #[cfg(feature = "antithesis")]
            if let Some(ref loc) = self.test_location {
                crate::antithesis::emit_assertion(loc, !test_failed);
            }
        }

        if test_failed {
            let msg = match &final_result {
                Some(TestCaseResult::Interesting { panic_message }) => panic_message.as_str(),
                _ => "unknown",
            };
            panic!("Property test failed: {}", msg);
        }
    }
}

enum TestCaseResult {
    Valid,
    Invalid,
    Interesting { panic_message: String },
}

fn run_test_case<F: FnMut(TestCase)>(
    connection: &Arc<Connection>,
    test_channel: Channel,
    test_fn: &mut F,
    is_final: bool,
    verbosity: Verbosity,
    got_interesting: &Arc<AtomicBool>,
) -> TestCaseResult {
    // Create TestCase. The test function gets a clone (cheap Rc bump),
    // so we retain access to the same underlying TestCaseData after the test runs.
    let tc = TestCase::new(Arc::clone(connection), test_channel, verbosity, is_final);

    let result = with_test_context(|| catch_unwind(AssertUnwindSafe(|| test_fn(tc.clone()))));

    let (tc_result, origin) = match &result {
        Ok(()) => (TestCaseResult::Valid, None),
        Err(e) => {
            let msg = panic_message(e);
            if msg == ASSUME_FAIL_STRING || msg == STOP_TEST_STRING {
                (TestCaseResult::Invalid, None)
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
                (
                    TestCaseResult::Interesting { panic_message: msg },
                    Some(origin),
                )
            }
        }
    };

    // Send mark_complete using the same channel that generators used.
    // Skip if test was aborted (StopTest) - server already closed the channel.
    if !tc.test_aborted() {
        let status = match &tc_result {
            TestCaseResult::Valid => "VALID",
            TestCaseResult::Invalid => "INVALID",
            TestCaseResult::Interesting { .. } => "INTERESTING",
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

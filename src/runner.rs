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
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

const SUPPORTED_PROTOCOL_VERSIONS: (f64, f64) = (0.6, 0.7);
const HEGEL_SERVER_VERSION: &str = "0.2.3";
const HEGEL_SERVER_COMMAND_ENV: &str = "HEGEL_SERVER_COMMAND";
const FILE_LOCK_TIMEOUT: Duration = Duration::from_secs(300);
const FILE_LOCK_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Returns the cache directory for hegel installations.
///
/// Resolution order:
/// 1. `$XDG_CACHE_HOME/hegel` if `XDG_CACHE_HOME` is set
/// 2. `~/Library/Caches/hegel` on macOS
/// 3. `~/.cache/hegel` on other platforms
fn hegel_cache_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("hegel");
    }
    let home = std::env::var("HOME").expect(
        "Could not determine home directory: HOME is not set. \
         Set XDG_CACHE_HOME or HEGEL_SERVER_COMMAND to work around this.",
    );
    if cfg!(target_os = "macos") {
        PathBuf::from(home).join("Library/Caches/hegel")
    } else {
        PathBuf::from(home).join(".cache/hegel")
    }
}

/// Returns the versioned directory for the current hegel-core version.
/// e.g. `<cache>/versions/0.2.3`
fn hegel_version_dir() -> PathBuf {
    hegel_cache_dir()
        .join("versions")
        .join(HEGEL_SERVER_VERSION)
}

/// Acquire a cross-process file lock using mkdir (atomic on all platforms).
fn acquire_file_lock(lock_dir: &std::path::Path) -> Result<(), String> {
    let deadline = Instant::now() + FILE_LOCK_TIMEOUT;
    loop {
        match std::fs::create_dir(lock_dir) {
            Ok(()) => return Ok(()),
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(FILE_LOCK_POLL_INTERVAL);
            }
            Err(_) => {
                return Err(format!(
                    "hegel: timed out waiting for install lock at {} \
                     (another process may be installing; remove manually if stale)",
                    lock_dir.display()
                ));
            }
        }
    }
}

/// Release the cross-process file lock.
fn release_file_lock(lock_dir: &std::path::Path) {
    std::fs::remove_dir(lock_dir).expect("Failed to release install lock");
}

const UV_NOT_FOUND_MESSAGE: &str = "\
You are seeing this error message because hegel-rust tried to use `uv` to install \
hegel-core, but could not find uv on the PATH.

Hegel uses a Python server component called `hegel-core` to share core property-based \
testing functionality across languages. There are two ways for Hegel to get hegel-core:

* By default, Hegel looks for uv (https://docs.astral.sh/uv/) on the PATH, and \
  uses uv to install hegel-core into a cache directory. We recommend this \
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
    let version_dir = hegel_version_dir();
    let venv_dir = version_dir.join("venv");
    let version_file = venv_dir.join("hegel-version");
    let hegel_bin = venv_dir.join("bin/hegel");
    let install_log = version_dir.join("install.log");

    // Fast path (no locks): check cached version.
    if is_installed(&version_file, &hegel_bin) {
        return Ok(hegel_bin.to_string_lossy().into_owned());
    }

    // Create the version directory (needed for the file lock).
    std::fs::create_dir_all(&version_dir)
        .map_err(|e| format!("Failed to create {}: {e}", version_dir.display()))?;

    // Acquire cross-process file lock.
    let lock_dir = version_dir.join(".install-lock");
    acquire_file_lock(&lock_dir)?;
    let result = do_install(&venv_dir, &version_file, &hegel_bin, &install_log);
    release_file_lock(&lock_dir);
    result
}

fn is_installed(version_file: &std::path::Path, hegel_bin: &std::path::Path) -> bool {
    if let Ok(cached) = std::fs::read_to_string(version_file) {
        cached.trim() == HEGEL_SERVER_VERSION && hegel_bin.is_file()
    } else {
        false
    }
}

fn do_install(
    venv_dir: &std::path::Path,
    version_file: &std::path::Path,
    hegel_bin: &std::path::Path,
    install_log: &std::path::Path,
) -> Result<String, String> {
    // Re-check after acquiring lock (another process may have installed).
    if is_installed(version_file, hegel_bin) {
        return Ok(hegel_bin.to_string_lossy().into_owned());
    }

    let venv_str = venv_dir.to_string_lossy();

    let log_file = std::fs::File::create(install_log)
        .map_err(|e| format!("Failed to create install log: {e}"))?;

    let status = std::process::Command::new("uv")
        .args(["venv", "--clear", &venv_str])
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
            let log = std::fs::read_to_string(install_log).unwrap_or_default();
            return Err(format!("uv venv failed. Install log:\n{log}"));
        }
        Ok(_) => {}
    }

    let python_path = venv_dir.join("bin/python");
    let python_str = python_path.to_string_lossy();
    let status = std::process::Command::new("uv")
        .args([
            "pip",
            "install",
            "--python",
            &python_str,
            &format!("hegel-core=={HEGEL_SERVER_VERSION}"),
        ])
        .stderr(log_file.try_clone().unwrap())
        .stdout(log_file)
        .status()
        .map_err(|e| format!("Failed to run `uv pip install`: {e}"))?;
    if !status.success() {
        let log = std::fs::read_to_string(install_log).unwrap_or_default();
        return Err(format!(
            "Failed to install hegel-core (version: {HEGEL_SERVER_VERSION}). \
             Set {HEGEL_SERVER_COMMAND_ENV} to a hegel binary path to skip installation.\n\
             Install log:\n{log}"
        ));
    }

    if !hegel_bin.is_file() {
        return Err(format!(
            "hegel not found at {} after installation",
            hegel_bin.display()
        ));
    }

    std::fs::write(version_file, HEGEL_SERVER_VERSION)
        .map_err(|e| format!("Failed to write version file: {e}"))?;

    Ok(hegel_bin.to_string_lossy().into_owned())
}

fn server_log_file() -> File {
    let file = SERVER_LOG_FILE.get_or_init(|| {
        std::fs::create_dir_all(".hegel").ok();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(".hegel/server.log")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;
    use tempfile::TempDir;

    // Environment variable tests must run serially since env vars are process-global.
    // Use into_ok() to recover from poison (the should_panic test poisons the mutex).
    static ENV_LOCK: StdMutex<()> = StdMutex::new(());

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    // SAFETY: All env var mutations are serialized by ENV_LOCK, so no other
    // threads are reading these variables concurrently.

    unsafe fn set_env(key: &str, val: impl AsRef<std::ffi::OsStr>) {
        unsafe { std::env::set_var(key, val) }
    }

    unsafe fn remove_env(key: &str) {
        unsafe { std::env::remove_var(key) }
    }

    unsafe fn restore_env(key: &str, old: Option<String>) {
        match old {
            Some(v) => unsafe { set_env(key, v) },
            None => unsafe { remove_env(key) },
        }
    }

    // -- hegel_cache_dir tests --

    #[test]
    fn cache_dir_respects_xdg_cache_home() {
        let _guard = lock_env();
        let old_xdg = std::env::var("XDG_CACHE_HOME").ok();
        unsafe { set_env("XDG_CACHE_HOME", "/tmp/test-xdg-cache") };
        let result = hegel_cache_dir();
        unsafe { restore_env("XDG_CACHE_HOME", old_xdg) };
        assert_eq!(result, PathBuf::from("/tmp/test-xdg-cache/hegel"));
    }

    #[test]
    fn cache_dir_falls_back_to_home_when_xdg_unset() {
        let _guard = lock_env();
        let old_xdg = std::env::var("XDG_CACHE_HOME").ok();
        let old_home = std::env::var("HOME").ok();
        unsafe { remove_env("XDG_CACHE_HOME") };
        unsafe { set_env("HOME", "/Users/testuser") };
        let result = hegel_cache_dir();
        unsafe { restore_env("XDG_CACHE_HOME", old_xdg) };
        unsafe { restore_env("HOME", old_home) };
        if cfg!(target_os = "macos") {
            assert_eq!(
                result,
                PathBuf::from("/Users/testuser/Library/Caches/hegel")
            );
        } else {
            assert_eq!(result, PathBuf::from("/Users/testuser/.cache/hegel"));
        }
    }

    #[test]
    #[should_panic(expected = "Could not determine home directory")]
    fn cache_dir_panics_when_home_unset_and_no_xdg() {
        let _guard = lock_env();
        let old_xdg = std::env::var("XDG_CACHE_HOME").ok();
        let old_home = std::env::var("HOME").ok();
        unsafe { remove_env("XDG_CACHE_HOME") };
        unsafe { remove_env("HOME") };
        let _cleanup = defer(move || unsafe {
            restore_env("XDG_CACHE_HOME", old_xdg);
            restore_env("HOME", old_home);
        });
        hegel_cache_dir();
    }

    #[test]
    fn cache_dir_xdg_takes_priority_over_platform_default() {
        let _guard = lock_env();
        let old_xdg = std::env::var("XDG_CACHE_HOME").ok();
        unsafe { set_env("XDG_CACHE_HOME", "/custom/cache") };
        let result = hegel_cache_dir();
        unsafe { restore_env("XDG_CACHE_HOME", old_xdg) };
        // Even on macOS, XDG_CACHE_HOME should take priority
        assert_eq!(result, PathBuf::from("/custom/cache/hegel"));
    }

    // -- hegel_version_dir tests --

    #[test]
    fn version_dir_includes_version_number() {
        let _guard = lock_env();
        let old_xdg = std::env::var("XDG_CACHE_HOME").ok();
        unsafe { set_env("XDG_CACHE_HOME", "/tmp/test-cache") };
        let result = hegel_version_dir();
        unsafe { restore_env("XDG_CACHE_HOME", old_xdg) };
        assert_eq!(
            result,
            PathBuf::from(format!(
                "/tmp/test-cache/hegel/versions/{}",
                HEGEL_SERVER_VERSION
            ))
        );
    }

    // -- is_installed tests --

    #[test]
    fn is_installed_false_when_version_file_missing() {
        let dir = TempDir::new().unwrap();
        let version_file = dir.path().join("hegel-version");
        let hegel_bin = dir.path().join("bin/hegel");
        assert!(!is_installed(&version_file, &hegel_bin));
    }

    #[test]
    fn is_installed_false_when_version_mismatch() {
        let dir = TempDir::new().unwrap();
        let version_file = dir.path().join("hegel-version");
        let hegel_bin = dir.path().join("bin/hegel");
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        std::fs::write(&hegel_bin, "fake").unwrap();
        std::fs::write(&version_file, "0.0.0").unwrap();
        assert!(!is_installed(&version_file, &hegel_bin));
    }

    #[test]
    fn is_installed_false_when_binary_missing() {
        let dir = TempDir::new().unwrap();
        let version_file = dir.path().join("hegel-version");
        let hegel_bin = dir.path().join("bin/hegel");
        std::fs::write(&version_file, HEGEL_SERVER_VERSION).unwrap();
        assert!(!is_installed(&version_file, &hegel_bin));
    }

    #[test]
    fn is_installed_true_when_version_matches_and_binary_exists() {
        let dir = TempDir::new().unwrap();
        let version_file = dir.path().join("hegel-version");
        let hegel_bin = dir.path().join("bin/hegel");
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        std::fs::write(&hegel_bin, "fake").unwrap();
        std::fs::write(&version_file, HEGEL_SERVER_VERSION).unwrap();
        assert!(is_installed(&version_file, &hegel_bin));
    }

    #[test]
    fn is_installed_trims_whitespace_from_version_file() {
        let dir = TempDir::new().unwrap();
        let version_file = dir.path().join("hegel-version");
        let hegel_bin = dir.path().join("bin/hegel");
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        std::fs::write(&hegel_bin, "fake").unwrap();
        std::fs::write(&version_file, format!("  {HEGEL_SERVER_VERSION}\n")).unwrap();
        assert!(is_installed(&version_file, &hegel_bin));
    }

    #[test]
    fn is_installed_false_when_version_file_is_empty() {
        let dir = TempDir::new().unwrap();
        let version_file = dir.path().join("hegel-version");
        let hegel_bin = dir.path().join("bin/hegel");
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        std::fs::write(&hegel_bin, "fake").unwrap();
        std::fs::write(&version_file, "").unwrap();
        assert!(!is_installed(&version_file, &hegel_bin));
    }

    #[test]
    fn is_installed_false_when_binary_is_directory() {
        let dir = TempDir::new().unwrap();
        let version_file = dir.path().join("hegel-version");
        let hegel_bin = dir.path().join("bin/hegel");
        // Create hegel as a directory, not a file
        std::fs::create_dir_all(&hegel_bin).unwrap();
        std::fs::write(&version_file, HEGEL_SERVER_VERSION).unwrap();
        assert!(!is_installed(&version_file, &hegel_bin));
    }

    // -- acquire_file_lock / release_file_lock tests --

    #[test]
    fn file_lock_acquire_creates_directory() {
        let dir = TempDir::new().unwrap();
        let lock_dir = dir.path().join(".install-lock");
        assert!(!lock_dir.exists());
        acquire_file_lock(&lock_dir).unwrap();
        assert!(lock_dir.exists());
        assert!(lock_dir.is_dir());
        release_file_lock(&lock_dir);
    }

    #[test]
    fn file_lock_release_removes_directory() {
        let dir = TempDir::new().unwrap();
        let lock_dir = dir.path().join(".install-lock");
        acquire_file_lock(&lock_dir).unwrap();
        assert!(lock_dir.exists());
        release_file_lock(&lock_dir);
        assert!(!lock_dir.exists());
    }

    #[test]
    #[should_panic(expected = "Failed to release install lock")]
    fn file_lock_release_panics_when_not_held() {
        let dir = TempDir::new().unwrap();
        let lock_dir = dir.path().join(".install-lock");
        release_file_lock(&lock_dir);
    }

    #[test]
    fn file_lock_cannot_acquire_twice() {
        let dir = TempDir::new().unwrap();
        let lock_dir = dir.path().join(".install-lock");
        acquire_file_lock(&lock_dir).unwrap();
        // Second acquire from same thread would block forever;
        // verify the lock dir exists (which is what blocks acquisition).
        assert!(std::fs::create_dir(&lock_dir).is_err());
        release_file_lock(&lock_dir);
    }

    #[test]
    fn file_lock_acquire_succeeds_after_release_from_another_thread() {
        let dir = TempDir::new().unwrap();
        let lock_dir = dir.path().join(".install-lock");

        // Thread 1: hold the lock for a short time, then release.
        let lock_dir_clone = lock_dir.clone();
        let holder = std::thread::spawn(move || {
            acquire_file_lock(&lock_dir_clone).unwrap();
            std::thread::sleep(Duration::from_millis(200));
            release_file_lock(&lock_dir_clone);
        });

        // Give thread 1 time to acquire the lock.
        std::thread::sleep(Duration::from_millis(50));

        // Thread 2 (this thread): should block then succeed.
        let start = Instant::now();
        acquire_file_lock(&lock_dir).unwrap();
        let elapsed = start.elapsed();

        // It should have waited at least ~100ms (holder had ~150ms left).
        assert!(
            elapsed >= Duration::from_millis(50),
            "expected to wait for lock, but elapsed was {:?}",
            elapsed
        );
        release_file_lock(&lock_dir);
        holder.join().unwrap();
    }

    #[test]
    fn file_lock_concurrent_threads_serialize() {
        let dir = TempDir::new().unwrap();
        let lock_dir = dir.path().join(".install-lock");
        let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let max_concurrent = Arc::new(std::sync::atomic::AtomicU32::new(0));

        let mut handles = vec![];
        for _ in 0..5 {
            let ld = lock_dir.clone();
            let ctr = Arc::clone(&counter);
            let max = Arc::clone(&max_concurrent);
            handles.push(std::thread::spawn(move || {
                acquire_file_lock(&ld).unwrap();
                let current = ctr.fetch_add(1, Ordering::SeqCst) + 1;
                // Track the maximum number of concurrent holders.
                max.fetch_max(current, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(10));
                ctr.fetch_sub(1, Ordering::SeqCst);
                release_file_lock(&ld);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // At most one thread should have held the lock at a time.
        assert_eq!(max_concurrent.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn file_lock_fails_when_parent_directory_does_not_exist() {
        let lock_dir = PathBuf::from("/nonexistent/path/.install-lock");
        // mkdir on a nonexistent parent should fail immediately and keep failing
        // until timeout. We can't wait 5 minutes, so just verify the underlying
        // mkdir fails.
        let result = std::fs::create_dir(&lock_dir);
        assert!(result.is_err());
    }

    // -- do_install tests --

    #[test]
    fn do_install_returns_early_if_already_installed() {
        let dir = TempDir::new().unwrap();
        let venv_dir = dir.path().join("venv");
        let version_file = venv_dir.join("hegel-version");
        let hegel_bin = venv_dir.join("bin/hegel");
        let install_log = dir.path().join("install.log");

        // Set up a "pre-installed" state.
        std::fs::create_dir_all(venv_dir.join("bin")).unwrap();
        std::fs::write(&hegel_bin, "fake-binary").unwrap();
        std::fs::write(&version_file, HEGEL_SERVER_VERSION).unwrap();

        // do_install should return immediately without running uv.
        let result = do_install(&venv_dir, &version_file, &hegel_bin, &install_log);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hegel_bin.to_string_lossy());
    }

    #[test]
    fn do_install_fails_when_uv_not_found() {
        let dir = TempDir::new().unwrap();
        let venv_dir = dir.path().join("venv");
        let version_file = venv_dir.join("hegel-version");
        let hegel_bin = venv_dir.join("bin/hegel");
        let install_log = dir.path().join("install.log");

        // Ensure uv won't be found by setting PATH to empty.
        let _guard = lock_env();
        let old_path = std::env::var("PATH").ok();
        unsafe { set_env("PATH", "") };
        let result = do_install(&venv_dir, &version_file, &hegel_bin, &install_log);
        unsafe { restore_env("PATH", old_path) };

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("uv") && (err.contains("not found") || err.contains("PATH")),
            "expected uv-not-found error, got: {err}"
        );
    }

    #[test]
    fn do_install_fails_when_uv_venv_fails() {
        let dir = TempDir::new().unwrap();
        let venv_dir = dir.path().join("venv");
        let version_file = venv_dir.join("hegel-version");
        let hegel_bin = venv_dir.join("bin/hegel");
        let install_log = dir.path().join("install.log");

        // Create a script that pretends to be uv but fails on "venv".
        let fake_bin = dir.path().join("fake-bin");
        std::fs::create_dir_all(&fake_bin).unwrap();
        let fake_uv = fake_bin.join("uv");
        std::fs::write(&fake_uv, "#!/bin/sh\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake_uv, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let _guard = lock_env();
        let old_path = std::env::var("PATH").ok();
        unsafe { set_env("PATH", fake_bin.to_string_lossy().as_ref()) };
        let result = do_install(&venv_dir, &version_file, &hegel_bin, &install_log);
        unsafe { restore_env("PATH", old_path) };

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("uv venv failed"),
            "expected venv failure, got: {err}"
        );
    }

    #[test]
    fn do_install_fails_when_pip_install_fails() {
        let dir = TempDir::new().unwrap();
        let venv_dir = dir.path().join("venv");
        let version_file = venv_dir.join("hegel-version");
        let hegel_bin = venv_dir.join("bin/hegel");
        let install_log = dir.path().join("install.log");

        // Create a fake uv that succeeds on "venv" (creates the dir) but fails on "pip".
        let fake_bin = dir.path().join("fake-bin");
        std::fs::create_dir_all(&fake_bin).unwrap();
        let fake_uv = fake_bin.join("uv");
        let venv_str = venv_dir.to_string_lossy().to_string();
        std::fs::write(
            &fake_uv,
            format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = \"venv\" ]; then\n\
                   mkdir -p \"{venv_str}/bin\"\n\
                   touch \"{venv_str}/bin/python\"\n\
                   exit 0\n\
                 fi\n\
                 exit 1\n"
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake_uv, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let _guard = lock_env();
        let old_path = std::env::var("PATH").ok();
        unsafe { set_env("PATH", fake_bin.to_string_lossy().as_ref()) };
        let result = do_install(&venv_dir, &version_file, &hegel_bin, &install_log);
        unsafe { restore_env("PATH", old_path) };

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Failed to install hegel-core"),
            "expected pip install failure, got: {err}"
        );
    }

    #[test]
    fn do_install_fails_when_binary_missing_after_install() {
        let dir = TempDir::new().unwrap();
        let venv_dir = dir.path().join("venv");
        let version_file = venv_dir.join("hegel-version");
        let hegel_bin = venv_dir.join("bin/hegel");
        let install_log = dir.path().join("install.log");

        // Fake uv that succeeds for both commands but doesn't create the hegel binary.
        let fake_bin = dir.path().join("fake-bin");
        std::fs::create_dir_all(&fake_bin).unwrap();
        let fake_uv = fake_bin.join("uv");
        let venv_str = venv_dir.to_string_lossy().to_string();
        std::fs::write(
            &fake_uv,
            format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = \"venv\" ]; then\n\
                   mkdir -p \"{venv_str}/bin\"\n\
                   touch \"{venv_str}/bin/python\"\n\
                 fi\n\
                 exit 0\n"
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake_uv, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let _guard = lock_env();
        let old_path = std::env::var("PATH").ok();
        unsafe { set_env("PATH", fake_bin.to_string_lossy().as_ref()) };
        let result = do_install(&venv_dir, &version_file, &hegel_bin, &install_log);
        unsafe { restore_env("PATH", old_path) };

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("not found at") && err.contains("after installation"),
            "expected binary-missing error, got: {err}"
        );
    }

    // -- ensure_hegel_installed tests --

    #[test]
    fn ensure_hegel_installed_fast_path_when_already_installed() {
        let _guard = lock_env();

        // Point XDG_CACHE_HOME at a temp dir and pre-populate it.
        let dir = TempDir::new().unwrap();
        let old_xdg = std::env::var("XDG_CACHE_HOME").ok();
        unsafe { set_env("XDG_CACHE_HOME", dir.path()) };

        let version_dir = dir.path().join("hegel/versions").join(HEGEL_SERVER_VERSION);
        let venv_dir = version_dir.join("venv");
        std::fs::create_dir_all(venv_dir.join("bin")).unwrap();
        std::fs::write(venv_dir.join("bin/hegel"), "fake").unwrap();
        std::fs::write(venv_dir.join("hegel-version"), HEGEL_SERVER_VERSION).unwrap();

        let result = ensure_hegel_installed();
        unsafe { restore_env("XDG_CACHE_HOME", old_xdg) };

        assert!(result.is_ok());
        assert!(result.unwrap().contains(HEGEL_SERVER_VERSION));
    }

    #[test]
    fn ensure_hegel_installed_creates_version_directory() {
        let _guard = lock_env();

        let dir = TempDir::new().unwrap();
        let old_xdg = std::env::var("XDG_CACHE_HOME").ok();
        let old_path = std::env::var("PATH").ok();
        unsafe { set_env("XDG_CACHE_HOME", dir.path()) };
        // Empty PATH so uv won't be found — we just want to verify
        // directory creation happens before the uv error.
        unsafe { set_env("PATH", "") };

        let result = ensure_hegel_installed();

        unsafe { restore_env("XDG_CACHE_HOME", old_xdg) };
        unsafe { restore_env("PATH", old_path) };

        // It should fail (no uv) but the version directory should have been created.
        assert!(result.is_err());
        let version_dir = dir.path().join("hegel/versions").join(HEGEL_SERVER_VERSION);
        assert!(version_dir.is_dir());
    }

    #[test]
    fn ensure_hegel_installed_cleans_up_lock_on_failure() {
        let _guard = lock_env();

        let dir = TempDir::new().unwrap();
        let old_xdg = std::env::var("XDG_CACHE_HOME").ok();
        let old_path = std::env::var("PATH").ok();
        unsafe { set_env("XDG_CACHE_HOME", dir.path()) };
        unsafe { set_env("PATH", "") };

        let _ = ensure_hegel_installed();

        unsafe { restore_env("XDG_CACHE_HOME", old_xdg) };
        unsafe { restore_env("PATH", old_path) };

        // The lock directory should have been cleaned up even though install failed.
        let lock_dir = dir
            .path()
            .join("hegel/versions")
            .join(HEGEL_SERVER_VERSION)
            .join(".install-lock");
        assert!(
            !lock_dir.exists(),
            "lock directory should be cleaned up after failed install"
        );
    }

    // -- Helper for cleanup in panicking tests --

    struct Defer<F: FnOnce()>(Option<F>);

    impl<F: FnOnce()> Drop for Defer<F> {
        fn drop(&mut self) {
            if let Some(f) = self.0.take() {
                f();
            }
        }
    }

    fn defer<F: FnOnce()>(f: F) -> Defer<F> {
        Defer(Some(f))
    }
}

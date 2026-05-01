use crate::backend::{DataSource, TestCaseResult, TestRunResult, TestRunner};
use crate::cbor_utils::{as_bool, as_text, as_u64, cbor_map, map_get, map_insert};
use crate::runner::{Database, HealthCheck, Mode, Phase, Settings, Verbosity};
use crate::server::protocol::{Connection, HANDSHAKE_STRING, Stream};
use ciborium::Value;

use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::data_source::ServerDataSource;
use super::process::{
    HEGEL_SERVER_COMMAND_ENV, handle_channel_error, handle_handshake_failure, hegel_command,
    server_crash_message, server_log_file,
};
use super::runner::{cbor_decode, cbor_encode};

pub(super) const SUPPORTED_PROTOCOL_VERSIONS: (&str, &str) = ("0.11", "0.12");
pub(super) const HEGEL_SERVER_VERSION: &str = "0.6.0";

pub(super) static SESSION: Mutex<Option<Arc<HegelSession>>> = Mutex::new(None);

fn health_check_as_str(check: &HealthCheck) -> &'static str {
    match check {
        HealthCheck::FilterTooMuch => "filter_too_much",
        HealthCheck::TooSlow => "too_slow",
        HealthCheck::TestCasesTooLarge => "test_cases_too_large",
        HealthCheck::LargeInitialTestCase => "large_initial_test_case",
    }
}

fn phase_as_str(phase: &Phase) -> &'static str {
    match phase {
        Phase::Explicit => "explicit",
        Phase::Reuse => "reuse",
        Phase::Generate => "generate",
        Phase::Target => "target",
        Phase::Shrink => "shrink",
        Phase::Explain => "explain",
    }
}

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
pub(super) struct HegelSession {
    pub(super) connection: Arc<Connection>,
    /// The control stream is shared across threads, so it's behind a Mutex
    /// because Stream is not thread-safe. The lock is only held for the
    /// brief run_test send/receive; test execution runs concurrently on
    /// per-test streams.
    control: Mutex<Stream>,
    /// The server subprocess. Shared with the monitor thread so that
    /// `__test_kill_server` can call `child.kill()` directly rather than
    /// shelling out to the OS `kill` command.
    pub(super) child: Arc<Mutex<std::process::Child>>,
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
        super::runner::init_panic_hook();
        let session = Arc::new(HegelSession::init());
        *guard = Some(Arc::clone(&session));
        session
    }

    fn init() -> HegelSession {
        let mut cmd = hegel_command();
        cmd.arg("--verbosity").arg("normal");

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

/// Test runner that communicates with the hegel-core server.
pub(crate) struct ServerTestRunner;

impl ServerTestRunner {
    fn run_single_test_case(
        &self,
        settings: &Settings,
        run_case: &mut dyn FnMut(Box<dyn DataSource>, bool) -> TestCaseResult,
    ) -> TestRunResult {
        let session = HegelSession::get();
        let connection = &session.connection;
        let verbosity = settings.verbosity;

        let mut test_stream = connection.new_stream();

        let mut msg = cbor_map! {
            "command" => "single_test_case",
            "stream_id" => test_stream.stream_id
        };
        if let Some(seed) = settings.seed {
            map_insert(&mut msg, "seed", seed);
        }

        let response = {
            let mut control = session.control.lock().unwrap_or_else(|e| e.into_inner());
            let send_id = control.send_request(cbor_encode(&msg));
            send_id.and_then(|id| control.receive_reply(id))
        }
        .unwrap_or_else(|e| handle_channel_error(e));
        let _: Value = cbor_decode(&response);

        if verbosity == Verbosity::Debug {
            eprintln!("single_test_case response received");
        }

        let ack_null = cbor_map! {"result" => Value::Null};
        let mut failure_message: Option<String> = None;
        let mut passed = true;

        loop {
            let (event_id, event_payload) = receive_event(&mut test_stream, connection);

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

                    test_stream
                        .write_reply(event_id, cbor_encode(&ack_null))
                        .expect("Failed to ack test_case");

                    let backend = Box::new(ServerDataSource::new(
                        Arc::clone(connection),
                        test_case_stream,
                        verbosity,
                    ));
                    let tc_result = run_case(backend, true);

                    if let TestCaseResult::Interesting { panic_message } = tc_result {
                        passed = false;
                        failure_message = Some(panic_message);
                    }
                }
                "test_done" => {
                    let ack_true = cbor_map! {"result" => true};
                    test_stream
                        .write_reply(event_id, cbor_encode(&ack_true))
                        .expect("Failed to ack test_done");
                    break;
                }
                _ => {
                    panic!("unknown event: {}", event_type); // nocov
                }
            }
        }

        TestRunResult {
            passed,
            failure_message,
        }
    }
}

impl TestRunner for ServerTestRunner {
    fn run(
        &self,
        settings: &Settings,
        database_key: Option<&str>,
        run_case: &mut dyn FnMut(Box<dyn DataSource>, bool) -> TestCaseResult,
    ) -> TestRunResult {
        if settings.mode == Mode::SingleTestCase {
            return self.run_single_test_case(settings, run_case);
        }

        let session = HegelSession::get();
        let connection = &session.connection;
        let verbosity = settings.verbosity;

        let mut test_stream = connection.new_stream();

        let suppress_names: Vec<Value> = settings
            .suppress_health_check
            .iter()
            .map(|c| Value::Text(health_check_as_str(c).to_string()))
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
        let phase_names: Vec<Value> = settings
            .phases
            .iter()
            .map(|p| Value::Text(phase_as_str(p).to_string()))
            .collect();
        if let Value::Map(ref mut map) = run_test_msg {
            map.push((Value::Text("phases".to_string()), Value::Array(phase_names)));
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
            let (event_id, event_payload) = receive_event(&mut test_stream, connection);

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

                    if verbosity == Verbosity::Verbose {
                        eprintln!("Trying example: ");
                    }
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

fn receive_event(test_stream: &mut Stream, connection: &Connection) -> (u32, Vec<u8>) {
    match test_stream.receive_request() {
        Ok(event) => event,
        // nocov start
        Err(_) if connection.server_has_exited() => {
            panic!("{}", server_crash_message());
            // nocov end
        }
        Err(e) => unreachable!("Failed to receive event (server still running): {}", e),
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/server/session_tests.rs"]
mod tests;

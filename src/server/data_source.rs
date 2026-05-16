use crate::backend::{DataSource, DataSourceError, TestCaseResult};
use crate::cbor_utils::{cbor_map, map_insert};
use crate::runner::Verbosity;
use crate::server::protocol::{Connection, Stream};
use ciborium::Value;

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use super::process::server_crash_message;

/// Per-test-case outcome handle shared between [`ServerDataSource`] and
/// the engine that owns it.  Populated by `mark_complete` and consumed by
/// the engine via [`ServerDataSource::take_outcome`] after the test body
/// returns — same shape as the native backend's outcome channel, so both
/// backends go through the [`DataSource`] interface for per-test-case
/// results.
pub(crate) type ServerOutcomeHandle = Arc<Mutex<Option<TestCaseResult>>>;

/// Read `HEGEL_PROTOCOL_DEBUG` and decide whether protocol debug logging is on.
/// Extracted so tests can exercise the env-var → bool mapping without going
/// through `PROTOCOL_DEBUG`'s LazyLock cache (which is sensitive to whichever
/// test happens to access it first in a binary).
fn protocol_debug_from_env() -> bool {
    matches!(
        std::env::var("HEGEL_PROTOCOL_DEBUG")
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "1" | "true"
    )
}

static PROTOCOL_DEBUG: LazyLock<bool> = LazyLock::new(protocol_debug_from_env);

/// Backend implementation that communicates with the hegel-core server
/// over a multiplexed stream.
pub(crate) struct ServerDataSource {
    connection: Arc<Connection>,
    stream: Mutex<Stream>,
    aborted: AtomicBool,
    verbosity: Verbosity,
    /// Labels seen by `target_observation` this test case. Used to reject
    /// duplicate observations of the same label, mirroring
    /// upstream `hypothesis.control.target` (`control.py:354-356,372-376`).
    /// One `ServerDataSource` is constructed per test case (see
    /// `session.rs:236,378,443`), so this is per-test-case state.
    target_labels: Mutex<HashSet<String>>,
    /// Outcome reported by [`DataSource::mark_complete`].  Shared with the
    /// engine via [`ServerOutcomeHandle`] so the engine reads it back
    /// through the same `DataSource` interface the native backend uses.
    outcome: ServerOutcomeHandle,
}

impl ServerDataSource {
    pub(crate) fn new(
        connection: Arc<Connection>,
        stream: Stream,
        verbosity: Verbosity,
    ) -> (Self, ServerOutcomeHandle) {
        let outcome: ServerOutcomeHandle = Arc::new(Mutex::new(None));
        let handle = Arc::clone(&outcome);
        (
            ServerDataSource {
                connection,
                stream: Mutex::new(stream),
                aborted: AtomicBool::new(false),
                verbosity,
                target_labels: Mutex::new(HashSet::new()),
                outcome,
            },
            handle,
        )
    }

    /// Read the outcome reported via [`DataSource::mark_complete`].
    ///
    /// Panics if `mark_complete` was never called; the cross-backend
    /// lifecycle in `run_lifecycle::run_test_case` guarantees it always is.
    pub(crate) fn take_outcome(handle: &ServerOutcomeHandle) -> TestCaseResult {
        handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .expect("mark_complete must be called for every test case")
    }

    fn send_request(&self, command: &str, payload: &Value) -> Result<Value, DataSourceError> {
        if self.aborted.load(Ordering::SeqCst) {
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

        let result = self
            .stream
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .request_cbor(&request);

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
                    self.stream
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mark_closed();
                    self.aborted.store(true, Ordering::SeqCst);
                    Err(DataSourceError::StopTest)
                // nocov start
                } else if error_msg.contains("FlakyStrategyDefinition")
                    || error_msg.contains("FlakyReplay")
                // nocov end
                {
                    self.stream
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .mark_closed();
                    self.aborted.store(true, Ordering::SeqCst);
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

    fn new_collection(&self, min_size: u64, max_size: Option<u64>) -> Result<i64, DataSourceError> {
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
                Ok(n as i64)
            }
            // nocov start
            _ => panic!(
                "Expected integer response from new_collection, got {:?}",
                response
            ),
            // nocov end
        }
    }

    fn collection_more(&self, collection_id: i64) -> Result<bool, DataSourceError> {
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
        collection_id: i64,
        why: Option<&str>,
    ) -> Result<(), DataSourceError> {
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

    fn target_observation(&self, score: f64, label: &str) {
        // Mirror `NativeDataSource::target_observation` (post-A16) and
        // upstream `hypothesis.control.target` (`control.py:354-356,372-376`):
        // observations must be finite and each label may be observed at
        // most once per test case. Pre-N8 these were silently forwarded to
        // the Python server, surfacing as a CBOR round-trip error rather
        // than a clean client-side panic.
        if !score.is_finite() {
            panic!(
                "tc.target({score}, label={label:?}) requires a finite score; \
                 got non-finite value"
            );
        }
        let mut seen = self.target_labels.lock().unwrap_or_else(|e| e.into_inner());
        if !seen.insert(label.to_string()) {
            panic!(
                "tc.target({score}, label={label:?}) would overwrite previous \
                 tc.target(_, label={label:?}); each label can be observed at \
                 most once per test case"
            );
        }
        drop(seen);
        let _ = self.send_request(
            "target",
            &cbor_map! {
                "value" => score,
                "label" => label.to_string()
            },
        );
    }

    fn mark_complete(&self, result: &TestCaseResult) {
        // Record the outcome on the shared handle for the engine — same
        // path the native backend uses.
        *self.outcome.lock().unwrap_or_else(|e| e.into_inner()) = Some(result.clone());

        // Don't forward to the server if the test case aborted mid-draw
        // (StopTest / overflow): the server has already closed the stream
        // and is no longer interested in this test case's verdict.
        if self.aborted.load(Ordering::SeqCst) {
            return;
        }

        let (status, origin) = match result {
            TestCaseResult::Valid => ("VALID", None),
            TestCaseResult::Invalid | TestCaseResult::Overrun => ("INVALID", None),
            TestCaseResult::Interesting(f) => ("INTERESTING", Some(f.origin.as_str())),
        };
        let origin_value = match origin {
            Some(s) => Value::Text(s.to_string()),
            None => Value::Null,
        };
        let mark_complete = cbor_map! {
            "command" => "mark_complete",
            "status" => status,
            "origin" => origin_value
        };
        let mut stream = self.stream.lock().unwrap_or_else(|e| e.into_inner());
        let _ = stream.request_cbor(&mark_complete);
        let _ = stream.close();
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/server/data_source_tests.rs"]
mod tests;

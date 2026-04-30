use crate::backend::{DataSource, DataSourceError};
use crate::cbor_utils::{cbor_map, map_insert};
use crate::runner::Verbosity;
use crate::server::protocol::{Connection, Stream};
use ciborium::Value;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use super::process::server_crash_message;

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
    stream: Mutex<Stream>,
    aborted: AtomicBool,
    verbosity: Verbosity,
}

impl ServerDataSource {
    pub(crate) fn new(connection: Arc<Connection>, stream: Stream, verbosity: Verbosity) -> Self {
        ServerDataSource {
            connection,
            stream: Mutex::new(stream),
            aborted: AtomicBool::new(false),
            verbosity,
        }
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
        if self.aborted.load(Ordering::SeqCst) {
            return;
        }
        let _ = self.send_request(
            "target",
            &cbor_map! {
                "value" => score,
                "label" => label.to_string()
            },
        );
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
        let mut stream = self.stream.lock().unwrap_or_else(|e| e.into_inner());
        let _ = stream.request_cbor(&mark_complete);
        let _ = stream.close();
    }

    fn test_aborted(&self) -> bool {
        self.aborted.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/server/data_source_tests.rs"]
mod tests;

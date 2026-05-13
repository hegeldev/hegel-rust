// Native backend DataSource implementation.
//
// Wraps a NativeTestCase behind interior mutability so it can implement
// the DataSource trait (which takes &self).  The handle is shared with
// the engine so it can read back the recorded nodes / spans and, via
// `mark_complete`, the test case outcome after the test body returns.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ciborium::Value;

use crate::backend::{DataSource, DataSourceError, TestCaseResult};
use crate::native::core::{ChoiceNode, NativeTestCase, Span};
use crate::native::schema;

/// Per-test-case state shared between `NativeDataSource` and the engine
/// that owns the handle.  The engine constructs both halves up-front,
/// hands the data source into the test body, and then reads back nodes,
/// spans, and the outcome (populated by `mark_complete`) through the
/// handle.
pub struct NativeTestCaseInner {
    pub ntc: NativeTestCase,
    /// Set by [`DataSource::mark_complete`] after the test body returns.
    /// `None` only if `mark_complete` was never called — which the lifecycle
    /// in `run_lifecycle::run_test_case` guarantees won't happen — so the
    /// engine can safely unwrap when reading the outcome back.
    pub outcome: Option<TestCaseResult>,
}

/// Shared handle to the per-test-case inner state.
pub type NativeTestCaseHandle = Arc<Mutex<NativeTestCaseInner>>;

pub struct NativeDataSource {
    inner: NativeTestCaseHandle,
    aborted: AtomicBool,
}

impl NativeDataSource {
    /// Create a new `NativeDataSource` and return a shared handle.
    ///
    /// The handle is the only way the engine reads back per-test-case
    /// state: choice nodes, spans, and the outcome reported by
    /// [`DataSource::mark_complete`].
    pub fn new(ntc: NativeTestCase) -> (Self, NativeTestCaseHandle) {
        let inner = Arc::new(Mutex::new(NativeTestCaseInner { ntc, outcome: None }));
        let handle = Arc::clone(&inner);
        (
            NativeDataSource {
                inner,
                aborted: AtomicBool::new(false),
            },
            handle,
        )
    }

    /// Convenience: extract choice nodes from a handle after a test case.
    pub fn take_nodes(handle: &NativeTestCaseHandle) -> Vec<ChoiceNode> {
        handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ntc
            .nodes
            .clone()
    }

    /// Convenience: extract spans from a handle after a test case.
    pub fn take_spans(handle: &NativeTestCaseHandle) -> Vec<Span> {
        handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ntc
            .spans
            .clone()
            .into_vec()
    }

    /// Read the outcome reported via [`DataSource::mark_complete`].
    ///
    /// Panics if `mark_complete` was never called; the cross-backend
    /// lifecycle in `run_lifecycle::run_test_case` guarantees it always is.
    pub fn take_outcome(handle: &NativeTestCaseHandle) -> TestCaseResult {
        handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .outcome
            .take()
            .expect("mark_complete must be called for every test case")
    }

    /// Returns true if a previous request triggered a StopTest abort.
    /// Test-only helper — not part of the `DataSource` interface, so
    /// callers must hold a concrete `&NativeDataSource`.
    #[cfg(test)]
    pub(crate) fn test_aborted(&self) -> bool {
        self.aborted.load(Ordering::Relaxed)
    }

    fn dispatch(&self, command: &str, payload: &Value) -> Result<Value, DataSourceError> {
        if self.aborted.load(Ordering::Relaxed) {
            return Err(DataSourceError::StopTest);
        }
        let mut inner = self.inner.lock().unwrap();
        schema::dispatch_request(&mut inner.ntc, command, payload).map_err(|_stop| {
            self.aborted.store(true, Ordering::Relaxed);
            DataSourceError::StopTest
        })
    }
}

impl DataSource for NativeDataSource {
    fn generate(&self, schema: &Value) -> Result<Value, DataSourceError> {
        use crate::cbor_utils::cbor_map;
        self.dispatch("generate", &cbor_map! {"schema" => schema.clone()})
    }

    fn start_span(&self, label: u64) -> Result<(), DataSourceError> {
        use crate::cbor_utils::cbor_map;
        self.dispatch("start_span", &cbor_map! {"label" => label})?;
        Ok(())
    }

    fn stop_span(&self, discard: bool) -> Result<(), DataSourceError> {
        use crate::cbor_utils::cbor_map;
        self.dispatch("stop_span", &cbor_map! {"discard" => discard})?;
        Ok(())
    }

    fn new_collection(&self, min_size: u64, max_size: Option<u64>) -> Result<i64, DataSourceError> {
        use crate::cbor_utils::{cbor_map, map_insert};
        let mut payload = cbor_map! { "min_size" => min_size };
        if let Some(max) = max_size {
            map_insert(&mut payload, "max_size", max);
        }
        let Value::Integer(i) = self.dispatch("new_collection", &payload)? else {
            unreachable!("new_collection always returns Value::Integer")
        };
        Ok(i128::from(i) as i64)
    }

    fn collection_more(&self, collection_id: i64) -> Result<bool, DataSourceError> {
        use crate::cbor_utils::cbor_map;
        let response = self.dispatch(
            "collection_more",
            &cbor_map! { "collection_id" => collection_id },
        )?;
        let Value::Bool(b) = response else {
            unreachable!("collection_more always returns Value::Bool")
        };
        Ok(b)
    }

    fn collection_reject(
        &self,
        collection_id: i64,
        why: Option<&str>,
    ) -> Result<(), DataSourceError> {
        use crate::cbor_utils::{cbor_map, map_insert};
        let mut payload = cbor_map! { "collection_id" => collection_id };
        if let Some(reason) = why {
            map_insert(&mut payload, "why", reason.to_string());
        }
        self.dispatch("collection_reject", &payload)?;
        Ok(())
    }

    fn new_pool(&self) -> Result<i128, DataSourceError> {
        use crate::cbor_utils::cbor_map;
        let Value::Integer(i) = self.dispatch("new_pool", &cbor_map! {})? else {
            unreachable!("new_pool always returns Value::Integer")
        };
        Ok(i.into())
    }

    fn pool_add(&self, pool_id: i128) -> Result<i128, DataSourceError> {
        use crate::cbor_utils::cbor_map;
        let Value::Integer(i) = self.dispatch("pool_add", &cbor_map! {"pool_id" => pool_id})?
        else {
            unreachable!("pool_add always returns Value::Integer")
        };
        Ok(i.into())
    }

    fn pool_generate(&self, pool_id: i128, consume: bool) -> Result<i128, DataSourceError> {
        use crate::cbor_utils::cbor_map;
        let Value::Integer(i) = self.dispatch(
            "pool_generate",
            &cbor_map! {
                "pool_id" => pool_id,
                "consume" => consume,
            },
        )?
        else {
            unreachable!("pool_generate always returns Value::Integer")
        };
        Ok(i.into())
    }

    fn target_observation(&self, _score: f64, _label: &str) {
        todo!(
            "tc.target() is not yet supported by the native backend; \
             Phase::Target will land in a follow-up PR"
        );
    }

    fn mark_complete(&self, result: &TestCaseResult) {
        // Record the outcome on the shared handle so the engine can read
        // it via `take_outcome` after the test body returns.  This is the
        // sole cross-backend channel for per-test-case results — both the
        // server backend and this one consume `mark_complete` through the
        // same `DataSource` interface.
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.outcome = Some(result.clone());
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/data_source_tests.rs"]
mod tests;

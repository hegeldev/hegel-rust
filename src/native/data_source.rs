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
use crate::native::core::{ChoiceNode, ManyState, NativeTestCase, Span, StopTest};
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

    /// Acquire the test-case state under the abort guard.  Returns
    /// `StopTest` immediately if a previous call has already aborted
    /// the test case so subsequent draws short-circuit without
    /// touching `ntc`.
    fn with_ntc<R>(
        &self,
        f: impl FnOnce(&mut NativeTestCase) -> Result<R, StopTest>,
    ) -> Result<R, DataSourceError> {
        if self.aborted.load(Ordering::Relaxed) {
            return Err(DataSourceError::StopTest);
        }
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut inner.ntc).map_err(|_stop| {
            self.aborted.store(true, Ordering::Relaxed);
            DataSourceError::StopTest
        })
    }
}

impl DataSource for NativeDataSource {
    fn generate(&self, schema: &Value) -> Result<Value, DataSourceError> {
        self.with_ntc(|ntc| schema::interpret_schema(ntc, schema))
    }

    fn start_span(&self, label: u64) -> Result<(), DataSourceError> {
        self.with_ntc(|ntc| {
            ntc.start_span(label);
            Ok(())
        })
    }

    fn stop_span(&self, discard: bool) -> Result<(), DataSourceError> {
        self.with_ntc(|ntc| {
            ntc.stop_span(discard);
            Ok(())
        })
    }

    fn new_collection(&self, min_size: u64, max_size: Option<u64>) -> Result<i64, DataSourceError> {
        self.with_ntc(|ntc| {
            let state = ManyState::new(min_size as usize, max_size.map(|n| n as usize));
            Ok(ntc.new_collection(state))
        })
    }

    fn collection_more(&self, collection_id: i64) -> Result<bool, DataSourceError> {
        self.with_ntc(|ntc| {
            let mut state = ntc
                .collections
                .remove(&collection_id)
                .expect("collection_more: unknown collection_id");
            let result = schema::many_more(ntc, &mut state);
            ntc.collections.insert(collection_id, state);
            result
        })
    }

    fn collection_reject(
        &self,
        collection_id: i64,
        _why: Option<&str>,
    ) -> Result<(), DataSourceError> {
        self.with_ntc(|ntc| {
            let mut state = ntc
                .collections
                .remove(&collection_id)
                .expect("collection_reject: unknown collection_id");
            let result = schema::many_reject(ntc, &mut state);
            ntc.collections.insert(collection_id, state);
            result
        })
    }

    fn new_pool(&self) -> Result<i128, DataSourceError> {
        self.with_ntc(|ntc| {
            let pool_id = ntc.variable_pools.len() as i128;
            ntc.variable_pools
                .push(crate::native::core::NativeVariables::new());
            Ok(pool_id)
        })
    }

    fn pool_add(&self, pool_id: i128) -> Result<i128, DataSourceError> {
        self.with_ntc(|ntc| Ok(ntc.variable_pools[pool_id as usize].next()))
    }

    fn pool_generate(&self, pool_id: i128, consume: bool) -> Result<i128, DataSourceError> {
        self.with_ntc(|ntc| {
            let pool_idx = pool_id as usize;
            let active = ntc.variable_pools[pool_idx].active();
            if active.is_empty() {
                // No variables available: mark the test case invalid.
                return Err(StopTest);
            }
            let n = active.len() as i128;
            // Draw index from `[0, n-1]`.  Shrink towards `n-1`
            // (last added = most recent) by drawing `k` from
            // `[0, n-1]` and using `index = n-1-k`.
            let k = ntc.draw_integer(0, n - 1)?;
            let variable_id = active[(n - 1 - k) as usize];
            if consume {
                ntc.variable_pools[pool_idx].consume(variable_id);
            }
            Ok(variable_id)
        })
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

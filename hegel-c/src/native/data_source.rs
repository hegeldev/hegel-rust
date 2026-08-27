use crate::native::HashMap;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::backend::{DataSource, DataSourceError, Failure, RunError, TestCaseResult};
use crate::native::bignum::BigInt;
use crate::native::core::{
    ChoiceNode, EngineError, InterestingOrigin, ManyState, NativeStateMachine, NativeTestCase,
    NativeTestCaseHandle, NativeVariables, RecursionState, Span, SpanEvent, Status,
};
use crate::native::draws;

pub struct NativeDataSource {
    inner: NativeTestCaseHandle,
    aborted: AtomicBool,
}

impl NativeDataSource {
    /// Create a new `NativeDataSource` and return a shared handle to its
    /// stream.
    ///
    /// The handle is the only way the engine reads back per-test-case
    /// state: choice nodes, spans, and the outcome reported by
    /// [`DataSource::mark_complete`].
    pub fn new(ntc: NativeTestCase) -> (Self, NativeTestCaseHandle) {
        let handle: NativeTestCaseHandle = Arc::new(crate::sys::sync::Mutex::new(ntc));
        (Self::from_handle(Arc::clone(&handle)), handle)
    }

    /// Wrap an existing stream handle — used for the root stream (via
    /// [`Self::new`]) and for cloned streams (via
    /// [`DataSource::clone_stream`]). Each wrapper has its own abort latch,
    /// so one stream aborting on overrun doesn't mark its siblings' sources
    /// aborted.
    fn from_handle(handle: NativeTestCaseHandle) -> Self {
        NativeDataSource {
            inner: handle,
            aborted: AtomicBool::new(false),
        }
    }

    /// Convenience: extract choice nodes from a handle after a test case.
    ///
    /// Reassembles first, so once the family has concluded every clone node
    /// carries its stream's realized record and the returned sequence is the
    /// self-contained pieced-together choice sequence of the whole family.
    pub fn take_nodes(handle: &NativeTestCaseHandle) -> Vec<ChoiceNode> {
        let mut ntc = handle.lock();
        ntc.reassemble();
        ntc.nodes.clone()
    }

    /// Convenience: extract spans from a handle after a test case.
    pub fn take_spans(handle: &NativeTestCaseHandle) -> Vec<Span> {
        handle.lock().spans.clone().into_vec()
    }

    /// Convenience: extract the live span-open/close events (with their draw
    /// positions) recorded during the test case, so the engine can fold them
    /// into the choice tree for faithful replay.
    pub fn take_span_events(handle: &NativeTestCaseHandle) -> Vec<(usize, SpanEvent)> {
        handle.lock().span_events.clone()
    }

    /// Read the `tc.target()` observations the test body recorded.
    ///
    /// Used by the targeting phase in `test_runner` to read back per-label
    /// scores after a test case completes. Returns a clone without mutating
    /// the shared state: the handle may still be shared with a run-owned
    /// [`crate::HegelTestCase`], so reading it must not perturb it.
    pub fn take_target_observations(handle: &NativeTestCaseHandle) -> HashMap<String, f64> {
        handle.lock().family().target_observations.lock().clone()
    }

    /// The test case's outcome, reconstructed from its family's write-once
    /// conclusion. Whoever concluded the family first — a draw that overran
    /// or hit a terminal assume, or the body via `mark_complete` — set the
    /// status, and a later report could not change it.
    ///
    /// `Err` if the family never concluded — i.e. the driver resumed the
    /// engine without calling `mark_complete` on a case that didn't conclude
    /// during a draw. That violates the driving contract every client must
    /// uphold (libhegel's `hegel_next_test_case` refuses to resume an
    /// unconcluded run, so its callers can't reach this), reported as a
    /// run-level [`RunError::UsageError`] rather than a panic.
    pub fn take_outcome(handle: &NativeTestCaseHandle) -> Result<TestCaseResult, RunError> {
        let conclusion = handle.lock().family().conclusion();
        let Some((status, origin)) = conclusion else {
            return Err(RunError::UsageError(
                "the test case was never marked complete: every test case the \
                 engine offers must be concluded with mark_complete before the \
                 run is resumed"
                    .to_string(),
            ));
        };
        Ok(match status {
            Status::Valid => TestCaseResult::Valid,
            Status::Invalid => TestCaseResult::Invalid,
            Status::EarlyStop => TestCaseResult::Overrun,
            Status::Interesting => TestCaseResult::Interesting(Failure {
                origin: origin.map(|o| o.0).unwrap_or_default(),
                reproduce_blob: None,
            }),
        })
    }

    /// Returns true if a previous request triggered a EngineError abort.
    /// Test-only helper — not part of the `DataSource` interface, so
    /// callers must hold a concrete `&NativeDataSource`.
    #[cfg(test)]
    pub(crate) fn test_aborted(&self) -> bool {
        self.aborted.load(Ordering::Relaxed)
    }

    /// Acquire the test-case state under the abort guard.  Returns
    /// `DataSourceError::StopTest` immediately if a previous call has already
    /// aborted the test case so subsequent draws short-circuit without
    /// touching the stream.
    fn with_ntc<R>(
        &self,
        f: impl FnOnce(&mut NativeTestCase) -> Result<R, EngineError>,
    ) -> Result<R, DataSourceError> {
        if self.aborted.load(Ordering::Relaxed) {
            return Err(self.aborted_error());
        }
        let mut ntc = self.inner.lock();
        f(&mut ntc).map_err(|e| match e {
            EngineError::Overrun => {
                self.aborted.store(true, Ordering::Relaxed);
                DataSourceError::StopTest
            }
            EngineError::InvalidTestCase => {
                self.aborted.store(true, Ordering::Relaxed);
                DataSourceError::Assume
            }
            EngineError::AssumeViolation => DataSourceError::Assume,
            EngineError::InvalidArgument(msg) => DataSourceError::InvalidArgument(msg),
            EngineError::Internal(e) => DataSourceError::Internal(e),
        })
    }

    fn aborted_error(&self) -> DataSourceError {
        let status = self.inner.lock().status();
        match status {
            Some(Status::Invalid) => DataSourceError::Assume,
            _ => DataSourceError::StopTest,
        }
    }
}

impl DataSource for NativeDataSource {
    fn generate_integer(
        &self,
        min_value: &BigInt,
        max_value: &BigInt,
    ) -> Result<BigInt, DataSourceError> {
        self.with_ntc(|ntc| draws::generate_integer(ntc, min_value, max_value))
    }

    fn generate_float(
        &self,
        spec: &crate::native::draws::FloatSpec,
    ) -> Result<f64, DataSourceError> {
        self.with_ntc(|ntc| draws::generate_float(ntc, spec))
    }

    fn generate_string(
        &self,
        spec: &crate::native::draws::StringSpec,
    ) -> Result<String, DataSourceError> {
        self.with_ntc(|ntc| draws::generate_string(ntc, spec))
    }

    fn generate_date(
        &self,
        min: crate::native::draws::special::Date,
        max: crate::native::draws::special::Date,
    ) -> Result<crate::native::draws::special::Date, DataSourceError> {
        self.with_ntc(|ntc| crate::native::draws::special::generate_date(ntc, min, max))
    }

    fn generate_time(
        &self,
        min: crate::native::draws::special::Time,
        max: crate::native::draws::special::Time,
    ) -> Result<crate::native::draws::special::Time, DataSourceError> {
        self.with_ntc(|ntc| crate::native::draws::special::generate_time(ntc, min, max))
    }

    fn generate_datetime(
        &self,
        min: crate::native::draws::special::DateTime,
        max: crate::native::draws::special::DateTime,
    ) -> Result<crate::native::draws::special::DateTime, DataSourceError> {
        self.with_ntc(|ntc| crate::native::draws::special::generate_datetime(ntc, min, max))
    }

    fn generate_uuid(&self, version: Option<u8>) -> Result<[u8; 16], DataSourceError> {
        self.with_ntc(|ntc| crate::native::draws::special::generate_uuid(ntc, version))
    }

    fn generate_ipv4(&self) -> Result<core::net::Ipv4Addr, DataSourceError> {
        self.with_ntc(crate::native::draws::special::generate_ipv4)
    }

    fn generate_ipv6(&self) -> Result<core::net::Ipv6Addr, DataSourceError> {
        self.with_ntc(crate::native::draws::special::generate_ipv6)
    }

    fn generate_bytes(&self, min_size: usize, max_size: usize) -> Result<Vec<u8>, DataSourceError> {
        self.with_ntc(|ntc| draws::generate_bytes(ntc, min_size, max_size))
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

    fn clone_stream(&self) -> Result<Box<dyn DataSource + Send + Sync>, DataSourceError> {
        self.with_ntc(|ntc| ntc.clone_stream()).map(|handle| {
            Box::new(NativeDataSource::from_handle(handle)) as Box<dyn DataSource + Send + Sync>
        })
    }

    fn new_collection(
        &self,
        min_size: u64,
        max_size: Option<u64>,
    ) -> Result<ManyState, DataSourceError> {
        self.with_ntc(|_| {
            let min_size = usize::try_from(min_size).unwrap_or(usize::MAX);
            let max_size = max_size.map(|n| usize::try_from(n).unwrap_or(usize::MAX));
            Ok(ManyState::new(min_size, max_size))
        })
    }

    fn collection_more(&self, state: &mut ManyState) -> Result<bool, DataSourceError> {
        self.with_ntc(|ntc| draws::many_more(ntc, state))
    }

    fn collection_reject(
        &self,
        state: &mut ManyState,
        _why: Option<&str>,
    ) -> Result<(), DataSourceError> {
        self.with_ntc(|ntc| draws::many_reject(ntc, state))
    }

    fn new_recursion(
        &self,
        max_depth: u64,
        max_leaves: u64,
    ) -> Result<RecursionState, DataSourceError> {
        self.with_ntc(|ntc| {
            Ok(draws::new_recursion_state(
                max_depth,
                max_leaves,
                ntc.span_depth(),
            ))
        })
    }

    fn recursion_branch(
        &self,
        state: &RecursionState,
        depth: u64,
    ) -> Result<bool, DataSourceError> {
        self.with_ntc(|ntc| draws::recursion_branch(ntc, state, depth))
    }

    fn recursion_leaf(&self, state: &mut RecursionState) -> Result<bool, DataSourceError> {
        self.with_ntc(|_| Ok(state.count_leaf()))
    }

    fn recursion_retry(&self, state: &mut RecursionState) -> Result<(), DataSourceError> {
        self.with_ntc(|ntc| draws::recursion_retry(ntc, state))
    }

    fn new_state_machine(
        &self,
        rule_names: Vec<String>,
        rule_groups: Vec<i64>,
        _invariant_names: Vec<String>,
        min_concurrency: i64,
        max_concurrency: i64,
    ) -> Result<NativeStateMachine, DataSourceError> {
        if rule_names.is_empty() {
            return Err(DataSourceError::InvalidArgument(
                "cannot run a state machine with no rules".to_string(),
            ));
        }
        if rule_groups.len() != rule_names.len() {
            return Err(DataSourceError::InvalidArgument(format!(
                "rule_groups must be parallel to rule_names: got {} group assignments \
                 for {} rules",
                rule_groups.len(),
                rule_names.len()
            )));
        }
        if min_concurrency < 1 || max_concurrency < min_concurrency {
            return Err(DataSourceError::InvalidArgument(format!(
                "state machine concurrency bounds must satisfy 1 <= min <= max, \
                 got [{min_concurrency}, {max_concurrency}]"
            )));
        }
        self.with_ntc(|ntc| {
            if max_concurrency > 1 {
                let family = ntc.family();
                family.set_concurrent_machine();
                if family.reject_concurrent_machine() {
                    return Err(EngineError::AssumeViolation);
                }
            }
            NativeStateMachine::new(ntc, rule_groups, min_concurrency, max_concurrency)
        })
    }

    fn state_machine_next_group(
        &self,
        machine: &mut NativeStateMachine,
    ) -> Result<Option<i64>, DataSourceError> {
        self.with_ntc(|ntc| machine.next_group(ntc))
    }

    fn state_machine_next_rule(
        &self,
        machine: &mut NativeStateMachine,
        worker_index: i64,
    ) -> Result<Option<i64>, DataSourceError> {
        self.with_ntc(|ntc| machine.next_rule(ntc, worker_index))
    }

    fn state_machine_rule_rejected(
        &self,
        machine: &mut NativeStateMachine,
        worker_index: i64,
    ) -> Result<(), DataSourceError> {
        self.with_ntc(|_ntc| machine.rule_rejected(worker_index))
    }

    fn generate_boolean(&self, p: f64, forced: Option<bool>) -> Result<bool, DataSourceError> {
        self.with_ntc(|ntc| draws::generate_boolean(ntc, p, forced))
    }

    fn new_pool(&self) -> Result<NativeVariables, DataSourceError> {
        self.with_ntc(|_| Ok(NativeVariables::new()))
    }

    fn pool_add(&self, pool: &mut NativeVariables) -> Result<i64, DataSourceError> {
        self.with_ntc(|ntc| {
            let variable_id = draws::fresh_id(ntc)?;
            pool.add(variable_id);
            Ok(variable_id)
        })
    }

    fn pool_generate(
        &self,
        pool: &mut NativeVariables,
        consume: bool,
    ) -> Result<i64, DataSourceError> {
        self.with_ntc(|ntc| {
            let active = pool.active();
            if active.is_empty() {
                return Err(EngineError::AssumeViolation);
            }
            let variable_id = draws::choose_from(ntc, &active)?;
            if consume {
                pool.consume(variable_id);
            }
            Ok(variable_id)
        })
    }

    fn target_observation(&self, score: f64, label: &str) -> Result<(), DataSourceError> {
        if !score.is_finite() {
            return Err(DataSourceError::InvalidArgument(format!(
                "tc.target({score}, label={label:?}) requires a finite score; \
                 got non-finite value"
            )));
        }
        let family = Arc::clone(self.inner.lock().family());
        let mut observations = family.target_observations.lock();
        if observations.contains_key(label) {
            return Err(DataSourceError::InvalidArgument(format!(
                "tc.target({score}, label={label:?}) would overwrite previous \
                 tc.target(_, label={label:?}); each label can be observed at \
                 most once per test case"
            )));
        }
        observations.insert(label.to_string(), score);
        Ok(())
    }

    fn is_nondeterministic(&self) -> bool {
        self.inner.lock().is_nondeterministic()
    }

    fn mark_complete(&self, result: &TestCaseResult) {
        let mut ntc = self.inner.lock();
        let (status, origin) = match result {
            TestCaseResult::Valid => (Status::Valid, None),
            TestCaseResult::Invalid => (Status::Invalid, None),
            TestCaseResult::Overrun => (Status::EarlyStop, None),
            TestCaseResult::Interesting(failure) => (
                Status::Interesting,
                Some(InterestingOrigin(failure.origin.clone())),
            ),
        };
        ntc.conclude(status, origin);
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/data_source_tests.rs"]
mod tests;

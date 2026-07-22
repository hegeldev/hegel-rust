use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::backend::{DataSource, DataSourceError, Failure, TestCaseResult};
use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::core::{
    ChoiceNode, EngineError, InterestingOrigin, ManyState, NativeTestCase, NativeTestCaseHandle,
    Span, SpanEvent, Status,
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
        let handle: NativeTestCaseHandle = Arc::new(std::sync::Mutex::new(ntc));
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
        let mut ntc = handle.lock().unwrap_or_else(|e| e.into_inner());
        ntc.reassemble();
        ntc.nodes.clone()
    }

    /// Convenience: extract spans from a handle after a test case.
    pub fn take_spans(handle: &NativeTestCaseHandle) -> Vec<Span> {
        handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .spans
            .clone()
            .into_vec()
    }

    /// Convenience: extract the live span-open/close events (with their draw
    /// positions) recorded during the test case, so the engine can fold them
    /// into the choice tree for faithful replay.
    pub fn take_span_events(handle: &NativeTestCaseHandle) -> Vec<(usize, SpanEvent)> {
        handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .span_events
            .clone()
    }

    /// Read the `tc.target()` observations the test body recorded.
    ///
    /// Used by the targeting phase in `test_runner` to read back per-label
    /// scores after a test case completes. Returns a clone without mutating
    /// the shared state: the handle may still be shared with a run-owned
    /// [`crate::HegelTestCase`], so reading it must not perturb it.
    pub fn take_target_observations(handle: &NativeTestCaseHandle) -> HashMap<String, f64> {
        handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .family()
            .target_observations
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The test case's outcome, reconstructed from its family's write-once
    /// conclusion. Whoever concluded the family first — a draw that overran
    /// or hit a terminal assume, or the body via `mark_complete` — set the
    /// status, and a later report could not change it.
    ///
    /// Panics only if the family never concluded — i.e. `mark_complete` was
    /// never called on a case that didn't conclude during a draw, which the
    /// cross-backend lifecycle in `run_lifecycle::run_test_case` guarantees
    /// won't occur.
    pub fn take_outcome(handle: &NativeTestCaseHandle) -> TestCaseResult {
        let conclusion = handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .family()
            .conclusion();
        let (status, origin) =
            conclusion.expect("mark_complete must be called for every test case");
        match status {
            Status::Valid => TestCaseResult::Valid,
            Status::Invalid => TestCaseResult::Invalid,
            Status::EarlyStop => TestCaseResult::Overrun,
            Status::Interesting => TestCaseResult::Interesting(Failure {
                origin: origin.map(|o| o.0).unwrap_or_default(),
                reproduce_blob: None,
            }),
        }
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
        let mut ntc = self.inner.lock().unwrap_or_else(|e| e.into_inner());
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
        })
    }

    fn aborted_error(&self) -> DataSourceError {
        let status = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status();
        match status {
            Some(Status::Invalid) => DataSourceError::Assume,
            _ => DataSourceError::StopTest,
        }
    }
}

/// Build the `InvalidArgument` error for a caller-supplied opaque id (a
/// collection / pool / state-machine handle) that libhegel never issued.
/// Returned rather than panicked so the C ABI stays panic-free on bad input
/// (libhegel must remain correct under `panic = "abort"`; an invalid argument
/// is not a bug).
fn unknown_id_error(kind: &str, id: i64) -> EngineError {
    EngineError::InvalidArgument(format!("unknown {kind} id: {id}"))
}

/// Validate a caller-supplied opaque id against the length of the `Vec` it
/// indexes, returning its `usize` index or [`unknown_id_error`]. Rejects both
/// negative ids and ids past the end.
fn checked_id(kind: &str, id: i64, len: usize) -> Result<usize, EngineError> {
    usize::try_from(id)
        .ok()
        .filter(|&idx| idx < len)
        .ok_or_else(|| unknown_id_error(kind, id))
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

    fn generate_ipv4(&self) -> Result<std::net::Ipv4Addr, DataSourceError> {
        self.with_ntc(crate::native::draws::special::generate_ipv4)
    }

    fn generate_ipv6(&self) -> Result<std::net::Ipv6Addr, DataSourceError> {
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

    fn new_collection(&self, min_size: u64, max_size: Option<u64>) -> Result<i64, DataSourceError> {
        self.with_ntc(|ntc| {
            let min_size = usize::try_from(min_size).unwrap_or(usize::MAX);
            let max_size = max_size.map(|n| usize::try_from(n).unwrap_or(usize::MAX));
            let state = ManyState::new(min_size, max_size);
            Ok(ntc.new_collection(state))
        })
    }

    fn collection_more(&self, collection_id: i64) -> Result<bool, DataSourceError> {
        self.with_ntc(|ntc| {
            let family = Arc::clone(ntc.family());
            let mut collections = family.collections.lock().unwrap_or_else(|e| e.into_inner());
            let state = collections
                .get_mut(&collection_id)
                .ok_or_else(|| unknown_id_error("collection", collection_id))?;
            draws::many_more(ntc, state)
        })
    }

    fn collection_reject(
        &self,
        collection_id: i64,
        _why: Option<&str>,
    ) -> Result<(), DataSourceError> {
        self.with_ntc(|ntc| {
            let family = Arc::clone(ntc.family());
            let mut collections = family.collections.lock().unwrap_or_else(|e| e.into_inner());
            let state = collections
                .get_mut(&collection_id)
                .ok_or_else(|| unknown_id_error("collection", collection_id))?;
            draws::many_reject(ntc, state)
        })
    }

    fn new_state_machine(
        &self,
        group_names: Vec<String>,
        rule_names: Vec<String>,
        rule_groups: Vec<i64>,
        invariant_names: Vec<String>,
        concurrency: i64,
    ) -> Result<i64, DataSourceError> {
        if rule_names.is_empty() {
            return Err(DataSourceError::InvalidArgument(
                "cannot run a state machine with no rules".to_string(),
            ));
        }
        if group_names.is_empty() {
            return Err(DataSourceError::InvalidArgument(
                "cannot run a state machine with no concurrency groups".to_string(),
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
        if concurrency < 1 {
            return Err(DataSourceError::InvalidArgument(format!(
                "state machine concurrency must be at least 1, got {concurrency}"
            )));
        }
        let mut groups: Vec<Vec<usize>> = vec![Vec::new(); group_names.len()];
        for (rule, &group) in rule_groups.iter().enumerate() {
            let Some(members) = usize::try_from(group).ok().and_then(|g| groups.get_mut(g)) else {
                return Err(DataSourceError::InvalidArgument(format!(
                    "rule_groups[{rule}] must be in [0, {}), got {group}",
                    group_names.len()
                )));
            };
            members.push(rule);
        }
        if let Some(empty) = groups.iter().position(|members| members.is_empty()) {
            return Err(DataSourceError::InvalidArgument(format!(
                "concurrency group {:?} has no rules",
                group_names[empty]
            )));
        }
        let rule_groups: Vec<usize> = rule_groups.iter().map(|&g| g as usize).collect();
        self.with_ntc(|ntc| {
            let mut machines = ntc
                .family()
                .state_machines
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let id = machines.len() as i64;
            machines.push(Arc::new(std::sync::Mutex::new(
                crate::native::core::NativeStateMachine::new(
                    group_names,
                    rule_names,
                    rule_groups,
                    invariant_names,
                    concurrency,
                ),
            )));
            Ok(id)
        })
    }

    fn state_machine_next_group(
        &self,
        state_machine_id: i64,
    ) -> Result<Option<i64>, DataSourceError> {
        self.with_ntc(|ntc| {
            let machine = {
                let machines = ntc
                    .family()
                    .state_machines
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let idx = checked_id("state machine", state_machine_id, machines.len())?;
                Arc::clone(&machines[idx])
            };
            let mut machine = machine.lock().unwrap_or_else(|e| e.into_inner());
            Ok(machine.next_group(ntc)?.map(|group| group as i64))
        })
    }

    fn state_machine_next_rule(
        &self,
        state_machine_id: i64,
        thread_index: i64,
    ) -> Result<Option<i64>, DataSourceError> {
        self.with_ntc(|ntc| {
            let machine = {
                let machines = ntc
                    .family()
                    .state_machines
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let idx = checked_id("state machine", state_machine_id, machines.len())?;
                Arc::clone(&machines[idx])
            };
            let mut machine = machine.lock().unwrap_or_else(|e| e.into_inner());
            machine.next_rule(ntc, thread_index)
        })
    }

    fn generate_concurrency(&self, max_value: i64) -> Result<i64, DataSourceError> {
        self.with_ntc(|ntc| draws::generate_concurrency(ntc, max_value))
    }

    fn generate_boolean(&self, p: f64, forced: Option<bool>) -> Result<bool, DataSourceError> {
        self.with_ntc(|ntc| draws::generate_boolean(ntc, p, forced))
    }

    fn new_pool(&self) -> Result<i64, DataSourceError> {
        self.with_ntc(|ntc| {
            let mut pools = ntc
                .family()
                .variable_pools
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let pool_id = pools.len() as i64;
            pools.push(crate::native::core::NativeVariables::new());
            Ok(pool_id)
        })
    }

    fn pool_add(&self, pool_id: i64) -> Result<i64, DataSourceError> {
        self.with_ntc(|ntc| {
            let mut pools = ntc
                .family()
                .variable_pools
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let idx = checked_id("variable pool", pool_id, pools.len())?;
            Ok(pools[idx].next())
        })
    }

    fn pool_generate(&self, pool_id: i64, consume: bool) -> Result<i64, DataSourceError> {
        self.with_ntc(|ntc| {
            let family = Arc::clone(ntc.family());
            let mut pools = family
                .variable_pools
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let pool_idx = checked_id("variable pool", pool_id, pools.len())?;
            let active = pools[pool_idx].active();
            if active.is_empty() {
                return Err(EngineError::AssumeViolation);
            }
            let n = active.len();
            let k = ntc
                .draw_integer(BigInt::from(0), BigInt::from(n as i64 - 1))?
                .to_i128()
                .unwrap() as usize;
            let variable_id = active[n - 1 - k];
            if consume {
                pools[pool_idx].consume(variable_id);
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
        let family = Arc::clone(
            self.inner
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .family(),
        );
        let mut observations = family
            .target_observations
            .lock()
            .unwrap_or_else(|e| e.into_inner());
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

    fn mark_complete(&self, result: &TestCaseResult) {
        let mut ntc = self.inner.lock().unwrap_or_else(|e| e.into_inner());
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

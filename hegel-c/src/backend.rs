use ciborium::Value;

/// Error returned by [`DataSource`] methods when an operation cannot complete.
///
/// Not part of the public API: this is an implementation detail of the
/// backend machinery (primarily for libhegel embedding) and may change in any
/// release, including gaining new variants.
#[doc(hidden)]
#[derive(Debug)]
#[non_exhaustive]
pub enum DataSourceError {
    /// The backend ran out of data for this test case.
    StopTest,
    /// The backend rejected the current draw (e.g. a generated float could
    /// not be represented at the requested width).
    Assume,
    /// A caller-supplied argument (typically a schema) was semantically
    /// invalid. The main library converts this to a panic at the API surface;
    /// libhegel maps it to `HEGEL_E_INVALID_ARG` with the message exposed via
    /// `hegel_context_last_error`. Carries a human-readable diagnostic.
    InvalidArgument(String),
}

impl std::fmt::Display for DataSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataSourceError::StopTest => {
                write!(f, "Backend ran out of data for this test case (StopTest)")
            }
            DataSourceError::Assume => write!(f, "Backend rejected the current draw (Assume)"),
            DataSourceError::InvalidArgument(msg) => write!(f, "{}", msg),
        }
    }
}
impl std::error::Error for DataSourceError {}

/// Data source for test case generation.
///
/// Abstracts all communication with a data source (the native engine)
/// behind typed methods. Each fallible method returns `Result<T, DataSourceError>`
/// for operations that can be cut short by data exhaustion or assumption rejection.
///
/// All methods take `&self` — implementations use interior mutability as needed.
/// Implementations must be `Send + Sync` so a `TestCase` clone can be moved to
/// another thread.
pub trait DataSource: Send + Sync {
    /// Send a CBOR schema and receive a generated CBOR value.
    fn generate(&self, schema: &Value) -> Result<Value, DataSourceError>;

    /// Begin a labeled span (used for composite generator structure).
    fn start_span(&self, label: u64) -> Result<(), DataSourceError>;

    /// End the current span. If `discard` is true, the span's choices are discarded.
    fn stop_span(&self, discard: bool) -> Result<(), DataSourceError>;

    /// Create an independent cloned stream of this test case and return a
    /// data source for it.
    ///
    /// The clone occupies one choice position in this stream and then
    /// generates from its own independent choice sequence, so the clone and
    /// every other stream of the family can be driven concurrently from
    /// different threads without perturbing each other, deterministically
    /// under replay. Completion ([`Self::mark_complete`]) remains
    /// family-wide.
    fn clone_stream(&self) -> Result<Box<dyn DataSource + Send + Sync>, DataSourceError>;

    /// Create a new collection. Returns an opaque handle.
    fn new_collection(&self, min_size: u64, max_size: Option<u64>) -> Result<i64, DataSourceError>;

    /// Ask whether the collection should produce another element.
    fn collection_more(&self, collection_id: i64) -> Result<bool, DataSourceError>;

    /// Reject the last element drawn from a collection.
    fn collection_reject(
        &self,
        collection_id: i64,
        why: Option<&str>,
    ) -> Result<(), DataSourceError>;

    /// Register a state machine with the given rule and invariant names for
    /// engine-owned (swarm) rule selection. Returns an opaque state-machine
    /// id. Errors with `InvalidArgument` if `rule_names` is empty.
    fn new_state_machine(
        &self,
        rule_names: &[&str],
        invariant_names: &[&str],
    ) -> Result<i64, DataSourceError>;

    /// Draw the index of the next rule to run, in `[0, num_rules)`.
    fn state_machine_next_rule(&self, state_machine_id: i64) -> Result<i64, DataSourceError>;

    /// Draw a boolean that is `true` with probability `p`.
    ///
    /// If `forced` is `Some`, the choice is still recorded (so replay and
    /// shrinking stay aligned) but the value is forced and no entropy is
    /// consumed.
    fn primitive_boolean(&self, p: f64, forced: Option<bool>) -> Result<bool, DataSourceError>;

    /// Create a new variable pool. Returns an opaque pool id.
    fn new_pool(&self) -> Result<i64, DataSourceError>;

    /// Register a new variable in the pool. Returns the variable id.
    fn pool_add(&self, pool_id: i64) -> Result<i64, DataSourceError>;

    /// Draw a variable id from the pool.
    /// If `consume` is true, the variable is removed from the pool.
    fn pool_generate(&self, pool_id: i64, consume: bool) -> Result<i64, DataSourceError>;

    /// Record a targeting observation for the current test case.
    ///
    /// The score is used by the backend to guide generation toward
    /// higher-scoring inputs. Errors with `InvalidArgument` if the score is
    /// non-finite or the label has already been observed this test case.
    fn target_observation(&self, score: f64, label: &str) -> Result<(), DataSourceError>;

    /// Signal that the test case is complete and report its outcome.
    ///
    /// Called exactly once per test case, after the test body has finished
    /// (or panicked) and the lifecycle has translated the panic payload into
    /// a [`TestCaseResult`].  The implementation does whatever bookkeeping
    /// its engine needs here — e.g. stashing the outcome on a handle for the
    /// engine to consume.
    fn mark_complete(&self, result: &TestCaseResult);
}

/// A single interesting test case surfaced by a run.
///
/// A failure carries the origin the engine grouped on and the reproduce blob
/// the client replays; the rendered diagnostic (panic location, message,
/// backtrace) is produced when the client replays that blob.
#[derive(Debug, Clone)]
pub struct Failure {
    /// Opaque per-bug origin tag — currently `"Panic at file:line:col"` from
    /// the captured panic site (with `<unknown>` for the location when
    /// `take_panic_info` returns nothing).  Passed through
    /// `DataSource::mark_complete` so the engine can group test cases by
    /// which bug they trigger and shrink each origin to its own minimal
    /// counterexample.
    pub origin: String,
    /// A base64 "failure blob" encoding the minimal counterexample's choice
    /// sequence. `Some` for an interesting counterexample surfaced by a full
    /// run (the shrunk choices are available); `None` for a single-test-case
    /// run, which has no shrunk choice sequence to encode. The client replays
    /// it via `hegel_test_case_from_blob`; paste into
    /// `#[hegel::reproduce_failure("…")]` to replay it by hand.
    pub reproduce_blob: Option<String>,
}

/// Result of running a single test case.
#[derive(Debug, Clone)]
pub enum TestCaseResult {
    /// Test case passed normally.
    Valid,
    /// Test case was rejected because an assumption failed.
    Invalid,
    /// Test case was rejected because the backend ran out of data.
    Overrun,
    /// Test case found a bug.
    Interesting(Failure),
}

/// A failure of the run itself, as opposed to a failure of a test case:
/// the run could not produce a trustworthy verdict on the property.
///
/// These are returned as `Err` from the engine's exploration and surface at
/// the API boundary — the panic API panics with the message; libhegel reports
/// it through its error channel.
#[derive(Debug, Clone)]
pub enum RunError {
    /// A failed health check (FilterTooMuch, TooSlow, TestCasesTooLarge,
    /// LargeInitialTestCase).
    HealthCheck(String),
    /// The test produced different outcomes when run on identical data.
    Flaky(String),
    /// Data generation diverged between runs of the same choice sequence.
    NonDeterministic(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::HealthCheck(msg) | RunError::Flaky(msg) | RunError::NonDeterministic(msg) => {
                write!(f, "{}", msg)
            }
        }
    }
}

impl std::error::Error for RunError {}

/// Result of a full test run: the run's outcome once generation and
/// shrinking are done.
///
/// The engine only *explores*, so each [`Failure`] carries the origin the
/// engine grouped on and the reproduce blob the client replays. The client
/// (`run_lifecycle::drive` for the panic API) replays each blob itself and
/// owns the resulting report. The run passed iff `failures` is empty.
#[derive(Debug)]
pub struct TestRunResult {
    /// One entry per distinct interesting example surfaced by the run, one
    /// per distinct bug origin, in report order. Empty for a passing run.
    pub failures: Vec<Failure>,
}

#[cfg(test)]
#[path = "../tests/embedded/backend_tests.rs"]
mod tests;

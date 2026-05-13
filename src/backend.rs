use crate::Settings;
use ciborium::Value;

/// Error returned by [`DataSource`] methods when an operation cannot complete.
#[derive(Debug)]
pub enum DataSourceError {
    /// The backend ran out of data for this test case.
    StopTest,
    /// The backend rejected the current draw (e.g. a generated float could
    /// not be represented at the requested width).
    Assume,
    /// The backend returned an error (e.g. invalid arguments, internal error).
    ServerError(String),
}

impl std::fmt::Display for DataSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataSourceError::StopTest => {
                write!(f, "Backend ran out of data for this test case (StopTest)")
            }
            DataSourceError::Assume => write!(f, "Backend rejected the current draw (Assume)"),
            DataSourceError::ServerError(msg) => write!(f, "{}", msg),
        }
    }
}
impl std::error::Error for DataSourceError {}

/// Data source for test case generation.
///
/// Abstracts all communication with a data source (e.g. the hegel-core server)
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

    /// Create a new variable pool. Returns an opaque pool id.
    fn new_pool(&self) -> Result<i128, DataSourceError>;

    /// Register a new variable in the pool. Returns the variable id.
    fn pool_add(&self, pool_id: i128) -> Result<i128, DataSourceError>;

    /// Draw a variable id from the pool.
    /// If `consume` is true, the variable is removed from the pool.
    fn pool_generate(&self, pool_id: i128, consume: bool) -> Result<i128, DataSourceError>;

    /// Record a targeting observation for the current test case.
    ///
    /// The score is used by the backend to guide generation toward
    /// higher-scoring inputs. No-op if the test has been aborted.
    fn target_observation(&self, score: f64, label: &str);

    /// Signal that the test case is complete and report its outcome.
    ///
    /// Called exactly once per test case, after the test body has finished
    /// (or panicked) and the lifecycle has translated the panic payload into
    /// a [`TestCaseResult`].  Backends are expected to do whatever bookkeeping
    /// their engine needs here — forward the outcome to a remote server,
    /// stash it on a handle for the local engine to consume, etc.
    fn mark_complete(&self, result: &TestCaseResult);
}

/// A single failing test case discovered by a [`TestRunner`].
#[derive(Debug, Clone)]
pub struct Failure {
    /// The raw panic message from the failing test (the string passed to `panic!`).
    /// Used as-is for the legacy single-failure outer panic message.
    pub panic_message: String,
    /// Pre-rendered multi-line diagnostic — `thread '...' panicked at file:line:`
    /// followed by the panic message and (when captured) the stack backtrace.
    /// Same format that was previously printed inline by the runner on final replay.
    pub diagnostic: String,
    /// Opaque per-bug origin tag — currently `"Panic at file:line:col"` from
    /// the captured panic site (with `<unknown>` for the location when
    /// `take_panic_info` returns nothing).  Passed through
    /// `DataSource::mark_complete` so Hypothesis can group test cases by
    /// which bug they trigger and shrink each origin to its own minimal
    /// counterexample.
    pub origin: String,
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

/// Result of a full test run.
#[derive(Debug)]
pub struct TestRunResult {
    /// Whether all test cases passed.
    pub passed: bool,
    /// One entry per distinct failing example surfaced by the run.  Empty when
    /// `passed` is `true`.  For the multi-bug case (Hypothesis emits one final
    /// replay per `BaseExceptionGroup` origin), each origin contributes one
    /// entry, ordered as the backend replayed them.
    pub failures: Vec<Failure>,
}

/// Drives the test execution lifecycle.
///
/// Implementations control how test cases are generated, how data sources
/// are created for each test case, and how shrinking/replay works.
/// This trait has no reference to any external process — it can be
/// implemented purely in memory.
pub trait TestRunner {
    /// Execute a test run.
    ///
    /// `run_case` is called for each test case with:
    /// - A data source for generating test data
    /// - A bool indicating whether this is the final replay of a minimal failing example
    ///
    /// The callback runs the test body to completion; the per-test-case
    /// outcome is delivered to the backend through
    /// [`DataSource::mark_complete`] rather than as a return value, so both
    /// backends consume the result through the same interface.  Backends
    /// arrange to read it back (e.g. via a per-test-case handle to a shared
    /// outcome cell on the data source).
    fn run(
        &self,
        settings: &Settings,
        database_key: Option<&str>,
        run_case: &mut dyn FnMut(Box<dyn DataSource>, bool),
    ) -> TestRunResult;
}

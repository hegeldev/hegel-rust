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

    /// Signal that the test case is complete.
    fn mark_complete(&self, status: &str, origin: Option<&str>);

    /// Returns true if a previous request triggered an abort (overflow/StopTest).
    fn test_aborted(&self) -> bool;
}

/// Result of running a single test case.
#[derive(Debug)]
pub enum TestCaseResult {
    /// Test case passed normally.
    Valid,
    /// Test case was rejected because an assumption failed.
    Invalid,
    /// Test case was rejected because the backend ran out of data.
    Overrun,
    /// Test case found a bug.
    Interesting {
        /// The panic message from the failing test.
        panic_message: String,
    },
}

/// Result of a full test run.
#[derive(Debug)]
pub struct TestRunResult {
    /// Whether all test cases passed.
    pub passed: bool,
    /// If a test case failed, the message from the minimal failing example.
    pub failure_message: Option<String>,
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
    /// The callback returns the result of running the test case.
    fn run(
        &self,
        settings: &Settings,
        database_key: Option<&str>,
        run_case: &mut dyn FnMut(Box<dyn DataSource>, bool) -> TestCaseResult,
    ) -> TestRunResult;
}

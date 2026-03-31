use crate::Settings;
use ciborium::Value;

/// Error indicating the backend ran out of data for this test case.
#[derive(Debug)]
pub struct StopTestError;
impl std::fmt::Display for StopTestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Backend ran out of data for this test case (StopTest)")
    }
}
impl std::error::Error for StopTestError {}

/// Backend for test case data generation.
///
/// Abstracts all communication with a data source (e.g. the hegel-core server)
/// behind typed methods. Each fallible method returns `Result<T, StopTestError>`
/// for operations that can be cut short by data exhaustion.
///
/// All methods take `&self` — implementations use interior mutability as needed.
pub trait Backend {
    /// Send a CBOR schema and receive a generated CBOR value.
    fn generate(&self, schema: &Value) -> Result<Value, StopTestError>;

    /// Begin a labeled span (used for composite generator structure).
    fn start_span(&self, label: u64) -> Result<(), StopTestError>;

    /// End the current span. If `discard` is true, the span's choices are discarded.
    fn stop_span(&self, discard: bool) -> Result<(), StopTestError>;

    /// Create a new collection. Returns an opaque handle.
    fn new_collection(
        &self,
        name: &str,
        min_size: u64,
        max_size: Option<u64>,
    ) -> Result<String, StopTestError>;

    /// Ask whether the collection should produce another element.
    fn collection_more(&self, collection: &str) -> Result<bool, StopTestError>;

    /// Reject the last element drawn from a collection.
    fn collection_reject(&self, collection: &str, why: Option<&str>) -> Result<(), StopTestError>;

    /// Create a new variable pool. Returns an opaque pool id.
    fn new_pool(&self) -> Result<i128, StopTestError>;

    /// Register a new variable in the pool. Returns the variable id.
    fn pool_add(&self, pool_id: i128) -> Result<i128, StopTestError>;

    /// Draw a variable id from the pool.
    /// If `consume` is true, the variable is removed from the pool.
    fn pool_generate(&self, pool_id: i128, consume: bool) -> Result<i128, StopTestError>;

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
    /// Test case was rejected (assumption failed or data exhaustion).
    Invalid,
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
/// Implementations control how test cases are generated, how backends
/// are created for each test case, and how shrinking/replay works.
/// This trait has no reference to any external process — it can be
/// implemented purely in memory.
pub trait TestRunner {
    /// Execute a test run.
    ///
    /// `run_case` is called for each test case with:
    /// - A backend for generating test data
    /// - A bool indicating whether this is the final replay of a minimal failing example
    ///
    /// The callback returns the result of running the test case.
    fn run(
        &self,
        settings: &Settings,
        database_key: Option<&str>,
        run_case: &mut dyn FnMut(Box<dyn Backend>, bool) -> TestCaseResult,
    ) -> TestRunResult;
}

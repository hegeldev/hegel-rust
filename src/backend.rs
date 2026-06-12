use crate::Settings;
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
    /// `hegel_last_error_message`. Carries a human-readable diagnostic.
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
    fn new_pool(&self) -> Result<i64, DataSourceError>;

    /// Register a new variable in the pool. Returns the variable id.
    fn pool_add(&self, pool_id: i64) -> Result<i64, DataSourceError>;

    /// Draw a variable id from the pool.
    /// If `consume` is true, the variable is removed from the pool.
    fn pool_generate(&self, pool_id: i64, consume: bool) -> Result<i64, DataSourceError>;

    /// Record a targeting observation for the current test case.
    ///
    /// The score is used by the backend to guide generation toward
    /// higher-scoring inputs. No-op if the test has been aborted.
    fn target_observation(&self, score: f64, label: &str);

    /// Signal that the test case is complete and report its outcome.
    ///
    /// Called exactly once per test case, after the test body has finished
    /// (or panicked) and the lifecycle has translated the panic payload into
    /// a [`TestCaseResult`].  The implementation does whatever bookkeeping
    /// its engine needs here — e.g. stashing the outcome on a handle for the
    /// engine to consume.
    fn mark_complete(&self, result: &TestCaseResult);
}

/// A single failing test case discovered by a [`TestRunner`].
///
/// This is data about the failure, not its presentation: the rendered
/// diagnostic block (panic location, message, backtrace) is printed by the
/// lifecycle at the moment the panic is caught and never travels with the
/// failure.
#[derive(Debug, Clone)]
pub struct Failure {
    /// The raw panic message from the failing test (the string passed to `panic!`).
    /// Used as-is for the legacy single-failure outer panic message.
    pub panic_message: String,
    /// Opaque per-bug origin tag — currently `"Panic at file:line:col"` from
    /// the captured panic site (with `<unknown>` for the location when
    /// `take_panic_info` returns nothing).  Passed through
    /// `DataSource::mark_complete` so Hypothesis can group test cases by
    /// which bug they trigger and shrink each origin to its own minimal
    /// counterexample.
    pub origin: String,
    /// A base64 "failure blob" encoding the minimal counterexample's choice sequence.
    /// `Some` only on the native backend's final replay, where the shrunk choices
    /// are available; `None` everywhere else. Paste into `#[hegel::reproduce_failure("…")]`
    /// or feed to [`crate::Hegel::reproduce_failure`] to replay it.
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
/// These are returned as `Err` from [`TestRunner::explore`] and
/// [`TestRunner::replay_final`] and surface at the API boundary — the panic
/// API panics with the message; libhegel reports it through its error
/// channel. They never appear as [`Failure`]s: there is no counterexample,
/// reproduce blob, or diagnostic block, just a message about the run.
#[derive(Debug, Clone)]
pub enum RunError {
    /// A failed health check (FilterTooMuch, TooSlow, TestCasesTooLarge,
    /// LargeInitialTestCase).
    HealthCheck(String),
    /// The test produced different outcomes when run on identical data.
    Flaky(String),
    /// Data generation diverged between runs of the same choice sequence.
    NonDeterministic(String),
    /// `reproduce_failure`: the blob decoded but no longer fails.
    StaleBlob(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::HealthCheck(msg)
            | RunError::Flaky(msg)
            | RunError::NonDeterministic(msg)
            | RunError::StaleBlob(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for RunError {}

/// The outcome of a [`TestRunner::explore`] pass: the run's result once
/// generation and shrinking are done, before any final replay.
///
/// The caller — `run_lifecycle::drive` for the panic API,
/// [`crate::embed::run_native`] for FFI — replays each counterexample via
/// [`TestRunner::replay_final`] and owns the resulting report.
#[derive(Debug)]
pub enum Exploration<C> {
    /// The run found no failures.
    Passed,
    /// The run failed with a failure that has no counterexample left to
    /// replay — `Mode::SingleTestCase`, whose one test case is its own
    /// final replay.
    Failed(Failure),
    /// The run discovered counterexamples, one per distinct bug, in report
    /// order. Each still needs its final replay via
    /// [`TestRunner::replay_final`].
    Counterexamples(Vec<C>),
}

/// Result of a full test run: the aggregate, post-replay view.
///
/// [`crate::embed::run_native`] folds an [`Exploration`] plus its final
/// replays into one of these so libhegel can inspect the run as a whole.
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

/// Drives test exploration and counterexample replay.
///
/// Implementations control how test cases are generated, how data sources
/// are created for each test case, and how shrinking works.
/// This trait has no reference to any external process — it can be
/// implemented purely in memory.
///
/// In both methods, `run_case` is called once per test case with a data
/// source for generating test data and a bool indicating whether this is
/// the final replay of a minimal failing example. It runs the test body to
/// completion; the outcome is delivered back through
/// [`DataSource::mark_complete`], not as a return value.
pub trait TestRunner {
    /// A minimal counterexample discovered by [`explore`](Self::explore),
    /// replayable via [`replay_final`](Self::replay_final).
    type Counterexample;

    /// Run the exploration half of a test run: database replay, generation,
    /// and shrinking, stopping at the point where the run's outcome — and
    /// every distinct bug's minimal counterexample — is known. `Err` means
    /// the run itself failed (health check, nondeterminism) before reaching
    /// a verdict.
    fn explore(
        &self,
        settings: &Settings,
        database_key: Option<&str>,
        run_case: &mut dyn FnMut(Box<dyn DataSource + Send + Sync>, bool),
    ) -> Result<Exploration<Self::Counterexample>, RunError>;

    /// Replay one counterexample with `is_final = true` and return the
    /// [`Failure`] the test body reported (with its reproduce blob
    /// attached). `Err` means the counterexample stopped failing between
    /// discovery and replay — flaky for the native engine, a stale blob for
    /// a blob replay.
    fn replay_final(
        &self,
        counterexample: Self::Counterexample,
        run_case: &mut dyn FnMut(Box<dyn DataSource + Send + Sync>, bool),
    ) -> Result<Failure, RunError>;
}

/// Fold an [`Exploration`] plus its final replays into the aggregate
/// [`TestRunResult`] shape. Counterexamples are replayed in order; a replay
/// that no longer fails errors the whole run.
pub(crate) fn collect_failures<R: TestRunner>(
    runner: &R,
    exploration: Exploration<R::Counterexample>,
    run_case: &mut dyn FnMut(Box<dyn DataSource + Send + Sync>, bool),
) -> Result<TestRunResult, RunError> {
    match exploration {
        Exploration::Passed => Ok(TestRunResult {
            passed: true,
            failures: Vec::new(),
        }),
        Exploration::Failed(failure) => Ok(TestRunResult {
            passed: false,
            failures: vec![failure],
        }),
        Exploration::Counterexamples(counterexamples) => {
            let mut failures = Vec::new();
            for counterexample in counterexamples {
                failures.push(runner.replay_final(counterexample, run_case)?);
            }
            Ok(TestRunResult {
                passed: failures.is_empty(),
                failures,
            })
        }
    }
}

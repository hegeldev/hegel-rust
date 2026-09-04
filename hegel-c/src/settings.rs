use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// Health checks that can be suppressed during test execution.
///
/// Health checks detect common issues with test configuration that would
/// otherwise cause tests to run inefficiently or not at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum HealthCheck {
    /// Too many test cases are being filtered out via `assume()`.
    FilterTooMuch,
    /// Test execution is too slow.
    TooSlow,
    /// Generated test cases are too large.
    TestCasesTooLarge,
    /// The smallest natural input is very large.
    LargeInitialTestCase,
}

/// Controls which phases of the test lifecycle are executed.
///
/// By default, all phases run. Use [`Settings::phases`] to restrict which
/// phases execute — for example, passing only `[Phase::Generate]` disables
/// shrinking, which is useful when you only need to find a counterexample
/// quickly and don't need the minimal one.
///
/// Corresponds to a subset of `hypothesis.Phase` (the `explain` phase is not
/// yet supported in hegel-rust).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Phase {
    /// Run explicit test cases added via `#[hegel::explicit_test_case]`.
    Explicit,
    /// Replay examples from the failure database.
    Reuse,
    /// Generate new random examples.
    Generate,
    /// Use targeting to guide generation toward interesting areas.
    Target,
    /// Shrink failing examples to a minimal counterexample.
    Shrink,
}

/// Selects the source of randomness the engine draws from.
///
/// Mirrors Hypothesis's `backend` setting (specifically `backend="hypothesis"`
/// vs `backend="hypothesis-urandom"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Backend {
    /// The default: generate from a seeded pseudo-random generator. Runs are
    /// reproducible from [`Settings::seed`] and shrinking/replay work as usual.
    Default,
    /// Read fresh entropy from `/dev/urandom` on every draw, instead of
    /// expanding a single PRNG seed.
    ///
    /// This exists for running under [Antithesis](https://antithesis.com/),
    /// whose fuzzer controls the bytes returned by `/dev/urandom`. Sourcing
    /// every choice from the OS random device hands the fuzzer control over
    /// the entire test case (rather than just the PRNG seed), so it can steer
    /// and reproduce generation directly. When running inside Antithesis this
    /// backend is selected automatically unless you set one explicitly.
    ///
    /// The generation algorithm is otherwise unchanged — only the random
    /// source differs. On platforms without `/dev/urandom` (Windows) it falls
    /// back to an OS-seeded PRNG. You almost certainly don't want this backend
    /// unless you are running under Antithesis.
    Urandom,
}

/// Where engine-emitted output (verbose / debug progress traces, warnings)
/// is written.
///
/// The default is stderr. [`Output::callback`] redirects every line to a
/// caller-supplied sink instead — this is what backs the output callback the
/// C ABI's `hegel_run_start` / `hegel_test_case_from_blob` accept, letting
/// embeddings (e.g. a Go `testing.T`) capture engine output in-process. Lines
/// are delivered without a trailing newline. The engine emits from its worker
/// thread, so the sink must be `Send + Sync`.
#[derive(Clone)]
pub struct Output {
    sink: Option<OutputSink>,
}

/// A caller-supplied destination for engine output lines.
type OutputSink = alloc::sync::Arc<dyn Fn(&str) + Send + Sync>;

impl Output {
    /// The default destination: each line is written to stderr.
    pub fn stderr() -> Self {
        Output { sink: None }
    }

    /// Deliver each line to `sink` instead of stderr.
    pub fn callback(sink: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Output {
            sink: Some(alloc::sync::Arc::new(sink)),
        }
    }

    /// Emit one line of output to this destination.
    pub(crate) fn line(&self, line: &str) {
        match &self.sink {
            Some(sink) => sink(line),
            None => crate::sys::stderr_line(line),
        }
    }
}

impl core::fmt::Debug for Output {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.sink {
            Some(_) => f.write_str("Output(callback)"),
            None => f.write_str("Output(stderr)"),
        }
    }
}

/// Controls how much output Hegel produces during test runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verbosity {
    /// Suppress all output.
    Quiet,
    /// Default output level.
    Normal,
    /// Show more detail about the test run.
    Verbose,
    /// Show protocol-level debug information.
    Debug,
}

/// Configuration for a Hegel test run.
///
/// Use builder methods to customize, then pass to [`Hegel::settings`] or
/// the `settings` parameter of `#[hegel::test]`.
///
/// In CI environments (detected automatically), the database is disabled
/// and tests are derandomized by default. Inside Antithesis (detected via
/// `ANTITHESIS_OUTPUT_DIR`), the database and all health checks are disabled
/// by default: Antithesis owns reproduction, and its thread pausing makes
/// wall-clock health checks like `TooSlow` meaningless.
#[derive(Debug, Clone)]
pub struct Settings {
    pub(crate) test_cases: u64,
    pub(crate) stateful_step_count: i64,
    pub(crate) verbosity: Verbosity,
    pub(crate) output: Output,
    pub(crate) seed: Option<u64>,
    pub(crate) derandomize: bool,
    pub(crate) database: Database,
    pub(crate) suppress_health_check: Vec<HealthCheck>,
    /// Set when running inside Antithesis. Disables every health check
    /// regardless of `suppress_health_check`, which frontends may overwrite
    /// with their own (typically empty) list; see
    /// [`Settings::health_check_suppressed`].
    pub(crate) in_antithesis: bool,
    pub(crate) phases: Vec<Phase>,
    pub(crate) report_multiple_failures: bool,
    /// Print event statistics (`tc.event()` / `tc.event_value()`
    /// observations from the generation phase) at the end of the run.
    pub(crate) show_statistics: bool,
    /// The randomness backend, or `None` to let it be chosen automatically
    /// (urandom under Antithesis, the default PRNG otherwise). An explicit
    /// [`Settings::backend`] always wins over the automatic choice.
    pub(crate) backend: Option<Backend>,
}

impl Settings {
    /// Create settings with defaults. Detects CI and Antithesis
    /// environments automatically.
    pub fn new() -> Self {
        Self::for_env(
            is_in_ci(),
            crate::antithesis_detect::antithesis_env_var_set(),
        )
    }

    fn for_env(in_ci: bool, in_antithesis: bool) -> Self {
        Self {
            test_cases: 100,
            stateful_step_count: 50,
            verbosity: Verbosity::Normal,
            output: Output::stderr(),
            seed: None,
            derandomize: in_ci,
            database: if in_ci || in_antithesis {
                Database::Disabled
            } else {
                Database::Unset
            },
            suppress_health_check: Vec::new(),
            in_antithesis,
            phases: vec![
                Phase::Explicit,
                Phase::Reuse,
                Phase::Generate,
                Phase::Target,
                Phase::Shrink,
            ],
            report_multiple_failures: true,
            show_statistics: false,
            backend: None,
        }
    }

    /// Select the randomness backend.
    ///
    /// By default the backend is chosen automatically: [`Backend::Urandom`]
    /// when running inside Antithesis, and [`Backend::Default`] otherwise.
    /// Calling this pins the choice, overriding the automatic detection.
    pub fn backend(mut self, backend: Backend) -> Self {
        self.backend = Some(backend);
        self
    }

    /// Whether `check` should be skipped: either the user suppressed it
    /// explicitly, or the run is inside Antithesis, where every health check
    /// is off by default.
    pub(crate) fn health_check_suppressed(&self, check: HealthCheck) -> bool {
        self.in_antithesis || self.suppress_health_check.contains(&check)
    }

    /// Resolve the effective backend, given whether the process is running
    /// inside Antithesis.
    ///
    /// An explicit [`Settings::backend`] always wins; otherwise urandom is
    /// used under Antithesis and the default PRNG backend elsewhere.
    pub(crate) fn resolved_backend(&self, in_antithesis: bool) -> Backend {
        match self.backend {
            Some(backend) => backend,
            None if in_antithesis => Backend::Urandom,
            None => Backend::Default,
        }
    }

    /// Set the number of test cases to run (default: 100).
    pub fn test_cases(mut self, n: u64) -> Self {
        self.test_cases = n;
        self
    }

    /// Set the target number of steps run per stateful test case (default:
    /// 50). Each case runs at least one step and at most this many.
    pub fn stateful_step_count(mut self, n: i64) -> Self {
        self.stateful_step_count = n;
        self
    }

    /// Set the verbosity level.
    pub fn verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = verbosity;
        self
    }

    /// Set where engine-emitted output is written. Defaults to
    /// [`Output::stderr`].
    pub fn output(mut self, output: Output) -> Self {
        self.output = output;
        self
    }

    /// Set a fixed seed for reproducibility, or `None` for random.
    pub fn seed(mut self, seed: Option<u64>) -> Self {
        self.seed = seed;
        self
    }

    /// When true, use a fixed seed derived from the test name. Enabled by default in CI.
    pub fn derandomize(mut self, derandomize: bool) -> Self {
        self.derandomize = derandomize;
        self
    }

    /// Set the database path for storing failing examples, or `None` to disable.
    pub fn database(mut self, database: Option<String>) -> Self {
        self.database = match database {
            None => Database::Disabled,
            Some(path) => Database::Path(path),
        };
        self
    }

    /// Set which test lifecycle phases to run.
    ///
    /// Defaults to all phases: `[Phase::Explicit, Phase::Reuse, Phase::Generate, Phase::Target, Phase::Shrink]`.
    ///
    /// Example — skip shrinking (useful when you only need a witness, not a
    /// minimal counterexample):
    ///
    /// ```ignore
    /// use hegel::{Phase, Settings};
    ///
    /// let s = Settings::new().phases([Phase::Reuse, Phase::Generate]);
    /// ```
    pub fn phases(mut self, phases: impl IntoIterator<Item = Phase>) -> Self {
        self.phases = phases.into_iter().collect();
        self
    }

    /// Suppress one or more health checks so they do not cause test failure.
    ///
    /// Health checks detect common issues like excessive filtering or slow
    /// tests. Use this to suppress specific checks when they are expected.
    /// Replaces any previously configured suppressions, like [`Settings::phases`].
    ///
    /// # Example
    ///
    /// ```ignore
    /// use hegel::{HealthCheck, Verbosity};
    /// use hegel::generators as gs;
    ///
    /// #[hegel::test(suppress_health_check = [HealthCheck::FilterTooMuch, HealthCheck::TooSlow])]
    /// fn my_test(tc: hegel::TestCase) {
    ///     let n: i32 = tc.draw(gs::integers());
    ///     tc.assume(n > 0);
    /// }
    /// ```
    pub fn suppress_health_check(mut self, checks: impl IntoIterator<Item = HealthCheck>) -> Self {
        self.suppress_health_check = checks.into_iter().collect();
        self
    }

    /// Control whether multi-bug runs report every distinct failing example
    /// or collapse to just the first one.
    ///
    /// When `true` (the default), each distinct origin Hegel finds is surfaced
    /// as its own diagnostic, and the final panic message reports the count of
    /// distinct failures.  Setting this to `false` makes Hegel collapse a
    /// multi-bug run to one example — useful when you have a flaky predicate
    /// that triggers several superficially-distinct failures whose root cause
    /// is the same, and the extra reports are just noise.
    ///
    /// Maps to Hypothesis's `report_multiple_bugs` setting.
    pub fn report_multiple_failures(mut self, report_multiple_failures: bool) -> Self {
        self.report_multiple_failures = report_multiple_failures;
        self
    }

    /// Print event statistics (`tc.event()` / `tc.event_value()`
    /// observations from the generation phase) on the run's output at the
    /// end of the run. Defaults to off.
    pub fn show_statistics(mut self, show_statistics: bool) -> Self {
        self.show_statistics = show_statistics;
        self
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Database {
    Unset,
    Disabled,
    Path(String),
}

fn is_in_ci() -> bool {
    is_in_ci_from(crate::sys::env_var)
}

fn is_in_ci_from(env: impl Fn(&str) -> Option<String>) -> bool {
    const CI_VARS: &[(&str, Option<&str>)] = &[
        ("CI", None),
        ("TF_BUILD", Some("true")),
        ("BUILDKITE", Some("true")),
        ("CIRCLECI", Some("true")),
        ("CIRRUS_CI", Some("true")),
        ("CODEBUILD_BUILD_ID", None),
        ("GITHUB_ACTIONS", Some("true")),
        ("GITLAB_CI", None),
        ("HEROKU_TEST_RUN_ID", None),
        ("TEAMCITY_VERSION", None),
        ("bamboo.buildKey", None),
    ];

    CI_VARS.iter().any(|(key, value)| match value {
        None => env(key).is_some(),
        Some(expected) => env(key).as_deref() == Some(expected),
    })
}

#[cfg(test)]
#[path = "../tests/embedded/settings_tests.rs"]
mod tests;

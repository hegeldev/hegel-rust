use crate::antithesis::TestLocation;
use crate::test_case::{TestCase, invalid_argument};

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

impl HealthCheck {
    /// Returns all health check variants.
    ///
    /// Useful for suppressing all health checks at once:
    ///
    /// ```no_run
    /// use hegel::HealthCheck;
    ///
    /// #[hegel::test(suppress_health_check = HealthCheck::all())]
    /// fn my_test(tc: hegel::TestCase) {
    ///     // ...
    /// }
    /// ```
    pub const fn all() -> [HealthCheck; 4] {
        [
            HealthCheck::FilterTooMuch,
            HealthCheck::TooSlow,
            HealthCheck::TestCasesTooLarge,
            HealthCheck::LargeInitialTestCase,
        ]
    }
}

/// Controls which phases of the test lifecycle are executed.
///
/// By default, all phases run. Use [`Settings::phases`] to restrict which
/// phases execute — for example, passing only `[Phase::Generate]` disables
/// shrinking, which is useful when you only need to find a counterexample
/// quickly and don't need the minimal one.
///
/// Corresponds to `hypothesis.Phase`.
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
    /// Attempt to explain why the test failed: after shrinking, the engine
    /// varies each part of the minimal counterexample, and parts whose value
    /// turns out to be irrelevant are annotated with
    /// `// or any other generated value` in the reported failing example.
    /// Requires [`Phase::Shrink`].
    Explain,
}

/// Controls the test execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Mode {
    /// Run a full test (multiple test cases with shrinking). This is the default.
    TestRun,
    /// Run a single test case with no shrinking or replay. Useful for
    /// Antithesis workloads and other contexts where you want pure data
    /// generation without property-testing overhead.
    SingleTestCase,
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
/// and tests are derandomized by default.
#[derive(Debug, Clone)]
pub struct Settings {
    pub(crate) mode: Mode,
    pub(crate) test_cases: u64,
    pub(crate) verbosity: Verbosity,
    pub(crate) seed: Option<u64>,
    pub(crate) derandomize: bool,
    pub(crate) database: Database,
    pub(crate) suppress_health_check: Vec<HealthCheck>,
    pub(crate) phases: Vec<Phase>,
    pub(crate) report_multiple_failures: bool,
    pub(crate) print_blob: bool,
    /// The randomness backend, or `None` to let it be chosen automatically
    /// (urandom under Antithesis, the default PRNG otherwise). An explicit
    /// [`Settings::backend`] always wins over the automatic choice.
    pub(crate) backend: Option<Backend>,
}

impl Settings {
    /// Create settings with defaults. Detects CI environments automatically.
    pub fn new() -> Self {
        let in_ci = is_in_ci();
        Self {
            mode: Mode::TestRun,
            test_cases: 100,
            verbosity: Verbosity::Normal,
            seed: None,
            derandomize: in_ci,
            database: if in_ci {
                Database::Disabled
            } else {
                Database::Unset // nocov
            },
            suppress_health_check: Vec::new(),
            phases: vec![
                Phase::Explicit,
                Phase::Reuse,
                Phase::Generate,
                Phase::Target,
                Phase::Shrink,
                Phase::Explain,
            ],
            report_multiple_failures: false,
            print_blob: false,
            backend: None,
        }
    }

    /// Set the execution mode. Defaults to [`Mode::TestRun`].
    pub fn mode(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
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

    /// Set the number of test cases to run (default: 100).
    pub fn test_cases(mut self, n: u64) -> Self {
        self.test_cases = n;
        self
    }

    /// Set the verbosity level.
    pub fn verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = verbosity;
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
    /// Defaults to all phases: `[Phase::Explicit, Phase::Reuse, Phase::Generate, Phase::Target, Phase::Shrink, Phase::Explain]`.
    ///
    /// [`Phase::Explain`] refines the shrunk counterexample, so including it
    /// without [`Phase::Shrink`] is an error.
    ///
    /// Example — skip shrinking (useful when you only need a witness, not a
    /// minimal counterexample):
    ///
    /// ```no_run
    /// use hegel::{Phase, Settings};
    ///
    /// let s = Settings::new().phases([Phase::Reuse, Phase::Generate]);
    /// ```
    pub fn phases(mut self, phases: impl IntoIterator<Item = Phase>) -> Self {
        self.phases = phases.into_iter().collect();
        if self.phases.contains(&Phase::Explain) && !self.phases.contains(&Phase::Shrink) {
            invalid_argument!("Phase::Explain requires Phase::Shrink");
        }
        self
    }

    /// Print a copy-pasteable `#[hegel::reproduce_failure("…")]` line for the
    /// counterexample when a test fails. Defaults to `false`.
    ///
    /// The reproduce blob is always *attached* to the failure. This setting only controls whether it is printed to
    /// the failure output. Has effect only on the native backend.
    pub fn print_blob(mut self, print_blob: bool) -> Self {
        self.print_blob = print_blob;
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
    /// ```no_run
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

    /// Returns `true` if the given phase is enabled in these settings.
    pub fn has_phase(&self, phase: Phase) -> bool {
        self.phases.contains(&phase)
    }

    /// Control whether multi-bug runs report every distinct failing example
    /// or collapse to just the first one.
    ///
    /// When `true`, each distinct origin Hegel finds is surfaced as its own
    /// diagnostic, and the final panic message reports the count of distinct
    /// failures.  When `false` (the default), Hegel collapses a multi-bug run
    /// to one example — several superficially-distinct failures often share a
    /// root cause, and the extra reports are just noise.
    ///
    /// Maps to Hypothesis's `report_multiple_bugs` setting.
    pub fn report_multiple_failures(mut self, report_multiple_failures: bool) -> Self {
        self.report_multiple_failures = report_multiple_failures;
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

#[doc(hidden)]
pub fn hegel<F>(test_fn: F)
where
    F: FnMut(TestCase),
{
    Hegel::new(test_fn).run();
}

fn is_in_ci() -> bool {
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
        None => std::env::var_os(key).is_some(),
        Some(expected) => std::env::var(key).ok().as_deref() == Some(expected),
    })
}

#[doc(hidden)]
pub struct Hegel<F> {
    test_fn: F,
    database_key: Option<String>,
    test_location: Option<TestLocation>,
    settings: Settings,
    reproduce_failure: Option<String>,
}

impl<F> Hegel<F>
where
    F: FnMut(TestCase),
{
    /// Create a new test builder with default settings.
    pub fn new(test_fn: F) -> Self {
        Self {
            test_fn,
            database_key: None,
            settings: Settings::new(),
            test_location: None,
            reproduce_failure: None,
        }
    }

    /// Override the default settings.
    pub fn settings(mut self, settings: Settings) -> Self {
        self.settings = settings;
        self
    }

    #[doc(hidden)]
    pub fn __database_key(mut self, key: String) -> Self {
        self.database_key = Some(key);
        self
    }

    #[doc(hidden)]
    pub fn test_location(mut self, location: TestLocation) -> Self {
        self.test_location = Some(location);
        self
    }

    /// Replay a single failing example from a base64 failure blob instead of
    /// generating fresh test cases.
    ///
    /// A failure blob encodes the choice sequence of a counterexample.
    /// Enable [`print_blob`](Settings::print_blob) to have a native failure
    /// print one. When set, [`run`](Self::run) decodes it and runs exactly
    /// that one example — bypassing generation and shrinking — so you can
    /// reproduce a CI failure locally and deterministically.
    ///
    /// First-wins: if a blob is already set, further calls are ignored.
    /// Stacked `#[hegel::reproduce_failure]` attributes lower to repeated
    /// calls here, so only the first attribute replays; the rest are
    /// bookkeeping to be deleted one by one as the failures are fixed.
    pub fn reproduce_failure(mut self, blob: impl Into<String>) -> Self {
        if self.reproduce_failure.is_none() {
            self.reproduce_failure = Some(blob.into());
        }
        self
    }

    /// Run the property-based tests.
    ///
    /// Panics if any test case fails.
    pub fn run(self) {
        if let Some(blob) = self.reproduce_failure {
            crate::run_lifecycle::drive_blob_replay(
                self.test_fn,
                &self.settings,
                self.database_key.as_deref(),
                &blob,
                self.test_location.as_ref(),
            );
            return;
        }

        crate::run_lifecycle::drive(
            self.test_fn,
            &self.settings,
            self.database_key.as_deref(),
            self.test_location.as_ref(),
        );
    }
}

#[cfg(test)]
#[path = "../tests/embedded/runner_tests.rs"]
mod tests;

use crate::antithesis::TestLocation;
use crate::test_case::TestCase;

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
    /// and reproduce generation directly. The shipped `antithesis` settings
    /// profile selects this backend.
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
/// # Profiles
///
/// The values a `Settings` starts from come from a named *profile*,
/// resolved by the engine. Two names are reserved: `base` is the immutable
/// base settings, and `default` is the default profile — the one in effect
/// when nothing names a profile, chosen by [`Settings::set_default_profile`],
/// `HEGEL_DEFAULT_PROFILE`, or the `default` entry in `hegel.toml`, else by
/// the environment (`antithesis` inside Antithesis, `ci` on a CI server,
/// `development` locally). [`Settings::new`] resolves `default`;
/// [`Settings::from_profile`] resolves a profile by name. Profiles are
/// modified and defined in a `hegel.toml` at the package or workspace root,
/// or registered with [`Settings::register_profile`].
///
/// The [`docs::settings`](crate::docs::settings) page covers the whole
/// system: every setting and the layers it can be set in, the shipped
/// profiles, inheritance, the `hegel.toml` format, and the programmatic
/// API.
#[derive(Debug, Clone)]
pub struct Settings {
    pub(crate) test_cases: u64,
    pub(crate) verbosity: Verbosity,
    pub(crate) seed: Option<u64>,
    pub(crate) derandomize: bool,
    pub(crate) database: Database,
    pub(crate) suppress_health_check: Vec<HealthCheck>,
    pub(crate) phases: Vec<Phase>,
    pub(crate) report_multiple_failures: bool,
    pub(crate) show_statistics: bool,
    pub(crate) print_blob: bool,
    pub(crate) backend: Backend,
}

impl Settings {
    /// Create settings from the `default` profile described in the
    /// [profiles](Settings#profiles) section. Panics when profile
    /// resolution fails: a default-profile setting names an unknown
    /// profile, or a `hegel.toml` is malformed.
    pub fn new() -> Self {
        Self::from_resolution(crate::ffi::settings_from_profile(None))
    }

    /// Create settings from the named profile: reserved (`base`,
    /// `default`), shipped (`development`, `ci`, `antithesis`), defined in
    /// `hegel.toml`, or registered with [`Settings::register_profile`].
    /// Selecting a profile does not change what the default profile is, and
    /// the named profile still implicitly extends `default`, so it layers
    /// over the environment's profile — except `base`, which is always the
    /// plain base settings. Panics when the profile is unknown or a
    /// `hegel.toml` is malformed; [`Settings::try_from_profile`] reports
    /// the failure as an `Err` instead.
    pub fn from_profile(name: &str) -> Self {
        Self::from_resolution(Self::try_from_profile(name).map_err(|e| e.message))
    }

    /// [`Settings::from_profile`], reporting resolution failure as an `Err`
    /// carrying the engine's diagnostic instead of panicking.
    pub fn try_from_profile(name: &str) -> Result<Self, ProfileError> {
        crate::ffi::settings_from_profile(Some(name)).map_err(|message| ProfileError { message })
    }

    fn from_resolution(resolution: Result<Self, String>) -> Self {
        resolution.unwrap_or_else(|message| crate::test_case::invalid_argument!("{message}"))
    }

    /// Set the default profile for the whole process: the profile
    /// [`Settings::new`] resolves and profiles without `extends` layer
    /// over. Takes precedence over `HEGEL_DEFAULT_PROFILE` and the
    /// `default` entry in `hegel.toml`; the `--profile` flag of a
    /// `#[hegel::main]` binary calls this. The name is not required to
    /// exist yet.
    ///
    /// Like [`Settings::register_profile`] this is not retroactive, so it
    /// must run before the tests that should see it; under `cargo test`
    /// prefer the `default` entry in `hegel.toml`.
    ///
    /// Panics when `name` is not a valid profile name.
    pub fn set_default_profile(name: &str) {
        if let Err(message) = crate::ffi::set_default_profile(Some(name)) {
            crate::test_case::invalid_argument!("{message}");
        }
    }

    /// Register a complete snapshot of `settings` as the profile `name`,
    /// process-wide, replacing any earlier registration of the same name.
    /// Registering a shipped profile's name replaces that profile; a
    /// `hegel.toml` section for `name` still merges on top of the snapshot.
    ///
    /// Registration is not retroactive (settings values already created
    /// keep their fields), so it must run before the tests that use the
    /// profile. A `#[hegel::main]` binary or an embedding controls that
    /// ordering. Under `cargo test` there is no reliable pre-test hook, so
    /// prefer `hegel.toml` there.
    ///
    /// Panics when `name` is not a valid profile name (ASCII letters,
    /// digits, `-` and `_`) or is one of the reserved names `base` and
    /// `default`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::Settings;
    ///
    /// Settings::register_profile("nightly", Settings::from_profile("ci").test_cases(10_000));
    /// ```
    pub fn register_profile(name: &str, settings: Settings) {
        if let Err(message) = crate::ffi::register_profile(name, &settings) {
            crate::test_case::invalid_argument!("{message}");
        }
    }

    /// Select the randomness backend (base value: [`Backend::Default`]; the
    /// shipped `antithesis` profile selects [`Backend::Urandom`]).
    pub fn backend(mut self, backend: Backend) -> Self {
        self.backend = backend;
        self
    }

    /// Set the number of test cases to run (default: 100).
    ///
    /// The `HEGEL_TEST_CASES` environment variable, when set and non-empty,
    /// overrides this value at runtime — including a value set explicitly
    /// here or via `#[hegel::test(test_cases = ...)]`. This makes it easy to
    /// scale a whole test suite up (a nightly deep run) or down (a quick
    /// smoke pass) without editing source.
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

    /// When true, use a fixed seed derived from the test name. Enabled by
    /// the shipped `ci` profile.
    pub fn derandomize(mut self, derandomize: bool) -> Self {
        self.derandomize = derandomize;
        self
    }

    /// Set the database path for storing failing examples, or `None` to disable.
    ///
    /// The `HEGEL_DATABASE` environment variable, when set and non-empty,
    /// overrides this value at runtime: the literal value `disabled` turns
    /// the database off (matching the `--database` CLI flag's keyword), and
    /// any other value is used as the database path.
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
    /// ```no_run
    /// use hegel::{Phase, Settings};
    ///
    /// let s = Settings::new().phases([Phase::Reuse, Phase::Generate]);
    /// ```
    pub fn phases(mut self, phases: impl IntoIterator<Item = Phase>) -> Self {
        self.phases = phases.into_iter().collect();
        self
    }

    /// Print a copy-pasteable `#[hegel::reproduce_failure("…")]` line for the
    /// counterexample when a test fails. Defaults to `false`; the shipped
    /// `ci` profile turns it on, since with the database disabled the blob
    /// is the way to reproduce a CI failure locally.
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

    /// Print event statistics at the end of the run (default: off): for
    /// each label recorded with [`TestCase::event`](crate::TestCase::event),
    /// the fraction of generation-phase test cases it occurred in, and for
    /// each label recorded with
    /// [`TestCase::event_value`](crate::TestCase::event_value), a summary of
    /// the observed distribution.
    ///
    /// The `HEGEL_STATISTICS` environment variable, when set to anything
    /// but `"0"` or the empty string, turns this on at runtime without
    /// editing source.
    pub fn show_statistics(mut self, show_statistics: bool) -> Self {
        self.show_statistics = show_statistics;
        self
    }

    /// Apply environment-variable overrides to these settings. Called once
    /// per run, after all builder configuration, so the environment wins
    /// over values set in source.
    pub(crate) fn with_env_overrides(self) -> Self {
        self.with_env_overrides_from(env_var)
    }

    fn with_env_overrides_from(mut self, env: impl Fn(&str) -> Option<String>) -> Self {
        if let Some(value) = env("HEGEL_TEST_CASES") {
            if !value.is_empty() {
                match value.parse::<u64>() {
                    Ok(n) if n > 0 => self.test_cases = n,
                    _ => panic!("HEGEL_TEST_CASES must be a positive integer, got {value:?}"),
                }
            }
        }
        if let Some(value) = env("HEGEL_DATABASE") {
            if !value.is_empty() {
                self.database = if value == "disabled" {
                    Database::Disabled
                } else {
                    Database::Path(value)
                };
            }
        }
        if let Some(value) = env("HEGEL_STATISTICS") {
            if !value.is_empty() && value != "0" {
                self.show_statistics = true;
            }
        }
        self
    }

    /// The settings a `#[hegel::main]` binary runs with: one test case, with
    /// the `TooSlow` and `TestCasesTooLarge` health checks suppressed, since
    /// both measure how valid test cases accumulate over a run and a run of
    /// one has nothing to measure.
    pub(crate) fn for_single_test_case(mut self) -> Self {
        self.test_cases = 1;
        for check in [HealthCheck::TooSlow, HealthCheck::TestCasesTooLarge] {
            if !self.suppress_health_check.contains(&check) {
                self.suppress_health_check.push(check);
            }
        }
        self
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

/// Why [`Settings::try_from_profile`] could not resolve a profile: the name
/// is unknown, or a `hegel.toml` is malformed. The `Display` impl carries
/// the engine's diagnostic.
#[derive(Debug, Clone)]
pub struct ProfileError {
    pub(crate) message: String,
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProfileError {}

#[doc(hidden)]
pub fn hegel<F>(test_fn: F)
where
    F: FnMut(TestCase),
{
    Hegel::new(test_fn).run();
}

fn env_var(key: &str) -> Option<String> {
    std::env::var_os(key).map(|value| value.to_string_lossy().into_owned())
}

#[doc(hidden)]
pub struct Hegel<F> {
    test_fn: F,
    database_key: Option<String>,
    test_location: Option<TestLocation>,
    settings: Settings,
    reproduce_failure: Option<String>,
    single_test_case: bool,
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
            single_test_case: false,
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

    /// Run exactly one test case, the behavior of `#[hegel::main]` binaries.
    /// Applied after the environment overrides in [`run`](Self::run), so
    /// `HEGEL_TEST_CASES` cannot undo it. Also suppresses
    /// [`HealthCheck::TooSlow`] and [`HealthCheck::TestCasesTooLarge`]: both
    /// judge how a run accumulates valid test cases, which is meaningless
    /// for a run of one.
    #[doc(hidden)]
    pub fn __single_test_case(mut self) -> Self {
        self.single_test_case = true;
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
        let mut settings = self.settings.with_env_overrides();
        if self.single_test_case {
            settings = settings.for_single_test_case();
        }
        if let Some(blob) = self.reproduce_failure {
            crate::run_lifecycle::drive_blob_replay(
                self.test_fn,
                &settings,
                self.database_key.as_deref(),
                &blob,
                self.test_location.as_ref(),
            );
            return;
        }

        crate::run_lifecycle::drive(
            self.test_fn,
            &settings,
            self.database_key.as_deref(),
            self.test_location.as_ref(),
        );
    }
}

#[cfg(test)]
#[path = "../tests/embedded/runner_tests.rs"]
mod tests;

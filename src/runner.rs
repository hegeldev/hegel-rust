use crate::antithesis::TestLocation;
use crate::settings::Settings;
use crate::test_case::TestCase;

// ─── Hegel test builder ─────────────────────────────────────────────────────

// internal use only
#[doc(hidden)]
pub fn hegel<F>(test_fn: F)
where
    F: FnMut(TestCase),
{
    Hegel::new(test_fn).run();
}

// internal use only
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
        // A blob replay is a single deterministic case — no generation,
        // targeting, or shrinking — so it is phase-agnostic and takes
        // precedence over the normal run.
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

        // Everything else — including `Mode::SingleTestCase`, which the engine
        // handles from the settings — drives the engine through the C ABI.
        // There is no early-out when `Phase::Generate` is absent: the phases
        // are independent (e.g. `phases = [Phase::Reuse]` must still replay
        // stored counterexamples), and the engine skips whatever is disabled.
        crate::run_lifecycle::drive(
            self.test_fn,
            &self.settings,
            self.database_key.as_deref(),
            self.test_location.as_ref(),
        );
    }
}

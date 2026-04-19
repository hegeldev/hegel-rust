//! Swarm-testing feature flags.
//!
//! Port of Hypothesis's `FeatureFlags` and `FeatureStrategy` from
//! `hypothesis.strategies._internal.featureflags`. Selectively enables or
//! disables named features per test case so that bugs involving feature
//! interactions can be surfaced, with a shrink direction of "all features
//! enabled".

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::TestCase;
use crate::generators::Generator;

/// Tracks which features are enabled for a test case.
///
/// Hypothesis: `FeatureFlags` (`hypothesis.strategies._internal.featureflags`).
#[derive(Clone)]
pub struct FeatureFlags {
    inner: Arc<Mutex<FeatureFlagsInner>>,
}

#[allow(dead_code)] // at_least_one_of / p_disabled are written by the (stubbed) generator.
struct FeatureFlagsInner {
    is_disabled: HashMap<String, bool>,
    at_least_one_of: HashSet<String>,
    p_disabled: f64,
    frozen: bool,
}

impl FeatureFlags {
    /// Construct a FeatureFlags outside a test context: all features enabled
    /// by default, no decisions recorded.
    pub fn new() -> Self {
        FeatureFlags::with_flags(std::iter::empty::<String>(), std::iter::empty::<String>())
    }

    /// Construct a FeatureFlags with pre-seeded enabled / disabled names.
    ///
    /// Used outside a test context (e.g. for round-tripping). Inside a test
    /// run, use `FeatureStrategy` instead.
    pub fn with_flags<E, D, S>(enabled: E, disabled: D) -> Self
    where
        E: IntoIterator<Item = S>,
        D: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut is_disabled = HashMap::new();
        for name in enabled {
            is_disabled.insert(name.into(), false);
        }
        for name in disabled {
            is_disabled.insert(name.into(), true);
        }
        FeatureFlags {
            inner: Arc::new(Mutex::new(FeatureFlagsInner {
                is_disabled,
                at_least_one_of: HashSet::new(),
                p_disabled: 0.0,
                frozen: true,
            })),
        }
    }

    /// Returns whether the feature named `name` is enabled on this test run.
    pub fn is_enabled(&self, name: &str) -> bool {
        let inner = self.inner.lock().unwrap();
        if let Some(&is_disabled) = inner.is_disabled.get(name) {
            return !is_disabled;
        }
        if inner.frozen {
            // Frozen / example mode: unknown features default to enabled,
            // matching Hypothesis's shrink direction.
            return true;
        }
        drop(inner);
        // Active test: need to make a weighted draw through the native
        // engine. Not yet wired up — the fixer loop will pick this up.
        todo!(
            "FeatureFlags::is_enabled on an active test case: weighted-draw \
             primitive needs to be wired through the native engine."
        )
    }
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for FeatureFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.lock().unwrap();
        let mut enabled: Vec<&String> = inner
            .is_disabled
            .iter()
            .filter_map(|(k, &v)| (!v).then_some(k))
            .collect();
        let mut disabled: Vec<&String> = inner
            .is_disabled
            .iter()
            .filter_map(|(k, &v)| v.then_some(k))
            .collect();
        enabled.sort();
        disabled.sort();
        f.debug_struct("FeatureFlags")
            .field("enabled", &enabled)
            .field("disabled", &disabled)
            .finish()
    }
}

/// Generator producing [`FeatureFlags`].
///
/// Hypothesis: `FeatureStrategy` (same module as `FeatureFlags`).
#[derive(Clone, Default)]
pub struct FeatureStrategy {
    at_least_one_of: HashSet<String>,
}

impl FeatureStrategy {
    pub fn new() -> Self {
        FeatureStrategy::default()
    }

    /// Require that at least one of `names` remains enabled per test run.
    ///
    /// Matches Hypothesis's `at_least_one_of` keyword on `FeatureStrategy`.
    pub fn at_least_one_of<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.at_least_one_of = names.into_iter().map(Into::into).collect();
        self
    }
}

impl Generator<FeatureFlags> for FeatureStrategy {
    fn do_draw(&self, _tc: &TestCase) -> FeatureFlags {
        // To port Hypothesis semantics we need:
        //   - draw an integer in [0, 254] for p_disabled
        //   - for each is_enabled() call during the test, draw a weighted
        //     boolean with probability p_disabled, using the forced value
        //     when at_least_one_of constraints require it
        // The weighted-draw primitive is internal to `src/native/core`; a
        // follow-up commit will expose it for this generator to call.
        let _ = &self.at_least_one_of;
        todo!(
            "FeatureStrategy::do_draw: needs weighted-boolean access from the \
             native engine (`NativeTestCase::weighted`). See \
             src/native/featureflags.rs."
        )
    }
}

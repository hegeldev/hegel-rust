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
use crate::native::core::StopTest;
use crate::native::with_native_tc;
use crate::test_case::STOP_TEST_STRING;

/// Tracks which features are enabled for a test case.
///
/// Hypothesis: `FeatureFlags` (`hypothesis.strategies._internal.featureflags`).
#[derive(Clone)]
pub struct FeatureFlags {
    inner: Arc<Mutex<FeatureFlagsInner>>,
}

struct FeatureFlagsInner {
    is_disabled: HashMap<String, bool>,
    at_least_one_of: HashSet<String>,
    p_disabled: f64,
    /// True when this FeatureFlags has no live test case backing it (either
    /// constructed outside a test run, or the run it was created in has
    /// since completed). In that state, `is_enabled` uses only the stored
    /// enable/disable map and shrink-open defaults.
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

    /// Construct a live FeatureFlags for an in-progress test case.
    ///
    /// Called from `FeatureStrategy::do_draw` after drawing `p_disabled`.
    fn live(p_disabled: f64, at_least_one_of: HashSet<String>) -> Self {
        FeatureFlags {
            inner: Arc::new(Mutex::new(FeatureFlagsInner {
                is_disabled: HashMap::new(),
                at_least_one_of,
                p_disabled,
                frozen: false,
            })),
        }
    }

    /// Returns whether the feature named `name` is enabled on this test run.
    pub fn is_enabled(&self, name: &str) -> bool {
        let (p_disabled, forced) = {
            let inner = self.inner.lock().unwrap();
            if inner.frozen {
                return !inner.is_disabled.get(name).copied().unwrap_or(false);
            }
            // Live path: compute the forced argument against the current
            // oneof/is_disabled state before any mutation.
            let oneof = &inner.at_least_one_of;
            let forced = if oneof.len() == 1 && oneof.contains(name) {
                Some(false)
            } else {
                inner.is_disabled.get(name).copied()
            };
            (inner.p_disabled, forced)
        };

        // A live FeatureFlags may outlive its generating test case (e.g. be
        // returned from `find_any` / `minimal` and inspected afterwards). In
        // that case the thread-local handle is gone, so fall back to the
        // frozen-mode default: enabled, unless we already recorded a decision.
        let Some(is_disabled) = with_native_tc(|handle| {
            handle.map(|handle| {
                let start = handle.borrow().nodes.len();
                let value = match handle.borrow_mut().weighted(p_disabled, forced) {
                    Ok(v) => v,
                    Err(StopTest) => panic!("{}", STOP_TEST_STRING),
                };
                let end = handle.borrow().nodes.len();
                handle
                    .borrow_mut()
                    .record_span(start, end, "feature_flag".to_string());
                value
            })
        }) else {
            let inner = self.inner.lock().unwrap();
            return !inner.is_disabled.get(name).copied().unwrap_or(false);
        };

        let mut inner = self.inner.lock().unwrap();
        inner.is_disabled.insert(name.to_string(), is_disabled);
        if inner.at_least_one_of.contains(name) && !is_disabled {
            inner.at_least_one_of.clear();
        }
        inner.at_least_one_of.remove(name);
        !is_disabled
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

#[cfg(test)]
#[path = "../../tests/embedded/native/featureflags_tests.rs"]
mod tests;

impl Generator<FeatureFlags> for FeatureStrategy {
    fn do_draw(&self, _tc: &TestCase) -> FeatureFlags {
        // Mirrors Hypothesis's FeatureFlags.__init__: draw an integer in
        // [0, 254] to decide the probability that each individual feature
        // is disabled. Zero (the shrink target) means every feature is
        // enabled.
        let p_disabled = with_native_tc(|handle| {
            let handle =
                handle.expect("FeatureStrategy::do_draw called outside the native test context");
            match handle.borrow_mut().draw_integer(0, 254) {
                Ok(n) => n as f64 / 255.0,
                Err(StopTest) => panic!("{}", STOP_TEST_STRING),
            }
        });
        FeatureFlags::live(p_disabled, self.at_least_one_of.clone())
    }
}

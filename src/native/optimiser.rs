// Hypothesis's targeted property-based testing: `target_observations`,
// `best_observed_targets`, and the `optimise_targets` hill-climbing pass.
//
// This module is a stub — every public method panics with `todo!()`. It
// exists so that the ported `tests/hypothesis/conjecture_optimiser.rs`
// compiles in both server and native modes. See TODO.yaml
// "Implement native targeting/optimiser" for the acceptance criteria and
// design sketch.

use std::collections::HashMap;

use rand::rngs::SmallRng;

use crate::native::core::{ChoiceValue, Status};
use crate::native::intervalsets::IntervalSet;

/// Sentinel returned from [`TargetedRunner::optimise_targets`] when the run
/// budget is exhausted. Port of Hypothesis's `RunIsComplete`.
#[derive(Debug, Clone, Copy)]
pub struct RunIsComplete;

/// Settings snapshot for [`TargetedRunner`]. The upstream file only tweaks
/// `max_examples`; everything else defaults.
pub struct TargetedRunnerSettings {
    pub max_examples: usize,
}

impl TargetedRunnerSettings {
    pub fn new() -> Self {
        TargetedRunnerSettings { max_examples: 100 }
    }

    pub fn max_examples(mut self, n: usize) -> Self {
        self.max_examples = n;
        self
    }
}

impl Default for TargetedRunnerSettings {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a single [`TargetedRunner::cached_test_function`] call.
pub struct CachedTestResult {
    pub status: Status,
}

/// Test-case surface passed to the runner callback. Exposes a mutable
/// `target_observations` map (the hill-climber's objective) plus the draw /
/// span / invalidity methods exercised by `test_optimiser.py`.
#[non_exhaustive]
pub struct TargetedTestCase {
    pub target_observations: HashMap<String, f64>,
}

impl TargetedTestCase {
    pub fn draw_integer(&mut self, _min_value: i128, _max_value: i128) -> i128 {
        todo!(
            "TargetedTestCase::draw_integer — native targeting/optimiser not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }

    pub fn draw_boolean(&mut self, _p: f64) -> bool {
        todo!(
            "TargetedTestCase::draw_boolean — native targeting/optimiser not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }

    pub fn draw_bytes(&mut self, _min_size: usize, _max_size: usize) -> Vec<u8> {
        todo!(
            "TargetedTestCase::draw_bytes — native targeting/optimiser not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }

    pub fn draw_string(
        &mut self,
        _intervals: &IntervalSet,
        _min_size: usize,
        _max_size: usize,
    ) -> String {
        todo!(
            "TargetedTestCase::draw_string — native targeting/optimiser not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }

    pub fn mark_invalid(&mut self) {
        todo!(
            "TargetedTestCase::mark_invalid — native targeting/optimiser not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }

    pub fn start_span(&mut self, _label: u64) {
        todo!(
            "TargetedTestCase::start_span — native targeting/optimiser not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }

    pub fn stop_span(&mut self) {
        todo!(
            "TargetedTestCase::stop_span — native targeting/optimiser not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }
}

/// Port of Hypothesis's `ConjectureRunner` surface used by `test_optimiser.py`.
///
/// A real implementation would own the user test function, a PRNG, a tree of
/// observed test cases, and the per-target running maximum. The current stub
/// is a unit struct whose associated methods all panic with `todo!()`.
#[non_exhaustive]
pub struct TargetedRunner;

impl TargetedRunner {
    pub fn new<F>(_test_fn: F, _settings: TargetedRunnerSettings, _rng: SmallRng) -> Self
    where
        F: FnMut(&mut TargetedTestCase) + 'static,
    {
        todo!(
            "TargetedRunner::new — native targeting/optimiser not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }

    pub fn cached_test_function(&mut self, _choices: &[ChoiceValue]) -> CachedTestResult {
        todo!(
            "TargetedRunner::cached_test_function — native targeting/optimiser not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }

    /// `cached_test_function` with `extend=N` from Hypothesis — after replaying
    /// `choices`, fill up to `extend` additional random draws.
    pub fn cached_test_function_extend(
        &mut self,
        _choices: &[ChoiceValue],
        _extend: usize,
    ) -> CachedTestResult {
        todo!(
            "TargetedRunner::cached_test_function_extend — native targeting/optimiser not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }

    pub fn optimise_targets(&mut self) -> Result<(), RunIsComplete> {
        todo!(
            "TargetedRunner::optimise_targets — native targeting/optimiser not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }

    pub fn best_observed_targets(&self) -> &HashMap<String, f64> {
        todo!(
            "TargetedRunner::best_observed_targets — native targeting/optimiser not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }
}

/// RAII guard that temporarily lowers the native engine's buffer size limit.
/// Port of `tests/conjecture/common.py::buffer_size_limit`.
#[non_exhaustive]
pub struct BufferSizeLimit;

impl BufferSizeLimit {
    pub fn new(_n: usize) -> Self {
        todo!(
            "BufferSizeLimit::new — native buffer size override not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }
}

impl Drop for BufferSizeLimit {
    fn drop(&mut self) {
        todo!(
            "BufferSizeLimit::drop — native buffer size override not implemented \
             (see TODO.yaml 'Implement native targeting/optimiser')"
        )
    }
}

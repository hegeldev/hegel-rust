//! Pass scheduling for the shrinker.
//!
//! Each pass is wrapped in a `ShrinkPass` with per-pass stats (calls,
//! shrinks, deletions) and the outer loop re-sorts them by recent
//! success so useful passes float to the front of the list.
//!
//! The "step" granularity is one whole pass invocation: a step is
//! considered to have made progress when `Shrinker::improvements` —
//! the count of accepted strict shrinks — bumped during the pass. A
//! finer-grained step is a future refinement; the scheduling skeleton
//! here stays the same either way.

use alloc::boxed::Box;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;

use super::{ShrinkResult, Shrinker};

/// A boxed shrink-pass step. Returns [`ShrinkHalt::Stop`](super::ShrinkHalt::Stop) once the
/// shrink deadline has passed so the scheduler unwinds promptly. The step
/// borrows the shrinker for the duration of the returned future, which is
/// boxed via [`boxed_pass`](super::boxed_pass) so the closure's return type
/// is nameable.
pub type ShrinkPassFn<'a> = Box<
    dyn for<'s> FnMut(
            &'s mut Shrinker<'a>,
        ) -> Pin<Box<dyn Future<Output = ShrinkResult<()>> + Send + 's>>
        + Send
        + 'a,
>;

/// One scheduled shrink pass with per-pass statistics.
///
/// The `run` callback is invoked by [`Shrinker::fixate_shrink_passes`]
/// with the active shrinker; each invocation should attempt one "step"
/// of the underlying pass and let the scheduler decide whether to call
/// it again.
pub struct ShrinkPass<'a> {
    /// Display name.  Read by `fixate_shrink_passes` for the per-pass
    /// "Trying shrink pass: <name>" debug line and by
    /// `Shrinker::shrink`'s end-of-run profile report.
    pub name: &'static str,
    /// The callable to run.
    pub run: ShrinkPassFn<'a>,
    /// Total times this pass has been stepped.
    pub calls: usize,
    /// Times the pass step strictly improved the shrink target.
    pub shrinks: usize,
    /// Times the pass step reduced the sequence length.
    pub deletions: usize,
    /// Whether a step's outcome depends on fresh randomness (probe-based
    /// passes). Deterministic passes fixate after one non-improving step —
    /// re-stepping an unchanged target would re-propose the same candidates
    /// — while stochastic passes keep their consecutive-failure retry
    /// budget, since a re-step draws new random continuations.
    pub stochastic: bool,
}

impl<'a> ShrinkPass<'a> {
    /// Construct a named pass from a closure.
    pub fn new(name: &'static str, run: ShrinkPassFn<'a>) -> Self {
        ShrinkPass {
            name,
            run,
            calls: 0,
            shrinks: 0,
            deletions: 0,
            stochastic: false,
        }
    }

    /// Mark the pass as stochastic (see [`ShrinkPass::stochastic`]).
    pub fn stochastic(mut self) -> Self {
        self.stochastic = true;
        self
    }
}

impl<'a> Shrinker<'a> {
    /// Run the supplied list of passes to a fixed point.
    ///
    /// * Each outer iteration steps every pass; a deterministic pass is
    ///   re-stepped only while each step strictly improves the shrink
    ///   target. A step that completes without improving ends the pass's
    ///   work for this iteration — one step already attempts the pass's
    ///   whole repertoire against the current target, so re-stepping an
    ///   unchanged target would only re-propose the same candidates.
    /// * A [stochastic](ShrinkPass::stochastic) pass draws fresh random
    ///   continuations on every step, so it keeps a retry budget of
    ///   `STOCHASTIC_MAX_FAILURES` consecutive non-improving steps (the
    ///   budget every pass had before deterministic passes fixated early).
    /// * Inside each per-pass loop, `Shrinker::max_stall` is grown to
    ///   `max(max_stall, 2 * max_calls_per_failing_step + (calls -
    ///   calls_at_loop_start))` so a long shrink search where each step
    ///   is expensive doesn't get cut off by the stall guard.
    /// * Between outer iterations, passes are re-sorted by reorder key:
    ///   passes that deleted nodes (-1) come first, then passes that
    ///   changed shape (0), then useless passes (1).
    ///
    /// Returns when no pass made any progress over a full outer
    /// iteration. Called by [`Shrinker::shrink`].
    pub async fn fixate_shrink_passes(
        &mut self,
        passes: &mut Vec<ShrinkPass<'a>>,
    ) -> ShrinkResult<()> {
        const STOCHASTIC_MAX_FAILURES: usize = 6;
        let mut any_ran = true;
        while any_ran {
            any_ran = false;
            let mut can_discard = self.remove_discarded().await?;
            let calls_at_loop_start = self.calls;
            let mut max_calls_per_failing_step: usize = 1;
            let mut reorder_keys: Vec<i32> = vec![0; passes.len()];
            for idx in 0..passes.len() {
                if can_discard {
                    can_discard = self.remove_discarded().await?;
                }
                let before_nodes_len = self.current_nodes.len();
                let epoch_before_pass = self.improvements;
                let max_failures = if passes[idx].stochastic {
                    STOCHASTIC_MAX_FAILURES
                } else {
                    1
                };
                let mut failures: usize = 0;

                while failures < max_failures {
                    let span = self.calls.saturating_sub(calls_at_loop_start);
                    let target = max_calls_per_failing_step
                        .saturating_mul(2)
                        .saturating_add(span);
                    if target > self.max_stall {
                        self.max_stall = target;
                    }

                    if self.debug.is_some() {
                        let name = passes[idx].name;
                        self.debug_msg(&format!("Trying shrink pass: {name}"));
                    }
                    passes[idx].calls += 1;
                    let epoch_before_iter = self.improvements;
                    let initial_calls = self.calls;
                    (passes[idx].run)(self).await?;
                    if self.improvements > epoch_before_iter {
                        passes[idx].shrinks += 1;
                        if self.current_nodes.len() < before_nodes_len {
                            passes[idx].deletions += 1;
                        }
                        any_ran = true;
                        failures = 0;
                    } else if initial_calls != self.calls {
                        max_calls_per_failing_step = max_calls_per_failing_step
                            .max(self.calls.saturating_sub(initial_calls));
                        failures += 1;
                    } else {
                        break;
                    }
                }

                reorder_keys[idx] = if self.current_nodes.len() < before_nodes_len {
                    -1
                } else if self.improvements > epoch_before_pass {
                    0
                } else {
                    1
                };
            }

            let mut indexed: Vec<(i32, ShrinkPass<'a>)> = core::mem::take(passes)
                .into_iter()
                .enumerate()
                .map(|(i, pass)| (reorder_keys[i], pass))
                .collect();
            indexed.sort_by_key(|(key, _)| *key);
            passes.extend(indexed.into_iter().map(|(_, pass)| pass));
        }
        Ok(())
    }

    /// Read-only access to per-pass stats; used by `shrink`'s profile
    /// report and by tests asserting that `fixate_shrink_passes` actually
    /// drives each pass.
    ///
    /// Returns `(name, calls, shrinks, deletions)` tuples for each pass
    /// in `passes`.
    pub fn pass_stats(
        &self,
        passes: &[ShrinkPass<'a>],
    ) -> Vec<(&'static str, usize, usize, usize)> {
        passes
            .iter()
            .map(|sp| (sp.name, sp.calls, sp.shrinks, sp.deletions))
            .collect()
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_scheduling_tests.rs"]
mod tests;

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

use super::{ShrinkResult, Shrinker};

/// A boxed shrink-pass step. Returns [`ShrinkStop`](super::ShrinkStop) once the
/// shrink deadline has passed so the scheduler unwinds promptly.
pub type ShrinkPassFn<'a> = Box<dyn FnMut(&mut Shrinker<'a>) -> ShrinkResult<()> + 'a>;

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
    /// The `improvements` epoch the pass last ran at. Every pass is
    /// deterministic given the shrink target, so re-running it against an
    /// unchanged target is pure waste (consider-level candidates are
    /// absorbed by the result cache, but probe executions are not). This
    /// is the analogue of Hypothesis's per-pass ChoiceTree being
    /// exhausted: the tree only resets when the shrink target changes.
    last_run_epoch: Option<usize>,
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
            last_run_epoch: None,
        }
    }
}

impl<'a> Shrinker<'a> {
    /// Run the supplied list of passes to a fixed point.
    ///
    /// * Each pass keeps running while it strictly improves the shrink
    ///   target, and is otherwise exhausted until the target changes —
    ///   the analogue of Hypothesis's per-pass choice trees, which only
    ///   reset on a new shrink target.
    /// * Before each pass run, `Shrinker::max_stall` is grown to
    ///   `max(max_stall, 2 * max_calls_per_failing_step + (calls -
    ///   calls_at_loop_start))` so a long shrink search where each step
    ///   is expensive doesn't get cut off by the stall guard, and the
    ///   stall guard itself is enforced (ending the whole shrink, like
    ///   Hypothesis's `StopShrinking`).
    /// * Between outer iterations, passes are re-sorted by reorder key:
    ///   passes that deleted nodes (-1) come first, then passes that
    ///   changed shape (0), then useless passes (1).
    ///
    /// Returns when no pass made any progress over a full outer
    /// iteration. Called by [`Shrinker::shrink`].
    pub fn fixate_shrink_passes(&mut self, passes: &mut [ShrinkPass<'a>]) -> ShrinkResult<()> {
        let mut any_ran = true;
        while any_ran {
            any_ran = false;
            // Try the cleanup pass once at the start of each loop.
            let mut can_discard = self.remove_discarded()?;
            let calls_at_loop_start = self.calls;
            let mut max_calls_per_failing_step: usize = 1;
            let mut reorder_keys: Vec<i32> = vec![0; passes.len()];
            for idx in 0..passes.len() {
                if can_discard {
                    can_discard = self.remove_discarded()?;
                }
                let before_nodes_len = self.current_nodes.len();
                let epoch_before_pass = self.improvements;

                // Each pass is deterministic — running it again with the
                // shrink target unchanged would simply repeat the same
                // work, so the pass keeps running only while it strictly
                // improves and is otherwise exhausted until the target
                // changes (Hypothesis's per-pass choice tree resetting on
                // a new shrink target).
                //
                // `improvements` increments only when `accept_improvement`
                // commits a strictly-smaller candidate, so its delta
                // across a pass invocation is exactly "did the pass shrink
                // anything?" — no per-iteration `sort_key` snapshot needed.
                loop {
                    // Exhaustion: the pass already ran against this exact
                    // shrink target (`improvements` uniquely identifies it).
                    if passes[idx].last_run_epoch == Some(self.improvements) {
                        break;
                    }
                    // Grow max_stall to leave breathing room for the
                    // remainder of this outer iteration.
                    let span = self.calls.saturating_sub(calls_at_loop_start);
                    let target = max_calls_per_failing_step
                        .saturating_mul(2)
                        .saturating_add(span);
                    if target > self.max_stall {
                        self.max_stall = target;
                    }
                    // The stall guard — Hypothesis's `check_calls` raising
                    // `StopShrinking`: once `max_stall` calls have passed
                    // without an accepted shrink, the whole shrink ends.
                    // Checked at pass-step boundaries (Hypothesis checks
                    // per call, but its steps are chooser-sized; checking
                    // mid-pass here would cut whole passes short that
                    // Python's intra-iteration padding lets finish).
                    if self.calls.saturating_sub(self.calls_at_last_shrink) >= self.max_stall {
                        return Err(super::ShrinkStop);
                    }

                    if self.debug.is_some() {
                        let name = passes[idx].name;
                        self.debug_msg(&format!("Trying shrink pass: {name}"));
                    }
                    passes[idx].calls += 1;
                    passes[idx].last_run_epoch = Some(self.improvements);
                    let epoch_before_iter = self.improvements;
                    let initial_calls = self.calls;
                    (passes[idx].run)(self)?;
                    if self.improvements > epoch_before_iter {
                        passes[idx].shrinks += 1;
                        if self.current_nodes.len() < before_nodes_len {
                            passes[idx].deletions += 1;
                        }
                        any_ran = true;
                    } else {
                        max_calls_per_failing_step = max_calls_per_failing_step
                            .max(self.calls.saturating_sub(initial_calls));
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

            // Stable-sort passes by their reorder key — passes that
            // deleted (key -1) float to the front.
            let mut indexed: Vec<(i32, usize)> = reorder_keys
                .iter()
                .enumerate()
                .map(|(i, &k)| (k, i))
                .collect();
            indexed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            // Apply the permutation in place.  Each pass moves once; we
            // use a temporary swap so the borrow checker stays happy.
            let permutation: Vec<usize> = indexed.iter().map(|t| t.1).collect();
            let mut new_order: Vec<Option<ShrinkPass<'a>>> =
                (0..passes.len()).map(|_| None).collect();
            for (dest, &src) in permutation.iter().enumerate() {
                new_order[dest] = Some(std::mem::replace(
                    &mut passes[src],
                    // Temporarily fill with a noop placeholder.
                    ShrinkPass::new("__placeholder__", Box::new(|_| Ok(()))),
                ));
            }
            for (dest, slot) in new_order.into_iter().enumerate() {
                passes[dest] = slot.expect("permutation fills every slot");
            }
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

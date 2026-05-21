//! Pass scheduling for the shrinker.
//!
//! Port of `shrinker.py:837-929` (`fixate_shrink_passes`).  Each pass is
//! wrapped in a `ShrinkPass` with per-pass stats (calls, shrinks,
//! deletions) and the outer loop re-sorts them by recent success so
//! useful passes float to the front of the list.
//!
//! Without a `ChoiceTree`/`Chooser` integration the "step" granularity
//! is one whole pass invocation: a step is considered to have made
//! progress when the shrink target's sort_key strictly decreased.  A
//! finer-grained `&mut Chooser`-based step is a future refinement; the
//! scheduling skeleton here stays the same either way.

use crate::native::core::sort_key;

use super::Shrinker;

/// One scheduled shrink pass with per-pass statistics.
///
/// Mirrors Hypothesis's `ShrinkPass` dataclass (`shrinker.py:122-140`).
/// The `run` callback is invoked by [`Shrinker::fixate_shrink_passes`]
/// with the active shrinker; each invocation should attempt one
/// "step" of the underlying pass and let the scheduler decide whether
/// to call it again.
pub struct ShrinkPass<'a> {
    /// Display name; appears in stats and debugging output.
    pub name: &'static str,
    /// The callable to run.
    pub run: Box<dyn FnMut(&mut Shrinker<'a>) + 'a>,
    /// Total times this pass has been stepped.
    pub calls: usize,
    /// Times the pass step strictly improved the shrink target.
    pub shrinks: usize,
    /// Times the pass step reduced the sequence length.
    pub deletions: usize,
}

impl<'a> ShrinkPass<'a> {
    /// Construct a named pass from a closure.
    pub fn new(name: &'static str, run: Box<dyn FnMut(&mut Shrinker<'a>) + 'a>) -> Self {
        ShrinkPass {
            name,
            run,
            calls: 0,
            shrinks: 0,
            deletions: 0,
        }
    }
}

impl<'a> Shrinker<'a> {
    /// Run the supplied list of passes to a fixed point.
    ///
    /// Mirrors Hypothesis's `fixate_shrink_passes` (`shrinker.py:837-929`):
    ///
    /// * Each outer iteration steps every pass up to `MAX_FAILURES = 20`
    ///   times in a row without progress.
    /// * Passes are stably re-sorted between iterations by their
    ///   reorder key — passes that deleted nodes (-1) come first, then
    ///   passes that changed shape (0), then useless passes (1).
    /// * `max_stall` grows on each successful shrink so a long
    ///   shrink-search doesn't trigger the runner's stall guard.
    ///
    /// Returns when no pass made any progress over a full outer
    /// iteration.  Called by [`Shrinker::shrink`].
    pub fn fixate_shrink_passes(&mut self, passes: &mut [ShrinkPass<'a>]) {
        const MAX_FAILURES: usize = 20;
        let mut any_ran = true;
        while any_ran {
            any_ran = false;
            // Try the cleanup pass once at the start of each loop —
            // mirrors Hypothesis's `can_discard = self.remove_discarded()`.
            let mut can_discard = self.remove_discarded();
            let mut reorder_keys: Vec<i32> = vec![0; passes.len()];
            for (idx, sp) in passes.iter_mut().enumerate() {
                if can_discard {
                    can_discard = self.remove_discarded();
                }
                let before_nodes_len = self.current_nodes.len();
                let before_key = sort_key(&self.current_nodes);

                // Without a `Chooser` integration, each pass is
                // deterministic — running it again with the shrink target
                // unchanged would simply repeat the same work.  We keep
                // calling the pass while it strictly improves and stop
                // as soon as it produces no change.  MAX_FAILURES is
                // retained as an upper bound for safety.
                for _ in 0..MAX_FAILURES {
                    sp.calls += 1;
                    let prev_key = sort_key(&self.current_nodes);
                    (sp.run)(self);
                    let now_key = sort_key(&self.current_nodes);
                    if now_key < prev_key {
                        sp.shrinks += 1;
                        if self.current_nodes.len() < before_nodes_len {
                            sp.deletions += 1;
                        }
                        any_ran = true;
                    } else {
                        break;
                    }
                }

                reorder_keys[idx] = if self.current_nodes.len() < before_nodes_len {
                    -1
                } else if sort_key(&self.current_nodes) < before_key {
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
            indexed.sort();
            // Apply the permutation in place.  Each pass moves once; we
            // use a temporary swap so the borrow checker stays happy.
            let permutation: Vec<usize> = indexed.iter().map(|(_, i)| *i).collect();
            let mut new_order: Vec<Option<ShrinkPass<'a>>> =
                (0..passes.len()).map(|_| None).collect();
            for (dest, &src) in permutation.iter().enumerate() {
                new_order[dest] = Some(std::mem::replace(
                    &mut passes[src],
                    // Temporarily fill with a noop placeholder.
                    ShrinkPass::new("__placeholder__", Box::new(|_| {})),
                ));
            }
            for (dest, slot) in new_order.into_iter().enumerate() {
                passes[dest] = slot.expect("permutation fills every slot");
            }
        }
    }

    /// Read-only access to per-pass stats (mainly for tests).
    #[allow(dead_code)]
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

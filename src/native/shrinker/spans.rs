//! Span-aware shrink passes.
//!
//! These passes consult [`Shrinker::current_spans`] (made available by
//! Step 1 of the parity port) to operate on structured sub-sequences of
//! the choice list rather than individual nodes.
//!
//! Hypothesis source: `hypothesis/internal/conjecture/shrinker.py`.

use super::Shrinker;

impl<'a> Shrinker<'a> {
    /// Delete every contiguous non-overlapping discarded span in one pass.
    ///
    /// Port of `shrinker.py:1290-1330` (`remove_discarded`).  Useful for
    /// rejection-sampling data left behind by filtered strategies — that
    /// whole region can usually be cut in a single attempt rather than
    /// element-by-element.
    ///
    /// Returns `true` if either (a) there was nothing to discard, or
    /// (b) the deletion attempts succeeded.  Returns `false` when the
    /// shrinker has discarded data that can't be removed (a follow-up
    /// pass shouldn't try this work again on the same target).
    // Wired into `shrink()` by Step 12 of the parity port; tests already
    // exercise this API.
    #[allow(dead_code)]
    pub(crate) fn remove_discarded(&mut self) -> bool {
        loop {
            // Gather the outermost discarded spans in source order.  A span
            // nested inside an already-collected discarded region is skipped
            // because the outer deletion subsumes it.
            let mut discarded: Vec<(usize, usize)> = Vec::new();
            for span in self.current_spans.iter() {
                if span.end > span.start
                    && span.discarded
                    && discarded
                        .last()
                        .is_none_or(|&(_, prev_end)| span.start >= prev_end)
                {
                    discarded.push((span.start, span.end));
                }
            }
            if discarded.is_empty() {
                return true;
            }

            let mut attempt = self.current_nodes.clone();
            for &(u, v) in discarded.iter().rev() {
                attempt.drain(u..v);
            }

            if !self.consider(&attempt) {
                return false;
            }
            // If `consider` accepted but the actual run produced no new
            // discards, the next loop iteration will find an empty
            // `discarded` and return.
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_remove_discarded_tests.rs"]
mod tests;

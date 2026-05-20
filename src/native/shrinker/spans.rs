//! Span-aware shrink passes.
//!
//! These passes consult [`Shrinker::current_spans`] (made available by
//! Step 1 of the parity port) to operate on structured sub-sequences of
//! the choice list rather than individual nodes.
//!
//! Hypothesis source: `hypothesis/internal/conjecture/shrinker.py`.

use super::{ShrinkRun, Shrinker};
use crate::native::core::sort_key;

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

    /// For every span in `current_spans`, try replacing its choices with their
    /// kind-simplest values.  Forced choices stay put.
    ///
    /// Port of `shrinker.py:1680-1708` (`try_trivial_spans`).  When the
    /// attempted replacement isn't interesting but the test run still
    /// produced a valid result, a second attempt is made using the
    /// realised span content from the run — this lets recursive
    /// structures whose simplest form is shape-dependent (e.g. an
    /// inner span that becomes shorter under simplest values) still
    /// converge.
    // Wired into `shrink()` by Step 12 / Step 18 of the parity port.
    #[allow(dead_code)]
    pub(crate) fn try_trivial_spans(&mut self) {
        let mut i = 0;
        while i < self.current_spans.len() {
            // Snapshot before any mutation so we can detect whether the
            // first attempt improved the shrink target.
            let prev_key = sort_key(&self.current_nodes);
            let span = self.current_spans[i].clone();
            if span.end > self.current_nodes.len() {
                i += 1;
                continue;
            }

            let already_trivial = self.current_spans.trivial(i, &self.current_nodes);
            if already_trivial {
                i += 1;
                continue;
            }

            let mut attempt: Vec<_> = self.current_nodes.clone();
            for node in attempt[span.start..span.end].iter_mut() {
                if node.was_forced {
                    continue;
                }
                let simplest = node.kind.simplest();
                if node.value != simplest {
                    *node = node.with_value(simplest);
                }
            }

            // Manually invoke the closure so we keep hold of the actual
            // realised nodes and spans even when the attempt isn't an
            // improvement — Hypothesis uses this to retry with the
            // realised span content (see line 1705-1708).
            let (is_interesting, actual_nodes, actual_spans) =
                (self.test_fn)(ShrinkRun::Full(&attempt));
            if is_interesting && sort_key(&actual_nodes) < sort_key(&self.current_nodes) {
                self.accept_improvement(actual_nodes, actual_spans);
                i += 1;
                continue;
            }

            // First attempt didn't improve.  If the run produced a valid
            // (or interesting-but-not-smaller) result that still records
            // a span at this index, splice its realised content back into
            // the original sequence and try once more.
            if sort_key(&self.current_nodes) == prev_key
                && let Some(new_span) = actual_spans.get(i)
                && new_span.start <= new_span.end
                && new_span.end <= actual_nodes.len()
            {
                let mut spliced = self.current_nodes[..span.start].to_vec();
                spliced.extend_from_slice(&actual_nodes[new_span.start..new_span.end]);
                spliced.extend_from_slice(&self.current_nodes[span.end..]);
                self.consider(&spliced);
            }
            i += 1;
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_remove_discarded_tests.rs"]
mod remove_discarded_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_try_trivial_spans_tests.rs"]
mod try_trivial_spans_tests;

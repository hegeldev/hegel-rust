//! Span-aware shrink passes.
//!
//! These passes consult [`Shrinker::current_spans`] to operate on
//! structured sub-sequences of the choice list rather than individual
//! nodes.

use super::ordering::shrink_ordering;
use super::{ShrinkResult, ShrinkRun, Shrinker};
use crate::control::{hegel_internal_debug_assert, hegel_internal_debug_assert_eq};
use crate::native::core::sort_key;

impl<'a> Shrinker<'a> {
    /// Delete every contiguous non-overlapping discarded span in one pass.
    ///
    /// Useful for rejection-sampling data left behind by filtered
    /// strategies — that whole region can usually be cut in a single
    /// attempt rather than element-by-element.
    ///
    /// Returns `true` if either (a) there was nothing to discard, or
    /// (b) the deletion attempts succeeded.  Returns `false` when the
    /// shrinker has discarded data that can't be removed (a follow-up
    /// pass shouldn't try this work again on the same target).
    pub(crate) fn remove_discarded(&mut self) -> ShrinkResult<bool> {
        loop {
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
                return Ok(true);
            }

            let mut attempt = self.current_nodes.clone();
            for &(u, v) in discarded.iter().rev() {
                attempt.drain(u..v);
            }

            if !self.consider(&attempt)? {
                return Ok(false);
            }
        }
    }

    /// For every span in `current_spans`, try replacing its choices with their
    /// kind-simplest values. Forced choices stay put.
    ///
    /// When the attempted replacement isn't interesting but the test run
    /// still produced a valid result, a second attempt is made using the
    /// realised span content from the run — this lets recursive
    /// structures whose simplest form is shape-dependent (e.g. an
    /// inner span that becomes shorter under simplest values) still
    /// converge.
    pub(crate) fn try_trivial_spans(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_spans.len() {
            let epoch_before = self.improvements;
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

            let (is_interesting, actual_nodes, actual_spans) =
                self.run_test_fn(ShrinkRun::Full(&attempt))?;
            if is_interesting && sort_key(&actual_nodes) < sort_key(&self.current_nodes) {
                self.accept_improvement(actual_nodes, actual_spans);
                i += 1;
                continue;
            }

            if self.improvements == epoch_before {
                if let Some(new_span) = actual_spans.get(i) {
                    if new_span.start <= new_span.end && new_span.end <= actual_nodes.len() {
                        let mut spliced = self.current_nodes[..span.start].to_vec();
                        spliced.extend_from_slice(&actual_nodes[new_span.start..new_span.end]);
                        spliced.extend_from_slice(&self.current_nodes[span.end..]);
                        self.consider(&spliced)?;
                    }
                }
            }
            i += 1;
        }
        Ok(())
    }

    /// Replace each span with one of its same-label descendants.
    ///
    /// This is the pass that lets recursive strategies collapse a tree
    /// onto one of its subtrees in a single step, instead of having to
    /// chew through each layer individually.
    ///
    /// For every pair `(ancestor, descendant)` of same-label spans where
    /// the descendant is strictly contained in the ancestor and is
    /// strictly shorter, we splice the descendant's nodes in place of the
    /// ancestor's and ask the predicate whether that's still interesting.
    pub(crate) fn pass_to_descendant(&mut self) -> ShrinkResult<()> {
        let spans: Vec<(usize, usize, String)> = self
            .current_spans
            .iter()
            .map(|s| (s.start, s.end, s.label.clone()))
            .collect();

        let mut by_label: std::collections::BTreeMap<&str, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (idx, (_, _, label)) in spans.iter().enumerate() {
            by_label.entry(label.as_str()).or_default().push(idx);
        }

        for (_label, indices) in by_label {
            if indices.len() < 2 {
                continue;
            }
            for ai in 0..indices.len() {
                let ancestor_idx = indices[ai];
                let (a_start, a_end, _) = spans[ancestor_idx].clone();
                let ancestor_len = a_end.saturating_sub(a_start);
                if ancestor_len == 0 {
                    continue;
                }
                for &descendant_idx in &indices[ai + 1..] {
                    let (d_start, d_end, _) = spans[descendant_idx].clone();
                    if d_start >= a_end {
                        break;
                    }
                    let descendant_len = d_end.saturating_sub(d_start);
                    if descendant_len == 0 || descendant_len >= ancestor_len {
                        continue;
                    }
                    if a_end > self.current_nodes.len() {
                        continue;
                    }
                    hegel_internal_debug_assert!(d_start >= a_start && d_end <= a_end);
                    let mut attempt = self.current_nodes[..a_start].to_vec();
                    attempt.extend_from_slice(&self.current_nodes[d_start..d_end]);
                    attempt.extend_from_slice(&self.current_nodes[a_end..]);
                    self.consider(&attempt)?;
                }
            }
        }
        Ok(())
    }

    /// Reorder same-label sibling spans into a more-sorted permutation.
    ///
    /// For each parent span, for each label that appears more than once
    /// among its direct children, run an [`shrink_ordering`] over the
    /// children indices, keyed by the sort key of the child's realised
    /// node slice.
    ///
    /// Permutation by index keeps the multiset of children intact; the
    /// only thing that changes is *which* slice ends up at which start
    /// position. This is the pass that ensures `test_not_equal(x, y)`
    /// collapses to a canonical `(x="", y="0")` rather than the
    /// symmetric alternative.
    pub(crate) fn reorder_spans(&mut self) -> ShrinkResult<()> {
        let parents: Vec<Option<usize>> = {
            let mut seen: std::collections::BTreeSet<Option<usize>> =
                std::collections::BTreeSet::new();
            for span in self.current_spans.iter() {
                seen.insert(span.parent);
            }
            seen.into_iter().collect()
        };

        for parent in parents {
            let mut by_label: std::collections::BTreeMap<String, Vec<usize>> =
                std::collections::BTreeMap::new();
            for (idx, span) in self.current_spans.iter().enumerate() {
                if span.parent == parent {
                    by_label.entry(span.label.clone()).or_default().push(idx);
                }
            }

            for (_label, child_indices) in by_label {
                if child_indices.len() <= 1 {
                    continue;
                }
                let endpoints: Vec<(usize, usize)> = child_indices
                    .iter()
                    .map(|&i| {
                        let s = &self.current_spans[i];
                        (s.start, s.end)
                    })
                    .collect();
                let nodes_len = self.current_nodes.len();
                if endpoints.iter().any(|&(_, e)| e > nodes_len) {
                    continue;
                }
                hegel_internal_debug_assert!({
                    let mut sorted_eps = endpoints.clone();
                    sorted_eps.sort();
                    sorted_eps.windows(2).all(|w| w[0].1 <= w[1].0)
                });

                let n = child_indices.len();
                let snapshot_nodes = self.current_nodes.clone();

                let cached_keys: Vec<crate::native::core::NodesSortKey<'_>> = endpoints
                    .iter()
                    .map(|&(s, e)| sort_key(&snapshot_nodes[s..e]))
                    .collect();

                shrink_ordering::<crate::native::core::NodesSortKey<'_>, _, _>(
                    n,
                    |i| cached_keys[i],
                    |permutation| -> ShrinkResult<bool> {
                        hegel_internal_debug_assert_eq!(permutation.len(), n);
                        let mut attempt: Vec<_> = Vec::with_capacity(snapshot_nodes.len());
                        attempt.extend_from_slice(&snapshot_nodes[..endpoints[0].0]);
                        for (k, &(_, target_end)) in endpoints.iter().enumerate() {
                            let src_idx = permutation[k];
                            let (src_start, src_end) = endpoints[src_idx];
                            attempt.extend_from_slice(&snapshot_nodes[src_start..src_end]);
                            if k + 1 < endpoints.len() {
                                attempt.extend_from_slice(
                                    &snapshot_nodes[target_end..endpoints[k + 1].0],
                                );
                            } else {
                                attempt.extend_from_slice(&snapshot_nodes[target_end..]);
                            }
                        }
                        self.consider(&attempt)
                    },
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_remove_discarded_tests.rs"]
mod remove_discarded_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_pass_to_descendant_tests.rs"]
mod pass_to_descendant_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_reorder_spans_tests.rs"]
mod reorder_spans_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_try_trivial_spans_tests.rs"]
mod try_trivial_spans_tests;

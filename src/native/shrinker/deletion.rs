// Deletion-based shrink passes: delete_chunks, bind_deletion, try_replace_with_deletion,
// minimize_individual_choices.

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceValue, sort_key};

use super::{Shrinker, bin_search_down};

impl<'a> Shrinker<'a> {
    /// Try deleting chunks of choices from the sequence.
    ///
    /// Longer chunks allow deleting composite elements (e.g. a list element
    /// requires deleting both the "include?" choice and the element itself).
    /// Iterates backwards since later choices tend to depend on earlier ones.
    pub(super) fn delete_chunks(&mut self) {
        let mut k: usize = 8;
        while k > 0 {
            let mut i = self.current_nodes.len().saturating_sub(k + 1);
            loop {
                // Only reached when a prior iteration shrank current_nodes to
                // empty; with usize i we can't go negative, so we bail.
                if i >= self.current_nodes.len() {
                    break; // nocov — sequences that shrink to empty mid-pass are rare
                }
                let end = (i + k).min(self.current_nodes.len());
                let mut attempt: Vec<_> = self.current_nodes[..i].to_vec();
                attempt.extend_from_slice(&self.current_nodes[end..]);
                assert!(attempt.len() < self.current_nodes.len());

                if !self.consider(&attempt) && i > 0 {
                    // Try decrementing the preceding choice (helps with
                    // collection length counters).
                    let prev = &attempt[i - 1];
                    let decremented = match (&prev.kind, &prev.value) {
                        (ChoiceKind::Integer(ic), ChoiceValue::Integer(v))
                            if *v != ic.simplest() =>
                        {
                            Some(ChoiceValue::Integer(v - 1))
                        }
                        (ChoiceKind::Boolean(_), ChoiceValue::Boolean(true)) => {
                            Some(ChoiceValue::Boolean(false))
                        }
                        _ => None,
                    };
                    if let Some(new_value) = decremented {
                        let mut modified = attempt.clone();
                        modified[i - 1] = modified[i - 1].with_value(new_value);
                        self.consider(&modified);
                    }
                }
                if i == 0 {
                    break;
                }
                i -= 1;
            }
            k -= 1;
        }
    }

    /// Port of Hypothesis's `bind_deletion`.
    ///
    /// When a value controls the length of a downstream sequence (e.g.
    /// via flat_map), reducing that value may shorten the test case without
    /// keeping the result interesting. This pass detects that situation and
    /// tries deleting the now-excess choices to recover an interesting result.
    pub(super) fn bind_deletion(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();

            // Only process integer nodes — these control sequence lengths.
            let (current_val, ic) = match (&node.kind, &node.value) {
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => (*v, ic.clone()),
                _ => {
                    i += 1;
                    continue;
                }
            };

            let simplest = ic.simplest();
            if current_val == simplest {
                i += 1;
                continue;
            }

            let expected_len = self.current_nodes.len();

            // Binary-search smaller integer values; for each candidate, try
            // replace-with-deletion.
            let changed = bin_search_down(simplest, current_val, &mut |v| {
                self.try_replace_with_deletion(i, ChoiceValue::Integer(v), expected_len)
            });
            let _ = changed;

            i += 1;
        }
    }

    /// Try replacing the value at `idx`. If the result is interesting, done.
    /// If the result is valid but used fewer nodes than `expected_len`, try
    /// deleting regions after `idx` to recover an interesting result.
    pub(super) fn try_replace_with_deletion(
        &mut self,
        idx: usize,
        value: ChoiceValue,
        expected_len: usize,
    ) -> bool {
        // First try a straight replace. consider() already calls test_fn and
        // records the interesting case; we'd just duplicate work by retrying.
        if self.replace(&HashMap::from([(idx, value.clone())])) {
            return true; // nocov — early-success path; deletion fallback below covers the common case
        }

        // The replace couldn't narrow the result directly. Re-run the test to
        // see how many nodes it consumed — if fewer than expected, the trailing
        // choices may be deletable. replace() asserted idx < current_nodes.len()
        // and, since it returned false, did not mutate current_nodes, so idx is
        // still in range here.
        let mut attempt = self.current_nodes.clone();
        attempt[idx] = attempt[idx].with_value(value);

        let (_, actual_nodes, _) = (self.test_fn)(super::ShrinkRun::Full(&attempt));
        if actual_nodes.len() >= expected_len {
            return false;
        }

        // The test used fewer nodes. Try deleting regions after idx.
        let k = expected_len - actual_nodes.len();
        for size in (1..=k).rev() {
            let start = attempt.len().saturating_sub(size);
            if start <= idx {
                continue;
            }
            for j in (idx + 1..=start).rev() {
                let mut candidate = attempt[..j].to_vec();
                candidate.extend_from_slice(&attempt[j + size..]);
                if self.consider(&candidate) {
                    return true;
                }
            }
        }
        false
    }

    /// Per-node minimization with size-dependency deletion fallback.
    ///
    /// Port of `shrinker.py:1710-1808` (`minimize_individual_choices`).  For
    /// each non-forced, non-simplest integer node, lowering it by one
    /// often shortens the realised sequence because the integer
    /// controlled a downstream collection size (the
    /// `lists(integers(min_size=n))` flat-map pattern).  When that
    /// happens but the lowered candidate isn't directly interesting, we
    /// try splicing out spans / nodes after the integer to recover an
    /// interesting (shorter) candidate.
    ///
    /// Non-integer nodes are deferred to the existing per-type passes
    /// — the unified Hypothesis driver only adds value for integers,
    /// per its own comment (`shrinker.py:1748-1756`).
    // Wired into `shrink()` by Step 12 / Step 18.
    #[allow(dead_code)]
    pub(crate) fn minimize_individual_choices(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();
            if node.was_forced {
                i += 1;
                continue;
            }
            let (ic, current_val) = match (&node.kind, &node.value) {
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => (ic.clone(), *v),
                _ => {
                    i += 1;
                    continue;
                }
            };
            let simplest = ic.simplest();
            if current_val == simplest {
                i += 1;
                continue;
            }

            // Phase 1: regular shrink target — bin_search the integer
            // toward simplest, accepting any candidate that consider()
            // approves.
            let initial_key = sort_key(&self.current_nodes);
            bin_search_down(simplest, current_val, &mut |v| {
                self.replace(&HashMap::from([(i, ChoiceValue::Integer(v))]))
            });
            if sort_key(&self.current_nodes) < initial_key {
                // Made progress; move on.
                i += 1;
                continue;
            }

            // Phase 2: size-dependency fallback.  Lower by exactly one,
            // peek at the realised actual_nodes — if shorter than the
            // current sequence, try deletions to recover validity.
            //
            // Re-read current_nodes since we may have inserted / removed
            // entries above.
            if i >= self.current_nodes.len() {
                break;
            }
            let original_len = self.current_nodes.len();
            // Lower-by-one in the direction of simplest.
            let towards = if current_val > simplest {
                current_val - 1
            } else {
                current_val + 1
            };
            let mut lowered = self.current_nodes.clone();
            lowered[i] = lowered[i].with_value(ChoiceValue::Integer(towards));

            let (_, actual_nodes, actual_spans) = (self.test_fn)(super::ShrinkRun::Full(&lowered));
            if actual_nodes.len() >= original_len || actual_nodes.len() <= i + 1 {
                i += 1;
                continue;
            }

            // Try deleting each span that starts after i.
            let mut shrank = false;
            for span_idx in 0..actual_spans.len() {
                let span = &actual_spans[span_idx];
                if span.start <= i || span.end > actual_nodes.len() || span.end <= span.start {
                    continue;
                }
                let mut candidate: Vec<_> = actual_nodes[..span.start].to_vec();
                candidate.extend_from_slice(&actual_nodes[span.end..]);
                if self.consider(&candidate) && sort_key(&self.current_nodes) < initial_key {
                    shrank = true;
                    break;
                }
            }

            if !shrank {
                // Try deleting individual nodes after i.
                for j in i + 1..actual_nodes.len() {
                    let mut candidate: Vec<_> = actual_nodes[..j].to_vec();
                    candidate.extend_from_slice(&actual_nodes[j + 1..]);
                    if self.consider(&candidate) && sort_key(&self.current_nodes) < initial_key {
                        break;
                    }
                }
            }
            i += 1;
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_minimize_individual_choices_tests.rs"]
mod minimize_individual_choices_tests;

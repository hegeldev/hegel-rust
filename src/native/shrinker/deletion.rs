// Deletion-based shrink passes: delete_chunks, bind_deletion, try_replace_with_deletion,
// minimize_individual_choices, node_program.

use std::collections::HashMap;

use crate::native::bignum::BigInt;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue};

use super::{Shrinker, bin_search_down_big, find_integer};

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
                    break;
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
                            ic.value_from_bigint(&(v.to_bigint() - 1))
                                .map(ChoiceValue::Integer)
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
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => (v.to_bigint(), ic.clone()),
                _ => {
                    i += 1;
                    continue;
                }
            };

            let simplest = ic.simplest_bigint();
            if current_val == simplest {
                i += 1;
                continue;
            }

            let expected_len = self.current_nodes.len();

            // Binary-search smaller integer values; for each candidate, try
            // replace-with-deletion.
            let changed = bin_search_down_big(simplest, current_val, &mut |v| match self
                .int_replacement(i, v)
            {
                Some(value) => self.try_replace_with_deletion(i, value, expected_len),
                None => false,
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
            return true;
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
    /// For each non-forced, non-simplest integer node, lowering it by
    /// one often shortens the realised sequence because the integer
    /// controlled a downstream collection size (the
    /// `lists(integers(min_size=n))` flat-map pattern). When that
    /// happens but the lowered candidate isn't directly interesting, we
    /// try splicing out spans / nodes after the integer to recover an
    /// interesting (shorter) candidate.
    ///
    /// Non-integer nodes are deferred to the existing per-type passes —
    /// the unified driver only adds value for integers.
    pub(crate) fn minimize_individual_choices(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();
            if node.was_forced {
                i += 1;
                continue;
            }
            let (ic, current_val) = match (&node.kind, &node.value) {
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => (ic.clone(), v.to_bigint()),
                _ => {
                    i += 1;
                    continue;
                }
            };
            let simplest = ic.simplest_bigint();
            if current_val == simplest {
                i += 1;
                continue;
            }

            // Phase 1: regular shrink target — bin_search the integer
            // toward simplest, accepting any candidate that consider()
            // approves.
            //
            // `self.improvements` only bumps on a strictly-smaller accept
            // (see `accept_improvement`), so its delta is "did we shrink?"
            // without needing a snapshot of the prior sort_key.
            let epoch_phase1 = self.improvements;
            bin_search_down_big(simplest.clone(), current_val.clone(), &mut |v| {
                self.replace_int(i, v)
            });
            if self.improvements > epoch_phase1 {
                // Made progress; move on.
                i += 1;
                continue;
            }

            // Phase 2: lower by exactly one, peek at the realised
            // actual_nodes for misalignment + size-dependency.
            //
            // Re-read current_nodes since `bin_search_down` may have
            // accepted candidates that shortened the sequence.  The
            // outer `while i < self.current_nodes.len()` guard means
            // `i` is still in range when we reach this point.
            debug_assert!(i < self.current_nodes.len());
            let original_len = self.current_nodes.len();
            // Lower-by-one in the direction of simplest.
            let towards = if current_val > simplest {
                &current_val - BigInt::from(1)
            } else {
                &current_val + BigInt::from(1)
            };
            let Some(towards_value) = self.int_replacement(i, &towards) else {
                i += 1;
                continue;
            };
            let mut lowered = self.current_nodes.clone();
            lowered[i] = lowered[i].with_value(towards_value);

            let (_, actual_nodes, actual_spans) = (self.test_fn)(super::ShrinkRun::Full(&lowered));

            // Misalignment-truncation retry. Even when the sequence
            // length didn't change, the realised draw of a string/bytes
            // node at `k > i` may be shorter than the candidate (the
            // test re-drew that node with a smaller min_size dictated
            // by the lowered integer). Retry with the candidate
            // truncated to the realised length.
            //
            // Runs independent of the size-dependency / deletion
            // fallback below.
            let mut misalignment_handled = false;
            for k in (i + 1)..lowered.len().min(actual_nodes.len()) {
                let cand = &lowered[k];
                let actual_val = &actual_nodes[k].value;
                let retry_value = match (&cand.value, actual_val) {
                    (ChoiceValue::String(c), ChoiceValue::String(a)) if c.len() > a.len() => {
                        Some(ChoiceValue::String(c[..a.len()].to_vec()))
                    }
                    (ChoiceValue::Bytes(c), ChoiceValue::Bytes(a)) if c.len() > a.len() => {
                        Some(ChoiceValue::Bytes(c[..a.len()].to_vec()))
                    }
                    _ => None,
                };
                if let Some(rv) = retry_value {
                    let mut candidate = lowered.clone();
                    candidate[k] = candidate[k].with_value(rv);
                    if self.consider(&candidate) && self.improvements > epoch_phase1 {
                        misalignment_handled = true;
                        break;
                    }
                }
            }
            if misalignment_handled {
                i += 1;
                continue;
            }

            // Size-dependency fallback only applies when the realised
            // run truncated the trailing sequence.
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
                if self.consider(&candidate) && self.improvements > epoch_phase1 {
                    shrank = true;
                    break;
                }
            }

            if !shrank {
                // Try deleting individual nodes after i.
                for j in i + 1..actual_nodes.len() {
                    let mut candidate: Vec<_> = actual_nodes[..j].to_vec();
                    candidate.extend_from_slice(&actual_nodes[j + 1..]);
                    if self.consider(&candidate) && self.improvements > epoch_phase1 {
                        break;
                    }
                }
            }
            i += 1;
        }
    }

    /// Adaptively delete `n` consecutive nodes at every position, with
    /// `find_integer` powering the repeat-count probe.
    ///
    /// Walks each starting index `i`, tries deleting `[i, i+n)`; if that
    /// lands, walks left to find the start of a contiguous deletable
    /// region and then `find_integer`s the largest repeat count that
    /// still keeps the candidate interesting.
    ///
    /// Each find_integer probe runs against a fixed snapshot taken when
    /// the probe started, so the repeat semantics are stable regardless
    /// of intervening shrink-target updates.
    ///
    /// This is `delete_chunks` rewritten as five named passes — one for
    /// each `n in 1..=5` — and gives O(log k) test-function calls when
    /// a long deletable region exists, vs. the linear O(k) of the legacy
    /// loop. `delete_chunks` is kept alongside as the native fallback.
    pub(crate) fn node_program(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let mut i = 0;
        while i + n <= self.current_nodes.len() {
            // First try a single application at i against the current
            // snapshot.
            let snapshot = self.current_nodes.clone();
            if !self.run_node_program(&snapshot, i, n, 1) {
                i += 1;
                continue;
            }
            // Walk left as far as the program still applies, against a
            // fresh snapshot (the success may have shrunk the
            // sequence).
            let snapshot = self.current_nodes.clone();
            let starting = i.min(snapshot.len());
            let left_offset = find_integer(|k| {
                if k * n > starting {
                    return false;
                }
                let pos = starting - k * n;
                self.run_node_program(&snapshot, pos, n, 1)
            });
            let start = starting.saturating_sub(left_offset * n);

            // Adaptively grow the repeat count from `start`, again
            // against a fresh snapshot.
            let snapshot = self.current_nodes.clone();
            find_integer(|k| self.run_node_program(&snapshot, start, n, k));

            // Advance past the region we just consumed.  Moving forward
            // by `n` guarantees progress on the next outer iteration.
            i = start.saturating_add(n);
        }
    }

    /// Apply the "delete n consecutive nodes" program `repeats` times at
    /// position `i` of `original`, then ask `consider` whether the
    /// resulting candidate is still interesting *and* an improvement.
    ///
    /// The deletion always operates on the supplied `original` snapshot,
    /// so repeat counts are well-defined regardless of intermediate
    /// shrink-target updates.
    fn run_node_program(
        &mut self,
        original: &[ChoiceNode],
        i: usize,
        program_len: usize,
        repeats: usize,
    ) -> bool {
        // `find_integer` starts probing at `n = 1`, so callers never
        // ask for zero-repeat applications.  A debug_assert documents
        // the precondition.
        debug_assert!(repeats > 0);
        let total_delete = program_len.saturating_mul(repeats);
        if i + total_delete > original.len() {
            return false;
        }
        let mut attempt = original[..i].to_vec();
        attempt.extend_from_slice(&original[i + total_delete..]);
        let epoch = self.improvements;
        self.consider(&attempt) && self.improvements > epoch
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_minimize_individual_choices_tests.rs"]
mod minimize_individual_choices_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_node_program_tests.rs"]
mod node_program_tests;

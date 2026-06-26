use std::collections::HashMap;

use crate::native::bignum::BigInt;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue};

use super::{ShrinkResult, Shrinker, bin_search_down_big_r, find_integer_r};
use crate::control::{hegel_internal_assert, hegel_internal_debug_assert};

impl<'a> Shrinker<'a> {
    /// Try deleting chunks of choices from the sequence.
    ///
    /// Longer chunks allow deleting composite elements (e.g. a list element
    /// requires deleting both the "include?" choice and the element itself).
    /// Iterates backwards since later choices tend to depend on earlier ones.
    pub(super) fn delete_chunks(&mut self) -> ShrinkResult<()> {
        let mut k: usize = 8;
        while k > 0 {
            let mut i = self.current_nodes.len().saturating_sub(k + 1);
            loop {
                if i >= self.current_nodes.len() {
                    break;
                }
                let end = (i + k).min(self.current_nodes.len());
                let mut attempt: Vec<_> = self.current_nodes[..i].to_vec();
                attempt.extend_from_slice(&self.current_nodes[end..]);
                hegel_internal_assert!(attempt.len() < self.current_nodes.len());

                if !self.consider(&attempt)? && i > 0 {
                    let prev = &attempt[i - 1];
                    let decremented = match (prev.kind.as_ref(), &prev.value) {
                        (ChoiceKind::Integer(ic), ChoiceValue::Integer(v))
                            if *v != ic.simplest() =>
                        {
                            ic.value_from_bigint(&(v.clone() - 1))
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
                        self.consider(&modified)?;
                    }
                }
                if i == 0 {
                    break;
                }
                i -= 1;
            }
            k -= 1;
        }
        Ok(())
    }

    /// When a value controls the length of a downstream sequence (e.g.
    /// via flat_map), reducing that value may shorten the test case without
    /// keeping the result interesting. This pass detects that situation and
    /// tries deleting the now-excess choices to recover an interesting result.
    pub(super) fn bind_deletion(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();

            let (current_val, ic) = match (node.kind.as_ref(), &node.value) {
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => (v.clone(), ic.clone()),
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

            bin_search_down_big_r(simplest, current_val, &mut |v| {
                let value = self.int_replacement(i, v);
                self.try_replace_with_deletion(i, value, expected_len)
            })?;

            i += 1;
        }
        Ok(())
    }

    /// Try replacing the value at `idx`. If the result is interesting, done.
    /// If the result is valid but used fewer nodes than `expected_len`, try
    /// deleting regions after `idx` to recover an interesting result.
    pub(super) fn try_replace_with_deletion(
        &mut self,
        idx: usize,
        value: ChoiceValue,
        expected_len: usize,
    ) -> ShrinkResult<bool> {
        if self.replace(&HashMap::from([(idx, value.clone())]))? {
            return Ok(true);
        }

        let mut attempt = self.current_nodes.clone();
        attempt[idx] = attempt[idx].with_value(value);

        let (_, actual_nodes, _) = self.run_test_fn(super::ShrinkRun::Full(&attempt))?;
        if actual_nodes.len() >= expected_len {
            return Ok(false);
        }

        let k = expected_len - actual_nodes.len();
        for size in (1..=k).rev() {
            let start = attempt.len().saturating_sub(size);
            if start <= idx {
                continue;
            }
            for j in (idx + 1..=start).rev() {
                let mut candidate = attempt[..j].to_vec();
                candidate.extend_from_slice(&attempt[j + size..]);
                if self.consider(&candidate)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
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
    pub(crate) fn minimize_individual_choices(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();
            if node.was_forced {
                i += 1;
                continue;
            }
            let (ic, current_val) = match (node.kind.as_ref(), &node.value) {
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => (ic.clone(), v.clone()),
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

            let epoch_phase1 = self.improvements;
            bin_search_down_big_r(simplest.clone(), current_val.clone(), &mut |v| {
                self.replace_int(i, v)
            })?;
            if self.improvements > epoch_phase1 {
                i += 1;
                continue;
            }

            hegel_internal_debug_assert!(i < self.current_nodes.len());
            let original_len = self.current_nodes.len();
            let towards = if current_val > simplest {
                &current_val - BigInt::from(1)
            } else {
                &current_val + BigInt::from(1)
            };
            let towards_value = self.int_replacement(i, &towards);
            let mut lowered = self.current_nodes.clone();
            lowered[i] = lowered[i].with_value(towards_value);

            let (_, actual_nodes, actual_spans) =
                self.run_test_fn(super::ShrinkRun::Full(&lowered))?;

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
                    if self.consider(&candidate)? && self.improvements > epoch_phase1 {
                        misalignment_handled = true;
                        break;
                    }
                }
            }
            if misalignment_handled {
                i += 1;
                continue;
            }

            if actual_nodes.len() >= original_len || actual_nodes.len() <= i + 1 {
                i += 1;
                continue;
            }

            let mut shrank = false;
            for span_idx in 0..actual_spans.len() {
                let span = &actual_spans[span_idx];
                if span.start <= i || span.end > actual_nodes.len() || span.end <= span.start {
                    continue;
                }
                let mut candidate: Vec<_> = actual_nodes[..span.start].to_vec();
                candidate.extend_from_slice(&actual_nodes[span.end..]);
                if self.consider(&candidate)? && self.improvements > epoch_phase1 {
                    shrank = true;
                    break;
                }
            }

            if !shrank {
                for j in i + 1..actual_nodes.len() {
                    let mut candidate: Vec<_> = actual_nodes[..j].to_vec();
                    candidate.extend_from_slice(&actual_nodes[j + 1..]);
                    if self.consider(&candidate)? && self.improvements > epoch_phase1 {
                        break;
                    }
                }
            }
            i += 1;
        }
        Ok(())
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
    pub(crate) fn node_program(&mut self, n: usize) -> ShrinkResult<()> {
        if n == 0 {
            return Ok(());
        }
        let mut i = 0;
        while i + n <= self.current_nodes.len() {
            let snapshot = self.current_nodes.clone();
            if !self.run_node_program(&snapshot, i, n, 1)? {
                i += 1;
                continue;
            }
            let snapshot = self.current_nodes.clone();
            let starting = i.min(snapshot.len());
            let left_offset = find_integer_r(|k| {
                if k * n > starting {
                    return Ok(false);
                }
                let pos = starting - k * n;
                self.run_node_program(&snapshot, pos, n, 1)
            })?;
            let start = starting.saturating_sub(left_offset * n);

            let snapshot = self.current_nodes.clone();
            find_integer_r(|k| self.run_node_program(&snapshot, start, n, k))?;

            i = start.saturating_add(n);
        }
        Ok(())
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
    ) -> ShrinkResult<bool> {
        hegel_internal_debug_assert!(repeats > 0);
        let total_delete = program_len.saturating_mul(repeats);
        if i + total_delete > original.len() {
            return Ok(false);
        }
        let mut attempt = original[..i].to_vec();
        attempt.extend_from_slice(&original[i + total_delete..]);
        let epoch = self.improvements;
        Ok(self.consider(&attempt)? && self.improvements > epoch)
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_minimize_individual_choices_tests.rs"]
mod minimize_individual_choices_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_node_program_tests.rs"]
mod node_program_tests;

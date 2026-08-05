use crate::native::HashMap;

use crate::native::bignum::BigInt;
use crate::native::core::{ChoiceData, ChoiceNode, ChoiceValue};

use super::search::{BinSearchDownBig, FindInteger, SearchStep};
use super::{ShrinkResult, Shrinker};
use crate::control::{hegel_internal_assert, hegel_internal_debug_assert};

impl<'a> Shrinker<'a> {
    /// Try deleting chunks of choices from the sequence.
    ///
    /// Longer chunks allow deleting composite elements (e.g. a list element
    /// requires deleting both the "include?" choice and the element itself).
    /// Iterates backwards since later choices tend to depend on earlier ones.
    pub(super) async fn delete_chunks(&mut self) -> ShrinkResult<()> {
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

                if !self.consider(&attempt).await? && i > 0 {
                    let prev = &attempt[i - 1];
                    let decremented: Option<ChoiceData> = match &prev.data {
                        ChoiceData::Integer(ic, v) if *v != ic.simplest() => ic
                            .value_from_bigint(&(v.clone() - 1))
                            .map(|nv| ChoiceData::Integer(std::sync::Arc::clone(ic), nv)),
                        ChoiceData::Boolean(true) => Some(ChoiceData::Boolean(false)),
                        _ => None,
                    };
                    if let Some(new_data) = decremented {
                        let mut modified = attempt.clone();
                        modified[i - 1] = ChoiceNode::new(new_data, modified[i - 1].was_forced);
                        self.consider(&modified).await?;
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
    pub(super) async fn bind_deletion(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();

            let ChoiceData::Integer(ic, current_val) = &node.data else {
                i += 1;
                continue;
            };
            let current_val = current_val.clone();

            let simplest = ic.simplest();
            if current_val == simplest {
                i += 1;
                continue;
            }

            let expected_len = self.current_nodes.len();

            let mut search = BinSearchDownBig::new(simplest, current_val);
            while let Some(v) = search.probe() {
                let ok = self
                    .try_replace_with_deletion(i, ChoiceValue::Integer(v), expected_len)
                    .await?;
                search.record(ok);
            }

            i += 1;
        }
        Ok(())
    }

    /// Try replacing the value at `idx`. If the result is interesting, done.
    /// If the result is valid but used fewer nodes than `expected_len`, try
    /// deleting regions after `idx` to recover an interesting result.
    pub(super) async fn try_replace_with_deletion(
        &mut self,
        idx: usize,
        value: ChoiceValue,
        expected_len: usize,
    ) -> ShrinkResult<bool> {
        if self
            .replace(&HashMap::from_iter([(idx, value.clone())]))
            .await?
        {
            return Ok(true);
        }

        let mut attempt = self.current_nodes.clone();
        let Some(replaced) = attempt[idx].with_value(&value) else {
            return Ok(false);
        };
        attempt[idx] = replaced;

        let (_, actual_nodes, _) = self.run_test_fn(super::ShrinkRun::Full(&attempt)).await?;
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
                if self.consider(&candidate).await? {
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
    pub(crate) async fn minimize_individual_choices(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();
            if node.was_forced {
                i += 1;
                continue;
            }
            let ChoiceData::Integer(ic, current_val) = &node.data else {
                i += 1;
                continue;
            };
            let (ic, current_val) = (std::sync::Arc::clone(ic), current_val.clone());
            let simplest = ic.simplest();
            if current_val == simplest {
                i += 1;
                continue;
            }

            let epoch_phase1 = self.improvements;
            let mut search = BinSearchDownBig::new(simplest.clone(), current_val.clone());
            while let Some(v) = search.probe() {
                let ok = self.replace_int(i, &v).await?;
                search.record(ok);
            }
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
            let mut lowered = self.current_nodes.clone();
            lowered[i] = ChoiceNode::new(
                ChoiceData::Integer(std::sync::Arc::clone(&ic), towards),
                lowered[i].was_forced,
            );

            let (_, actual_nodes, actual_spans) =
                self.run_test_fn(super::ShrinkRun::Full(&lowered)).await?;

            let mut misalignment_handled = false;
            for k in (i + 1)..lowered.len().min(actual_nodes.len()) {
                let cand = &lowered[k];
                let retry_data: Option<ChoiceData> = match (&cand.data, &actual_nodes[k].data) {
                    (ChoiceData::String(sc, c), ChoiceData::String(_, a)) if c.len() > a.len() => {
                        Some(ChoiceData::String(sc.clone(), c[..a.len()].to_vec()))
                    }
                    (ChoiceData::Bytes(bc, c), ChoiceData::Bytes(_, a)) if c.len() > a.len() => {
                        Some(ChoiceData::Bytes(bc.clone(), c[..a.len()].to_vec()))
                    }
                    _ => None,
                };
                if let Some(rd) = retry_data {
                    let mut candidate = lowered.clone();
                    candidate[k] = ChoiceNode::new(rd, candidate[k].was_forced);
                    if self.consider(&candidate).await? && self.improvements > epoch_phase1 {
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
                if self.consider(&candidate).await? && self.improvements > epoch_phase1 {
                    shrank = true;
                    break;
                }
            }

            if !shrank {
                for j in i + 1..actual_nodes.len() {
                    let mut candidate: Vec<_> = actual_nodes[..j].to_vec();
                    candidate.extend_from_slice(&actual_nodes[j + 1..]);
                    if self.consider(&candidate).await? && self.improvements > epoch_phase1 {
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
    /// The leftward walk probes against the *live* shrink target, so each
    /// accepted step compounds (Hypothesis's `offset_left` does the same);
    /// probing a fixed snapshot instead would re-include the region the
    /// previous step just deleted, and the walk would stall after one step.
    /// The final repeat-count probe runs against a fixed snapshot so the
    /// repeat semantics are well-defined.
    ///
    /// This is `delete_chunks` rewritten as five named passes — one for
    /// each `n in 1..=5` — and gives O(log k) test-function calls when
    /// a long deletable region exists, vs. the linear O(k) of the legacy
    /// loop. `delete_chunks` is kept alongside as the native fallback.
    pub(crate) async fn node_program(&mut self, n: usize) -> ShrinkResult<()> {
        if n == 0 {
            return Ok(());
        }
        let mut i = 0;
        while i + n <= self.current_nodes.len() {
            if !self.run_node_program_live(i, n, 1).await? {
                i += 1;
                continue;
            }
            let starting = i;
            let mut search = FindInteger::new();
            let left_offset = loop {
                match search.step() {
                    SearchStep::Done(v) => break v,
                    SearchStep::Probe(k) => {
                        let ok = if k * n > starting {
                            false
                        } else {
                            self.run_node_program_live(starting - k * n, n, 1).await?
                        };
                        search.record(ok);
                    }
                }
            };
            let start = starting.saturating_sub(left_offset * n);

            let snapshot = self.current_nodes.clone();
            let mut search = FindInteger::new();
            while let Some(k) = search.probe() {
                let ok = self.run_node_program(&snapshot, start, n, k).await?;
                search.record(ok);
            }

            i = start.saturating_add(n);
        }
        Ok(())
    }

    /// [`Self::run_node_program`] against the current shrink target.
    async fn run_node_program_live(
        &mut self,
        i: usize,
        program_len: usize,
        repeats: usize,
    ) -> ShrinkResult<bool> {
        let original = self.current_nodes.clone();
        self.run_node_program(&original, i, program_len, repeats)
            .await
    }

    /// Apply the "delete n consecutive nodes" program `repeats` times at
    /// position `i` of `original`, then ask `consider` whether the
    /// resulting candidate is still interesting *and* an improvement.
    ///
    /// The deletion always operates on the supplied `original` snapshot,
    /// so repeat counts are well-defined regardless of intermediate
    /// shrink-target updates.
    async fn run_node_program(
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
        Ok(self.consider(&attempt).await? && self.improvements > epoch)
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_minimize_individual_choices_tests.rs"]
mod minimize_individual_choices_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_node_program_tests.rs"]
mod node_program_tests;

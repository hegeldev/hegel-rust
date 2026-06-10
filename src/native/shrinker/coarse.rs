//! Pre-shrink coarse reduction phase.
//!
//! The coarse reduction runs *once* before the main fixate loop, and
//! re-randomises small integer choices that look like they might be
//! `one_of` branch selectors. Lowering such a branch can require
//! regenerating everything after it, so this work doesn't compose well
//! with the lexicographic reductions in the main loop — hence the
//! separate one-shot phase.

use crate::native::bignum::BigInt;
use crate::native::core::{ChoiceKind, ChoiceValue};

use super::{ShrinkResult, Shrinker};

impl<'a> Shrinker<'a> {
    /// Coarse pre-shrink reductions that need their own phase because
    /// they can re-randomise (and thus enlarge) the test case.  Called
    /// from `test_runner.rs` once, before the main `shrink()` loop.
    pub(crate) fn initial_coarse_reduction(&mut self) -> ShrinkResult<()> {
        self.reduce_each_alternative()
    }

    /// Walk small non-negative integer nodes and try to lower them as
    /// `one_of` branch selectors. For each candidate node with
    /// `value <= 10` and `min_value == 0`:
    /// 1. Probe whether *zeroing* the node changes the shape of the
    ///    test case (length, or kinds at later positions).  If not, the
    ///    expensive re-randomisation isn't needed — the main loop's
    ///    lexicographic shrinking will pick it up cheaply.
    /// 2. Otherwise, try `try_lower_node_as_alternative(i, v)` for
    ///    each smaller candidate `v < node.value`, stopping at the
    ///    first success.
    fn reduce_each_alternative(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();
            let (ic, current_val) = match (node.kind.as_ref(), &node.value) {
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => (ic.clone(), v.clone()),
                _ => {
                    i += 1;
                    continue;
                }
            };
            if node.was_forced
                || current_val > BigInt::from(10)
                || ic.min_value.clone() != BigInt::from(0)
            {
                i += 1;
                continue;
            }
            // Probe: does zeroing the node change the shape? Routed through
            // `cached_test_function` for accounting and caching — and, like
            // Hypothesis's reduce_each_alternative zero_attempt, the probe
            // is incorporated: if zeroing happens to be an improvement it
            // is simply accepted and there is nothing left to repair.
            let zero_val = ic
                .value_from_bigint(&BigInt::from(0))
                .expect("0 fits a min==0 integer choice");
            let mut zeroed = self.current_nodes.clone();
            zeroed[i] = zeroed[i].with_value(ChoiceValue::Integer(zero_val));
            let (improved, run) = self.cached_test_function(&zeroed)?;
            let Some(run) = (if improved { None } else { run }) else {
                i += 1;
                continue;
            };
            let zero_actual = run.nodes;
            let shape_changed = zero_actual.len() != self.current_nodes.len()
                || (i + 1..self.current_nodes.len()).any(|j| {
                    j >= zero_actual.len()
                        || std::mem::discriminant(self.current_nodes[j].kind.as_ref())
                            != std::mem::discriminant(zero_actual[j].kind.as_ref())
                });
            if shape_changed {
                let mut v = BigInt::from(0);
                while v < current_val {
                    if self.try_lower_node_as_alternative(i, &v)? {
                        break;
                    }
                    v += 1;
                }
            }
            i += 1;
        }
        Ok(())
    }

    /// Lower the integer at `i` to `v`, retrying the suffix as random
    /// continuations to repair any shape changes the lower caused. Each
    /// probe is additionally span-spliced (Hypothesis's
    /// `try_lower_node_as_alternative` inner `for j in spans` loop): for
    /// every span starting at `i`, the probe's realised span content is
    /// spliced in front of the *original* suffix, repairing test cases
    /// whose post-branch draws must keep their old values.
    fn try_lower_node_as_alternative(&mut self, i: usize, v: &BigInt) -> ShrinkResult<bool> {
        // Callers iterate `i < self.current_nodes.len()`, so this is a
        // documented precondition.
        debug_assert!(i < self.current_nodes.len());
        // First try the bare lowering.
        if self.replace_int(i, v)? {
            return Ok(true);
        }
        // Couldn't lower directly; re-randomise the suffix via `probe`.
        // Use the lowered prefix as the probe's prefix and let the engine pick
        // a random continuation. The prefix is replayed (and width-coerced) by
        // `for_choices`, so a `BigInt`-wrapped value is fine here.
        let mut prefix: Vec<ChoiceValue> = self.current_nodes[..i]
            .iter()
            .map(|n| n.value.clone())
            .collect();
        prefix.push(ChoiceValue::Integer(v.clone()));
        let max_size = self.current_nodes.len() + 16;
        let epoch = self.improvements;
        let initial_nodes = self.current_nodes.clone();
        // Indices of spans starting at `i`, snapshot from the pre-probe
        // target — positionally matched against each probe's realised
        // spans, exactly as Hypothesis matches `initial.spans[j]` with
        // `random_attempt.spans[j]`.
        let span_idxs: Vec<usize> = self
            .current_spans
            .iter()
            .enumerate()
            .filter(|(_, s)| s.start == i)
            .map(|(j, _)| j)
            .collect();
        let initial_span_ends: Vec<Option<usize>> = span_idxs
            .iter()
            .map(|&j| self.current_spans.get(j).map(|s| s.end))
            .collect();
        for seed in 0..3u64 {
            let Some(attempt) = self.probe(&prefix, seed, max_size)? else {
                continue;
            };
            if self.improvements > epoch {
                return Ok(true);
            }
            for (&j, &initial_end) in span_idxs.iter().zip(&initial_span_ends) {
                let Some(initial_end) = initial_end else {
                    continue;
                };
                let Some(attempt_span) = attempt.spans.get(j) else {
                    continue;
                };
                if attempt_span.start > attempt_span.end
                    || attempt_span.end > attempt.nodes.len()
                    || initial_end > initial_nodes.len()
                {
                    continue;
                }
                let mut candidate = initial_nodes[..i].to_vec();
                candidate.extend_from_slice(&attempt.nodes[attempt_span.start..attempt_span.end]);
                candidate.extend_from_slice(&initial_nodes[initial_end..]);
                self.consider(&candidate)?;
                if self.improvements > epoch {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_coarse_tests.rs"]
mod tests;

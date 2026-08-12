//! Mutation-based shrink pass.
//!
//! Port of Hypothesis's `shrinking/mutation.py`. Tries random mutations of the
//! current best result to escape local optima that deterministic passes
//! can't find — particularly useful when switching a branch index
//! (e.g. `one_of`) requires multiple downstream values to change
//! simultaneously.
//!
//! Run as a last resort: mutations increase entropy, creating more work
//! for subsequent deterministic passes.

use crate::native::bignum::BigUint;
use crate::native::core::{ChoiceKind, ChoiceValue, sort_key};
use alloc::vec::Vec;

use super::{ShrinkResult, ShrinkRun, Shrinker};

/// Number of random continuations to try per mutation.
const RANDOM_ATTEMPTS: u64 = 3;

/// Number of random continuations to invest in a single-node mutation whose
/// first probe realised a different downstream shape. Only such candidates
/// (branch switches) need random repair, so they get a deep budget; a
/// shape-preserving candidate stops after its one observing probe, since the
/// deterministic passes already explore value changes against a fixed shape.
const DIVERGENT_RANDOM_ATTEMPTS: u64 = 15;

/// Results with more than this many nodes are skipped.
const MAX_MUTATE_NODES: usize = 32;

impl<'a> Shrinker<'a> {
    /// Try random mutations of a few positions to escape local optima.
    ///
    /// Port of Hypothesis's `shrinking/mutation.py::mutate_and_shrink`,
    /// with the random continuation budget concentrated on mutations that
    /// actually switch the downstream shape (see
    /// [`Shrinker::probe_observing_divergence`]).
    pub(super) async fn mutate_and_shrink(&mut self) -> ShrinkResult<()> {
        if self.current_nodes.len() > MAX_MUTATE_NODES {
            return Ok(());
        }
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();
            if matches!(
                node.data,
                crate::native::core::ChoiceData::Bytes(..)
                    | crate::native::core::ChoiceData::String(..)
            ) {
                i += 1;
                continue;
            }
            let Some(current_idx) = node.data.to_index()? else {
                i += 1;
                continue;
            };

            let node_value = node.value();
            let mut candidates: Vec<ChoiceValue> = Vec::new();
            for delta in 1u32..=5 {
                for &sign in &[1i32, -1] {
                    let new_idx_opt = index_offset(&current_idx, delta, sign);
                    let Some(new_idx) = new_idx_opt else {
                        continue;
                    };
                    if let Some(v) = node.data.from_index(new_idx)? {
                        if v != node_value && !candidates.contains(&v) {
                            candidates.push(v);
                        }
                    }
                }
            }

            for new_val in &candidates {
                let prefix: Vec<ChoiceValue> = self.current_nodes[..i]
                    .iter()
                    .map(|n| n.value())
                    .chain(core::iter::once(new_val.clone()))
                    .collect();
                let max_size = crate::native::core::flattened_len(&self.current_nodes);

                if !self
                    .probe_observing_divergence(&prefix, max_size, i)
                    .await?
                {
                    continue;
                }

                for _ in 0..DIVERGENT_RANDOM_ATTEMPTS {
                    self.probe(&prefix, max_size).await?;
                }

                let mut j_offset: usize = 1;
                while j_offset < 3 && i + j_offset < self.current_nodes.len() {
                    let j = i + j_offset;
                    j_offset += 1;

                    let data_j = self.current_nodes[j].data.clone();
                    let Some(unit_val) = data_j.from_index(BigUint::from(1u32))? else {
                        continue;
                    };
                    let mut two_prefix = prefix.clone();
                    for k in (i + 1)..=j {
                        if k == j {
                            two_prefix.push(unit_val.clone());
                        } else {
                            two_prefix.push(self.current_nodes[k].data.simplest_value()?);
                        }
                    }
                    for _ in 0..RANDOM_ATTEMPTS {
                        self.probe(&two_prefix, max_size).await?;
                    }
                }
            }
            i += 1;
        }
        Ok(())
    }

    /// As [`Shrinker::probe`], but additionally reports whether the run
    /// realised a *branch switch*: the same node count as the shrink target
    /// the probe was built from, but a different choice kind (constraints
    /// included) requested at position `i + 1`.
    ///
    /// The draws up to `i` replay the target's own values, so the kind the
    /// test requests at `i + 1` is deterministic — it changes exactly when
    /// the mutated value at `i` redirected the test down another branch
    /// (the `one_of` shape). Positions past `i + 1` carry the random
    /// continuation and are pure noise for this comparison. A `true` result
    /// means only a lucky random continuation can realise the alternative's
    /// interesting region: it is worth investing the deep random budget. A
    /// `false` result (including a stall-guarded no-op) means the shape
    /// either stayed stable — the deterministic passes cover that — or
    /// changed length, which is collection-resize territory the deletion
    /// and clone passes own.
    async fn probe_observing_divergence(
        &mut self,
        prefix: &[ChoiceValue],
        max_size: usize,
        i: usize,
    ) -> ShrinkResult<bool> {
        if self.improvements >= self.max_improvements {
            return Err(super::ShrinkHalt::Stop);
        }
        if self.improvements > 0
            && self.calls.saturating_sub(self.calls_at_last_shrink) >= self.max_stall
        {
            return Ok(false);
        }
        let expected: Vec<ChoiceKind> = self.current_nodes.iter().map(|n| n.data.kind()).collect();
        let (is_interesting, actual_nodes, actual_spans) = self
            .run_test_fn(ShrinkRun::Probe { prefix, max_size })
            .await?;
        self.calls += 1;
        let diverged = actual_nodes.len() == expected.len()
            && match (actual_nodes.get(i + 1), expected.get(i + 1)) {
                (Some(a), Some(e)) => a.data.kind() != *e,
                _ => false,
            };
        if is_interesting && sort_key(&actual_nodes) < sort_key(&self.current_nodes) {
            self.accept_improvement(actual_nodes, actual_spans);
        }
        Ok(diverged)
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_mutation_tests.rs"]
mod tests;

/// Offset `current_idx` by `delta * sign`, returning `None` if the
/// result would be negative.  Hypothesis works in Python ints, which
/// are arbitrary-precision and signed; the Rust port runs on a
/// `BigUint` and handles the negative-result case explicitly.
fn index_offset(current_idx: &BigUint, delta: u32, sign: i32) -> Option<BigUint> {
    let delta_big = BigUint::from(delta);
    if sign >= 0 {
        Some(current_idx + delta_big)
    } else if current_idx < &delta_big {
        None
    } else {
        Some(current_idx - delta_big)
    }
}

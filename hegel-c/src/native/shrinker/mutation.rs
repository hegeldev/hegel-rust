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
/// observing replay realised a different downstream shape. Only such
/// candidates (branch switches) need random repair, so they get a deep
/// budget; a shape-preserving candidate stops after its one observing
/// replay, since the deterministic passes already explore value changes
/// against a fixed shape.
const DIVERGENT_RANDOM_ATTEMPTS: u64 = 32;

/// Results with more than this many nodes are skipped.
const MAX_MUTATE_NODES: usize = 32;

impl<'a> Shrinker<'a> {
    /// Try random mutations of a few positions to escape local optima.
    ///
    /// Port of Hypothesis's `shrinking/mutation.py::mutate_and_shrink`,
    /// with the random continuation budget concentrated on mutations that
    /// actually switch the downstream shape (see
    /// [`Shrinker::replay_observing_divergence`]).
    pub(super) async fn mutate_and_shrink(&mut self) -> ShrinkResult<()> {
        if self.current_nodes.len() > MAX_MUTATE_NODES {
            return Ok(());
        }
        let mut i = 0;
        while i < self.current_nodes.len() {
            let snapshot: Vec<crate::native::core::ChoiceNode> = self.current_nodes.clone();
            let node = snapshot[i].clone();
            if node.was_forced
                || matches!(
                    node.data,
                    crate::native::core::ChoiceData::Bytes(..)
                        | crate::native::core::ChoiceData::String(..)
                )
            {
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
                let prefix: Vec<ChoiceValue> = snapshot[..i]
                    .iter()
                    .map(|n| n.value())
                    .chain(core::iter::once(new_val.clone()))
                    .collect();
                let max_size = crate::native::core::flattened_len(&snapshot);

                if !self
                    .replay_observing_divergence(&snapshot, new_val, i)
                    .await?
                {
                    continue;
                }

                for _ in 0..DIVERGENT_RANDOM_ATTEMPTS {
                    self.probe(&prefix, max_size).await?;
                }

                let mut j_offset: usize = 1;
                while j_offset < 3 && i + j_offset < snapshot.len() {
                    let j = i + j_offset;
                    j_offset += 1;

                    let data_j = snapshot[j].data.clone();
                    let Some(unit_val) = data_j.from_index(BigUint::from(1u32))? else {
                        continue;
                    };
                    let mut two_prefix = prefix.clone();
                    for (k, snap_node) in snapshot.iter().enumerate().take(j + 1).skip(i + 1) {
                        if k == j {
                            two_prefix.push(unit_val.clone());
                        } else {
                            two_prefix.push(snap_node.data.simplest_value()?);
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

    /// Replay `snapshot` with the value at `i` mutated to `new_val`, and
    /// report whether the run realised a *branch switch*: the same node
    /// count as the snapshot, but a different choice kind (constraints
    /// included) at some position past `i`.
    ///
    /// The replay feeds the snapshot's own values after the mutation, so
    /// the realised kind sequence is deterministic (punning included): it
    /// differs exactly where the mutated value redirected the test down
    /// another branch — including branches that open with the same kinds
    /// and only diverge later. A `true` result means only a lucky random
    /// continuation can realise the alternative's interesting region: it is
    /// worth investing the deep random budget. A `false` result (including
    /// a stall-guarded no-op) means the shape either stayed stable — the
    /// deterministic passes cover that — or changed length, which is
    /// collection-resize territory the deletion and clone passes own. As
    /// with [`Shrinker::probe`], an interesting, strictly smaller replay is
    /// adopted as the new shrink target on the way through.
    async fn replay_observing_divergence(
        &mut self,
        snapshot: &[crate::native::core::ChoiceNode],
        new_val: &ChoiceValue,
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
        let Some(replaced) = snapshot[i].with_value(new_val) else {
            return Ok(false);
        };
        let mut attempt = snapshot.to_vec();
        attempt[i] = replaced;
        let expected: Vec<ChoiceKind> = snapshot.iter().map(|n| n.data.kind()).collect();
        let (is_interesting, actual_nodes, actual_spans) =
            self.run_test_fn(ShrinkRun::Full(&attempt)).await?;
        self.calls += 1;
        let diverged = actual_nodes.len() == expected.len()
            && actual_nodes
                .iter()
                .zip(&expected)
                .skip(i + 1)
                .any(|(a, e)| a.data.kind() != *e);
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

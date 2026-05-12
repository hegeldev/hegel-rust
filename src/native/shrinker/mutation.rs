//! Mutation-based shrink pass.
//!
//! Port of pbtkit's `shrinking/mutation.py`. Tries random mutations of the
//! current best result to escape local optima that deterministic passes
//! can't find — particularly useful when switching a branch index
//! (e.g. `one_of`) requires multiple downstream values to change
//! simultaneously.
//!
//! Run as a last resort: mutations increase entropy, creating more work
//! for subsequent deterministic passes.

use crate::native::bignum::BigUint;
use crate::native::core::ChoiceValue;

use super::Shrinker;

/// Number of random continuations to try per mutation.
const RANDOM_ATTEMPTS: u64 = 3;

/// Results with more than this many nodes are skipped.
const MAX_MUTATE_NODES: usize = 32;

impl<'a> Shrinker<'a> {
    /// Try random mutations of a few positions to escape local optima.
    ///
    /// Port of pbtkit's `shrinking/mutation.py::mutate_and_shrink`.
    pub(super) fn mutate_and_shrink(&mut self) {
        if self.current_nodes.len() > MAX_MUTATE_NODES {
            return;
        }
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();
            let kind = node.kind.clone();
            let current_idx = kind.to_index(&node.value);

            // Small index offsets (±1 through ±5), keeping only indices >= 0
            // that produce distinct values from `node.value`.
            let mut candidates: Vec<ChoiceValue> = Vec::new();
            for delta in 1u32..=5 {
                for &sign in &[1i32, -1] {
                    let new_idx_opt = index_offset(&current_idx, delta, sign);
                    let Some(new_idx) = new_idx_opt else {
                        continue;
                    };
                    if let Some(v) = kind.from_index(new_idx) {
                        if v != node.value && !candidates.contains(&v) {
                            candidates.push(v);
                        }
                    }
                }
            }

            for new_val in &candidates {
                let prefix: Vec<ChoiceValue> = self.current_nodes[..i]
                    .iter()
                    .map(|n| n.value.clone())
                    .chain(std::iter::once(new_val.clone()))
                    .collect();
                let max_size = self.current_nodes.len();

                // Probe with a fixed seed (matches pbtkit's `Random(0)`),
                // then RANDOM_ATTEMPTS random continuations.
                self.probe(&prefix, 0, max_size);
                for attempt in 0..RANDOM_ATTEMPTS {
                    let seed = (i as u64).wrapping_mul(1000).wrapping_add(attempt);
                    self.probe(&prefix, seed, max_size);
                }

                // Also try setting each of the next few positions to the
                // `unit` value (index 1), with random continuation. Re-check
                // len each iteration since mutations above may have
                // shortened current_nodes.
                let mut j_offset: usize = 1;
                while j_offset < 3 && i + j_offset < self.current_nodes.len() {
                    let j = i + j_offset;
                    j_offset += 1;

                    let kind_j = self.current_nodes[j].kind.clone();
                    let Some(unit_val) = kind_j.from_index(BigUint::from(1u32)) else {
                        continue;
                    };
                    // Build prefix: values up to i, new_val at i, then for
                    // positions i+1..=j, fill with simplest except unit_val at j.
                    let mut two_prefix = prefix.clone();
                    for k in (i + 1)..=j {
                        if k == j {
                            two_prefix.push(unit_val.clone());
                        } else {
                            two_prefix.push(self.current_nodes[k].kind.simplest());
                        }
                    }
                    for attempt in 0..RANDOM_ATTEMPTS {
                        let seed = (i as u64)
                            .wrapping_mul(1000)
                            .wrapping_add((j_offset as u64).wrapping_mul(100))
                            .wrapping_add(attempt);
                        self.probe(&two_prefix, seed, max_size);
                    }
                }
            }
            i += 1;
        }
    }
}

/// Offset `current_idx` by `delta * sign`, returning `None` if the result
/// would be negative. pbtkit uses Python integers throughout; Rust needs
/// to handle the signed arithmetic on a `BigUint`.
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

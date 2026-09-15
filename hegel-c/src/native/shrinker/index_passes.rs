//! Index-based shrink passes: `lower_and_bump` and `try_shortening_via_increment`.
//!
//! Both passes use the `to_index`/`from_index` API on `ChoiceData` for
//! type-generic shrinking.

use crate::control::hegel_internal_unwrap;
use crate::native::HashMap;
use alloc::vec::Vec;

use crate::native::bignum::{BigUint, Zero};
use crate::native::core::{ChoiceData, ChoiceValue};

use super::{ShrinkResult, Shrinker};

/// Nodes the index passes skip even though they carry a dense index:
/// sequence kinds get their own dedicated passes. Clone nodes are skipped
/// too, structurally — they have no dense index, so `to_index` returns
/// `None` for them.
fn is_sequence(data: &ChoiceData) -> bool {
    matches!(data, ChoiceData::Bytes(..) | ChoiceData::String(..))
}

impl<'a> Shrinker<'a> {
    /// For each indexed node not at simplest, try decrementing it (lowering
    /// the index) and bumping a later node (raising its index).
    ///
    /// Value punning (via `for_choices` with `prefix_nodes`) handles the
    /// case where decrementing changes the kind at position `j` (e.g. a
    /// `one_of` branch switch).
    ///
    /// The bumps tried on `j` are small relative offsets, then small
    /// absolute indices, then the largest index: for a pair coupled through
    /// a product or a threshold (`a × k ≥ 1000`), lowering `a` needs `k`
    /// raised much further than a few units, and the largest value is the
    /// one most likely to keep the predicate — once it does, the
    /// value-lowering passes bring `k` down to the smallest value that
    /// still does.
    pub(super) async fn lower_and_bump(&mut self) -> ShrinkResult<()> {
        let max_gap = core::cmp::min(self.current_nodes.len(), 4);
        for gap in 1..max_gap {
            let mut idx = 0;
            while idx < self.current_nodes.len() {
                let i = idx;
                let node_i = self.current_nodes[i].clone();
                if is_sequence(&node_i.data) {
                    idx += 1;
                    continue;
                }
                let Some(current_idx) = node_i.data.to_index()? else {
                    idx += 1;
                    continue;
                };
                if current_idx.is_zero() {
                    idx += 1;
                    continue;
                }

                let mut decrement_targets: Vec<ChoiceValue> = Vec::new();
                if current_idx > BigUint::from(1u32) {
                    let v0 = hegel_internal_unwrap!(
                        node_i.data.from_index(BigUint::zero())?,
                        "lower_and_bump: from_index(0) has no value for an indexed kind"
                    );
                    decrement_targets.push(v0);
                }
                if let Some(v_prev) = node_i.data.from_index(&current_idx - BigUint::from(1u32))? {
                    if !decrement_targets.contains(&v_prev) {
                        decrement_targets.push(v_prev);
                    }
                }

                let j_opt = i.checked_add(gap).filter(|&j| j < self.current_nodes.len());
                let Some(j) = j_opt else {
                    idx += 1;
                    continue;
                };

                for new_val in &decrement_targets {
                    if gap == 1 {
                        let mut attempt = self.current_nodes.clone();
                        if let Some(lowered) = attempt[i].with_value(new_val) {
                            attempt[i] = lowered;
                            self.consider(&attempt).await?;

                            let mut zeroed = attempt;
                            for node in &mut zeroed[i + 1..] {
                                *node = node.with_simplest()?;
                            }
                            self.consider(&zeroed).await?;
                        }
                    }

                    if j < self.current_nodes.len() && !is_sequence(&self.current_nodes[j].data) {
                        let data_j = self.current_nodes[j].data.clone();
                        let Some((target_idx, max_j)) = data_j.to_index()?.zip(data_j.max_index())
                        else {
                            continue;
                        };
                        let mut bumped_any_relative = false;
                        for bump in [1u32, 2, 4] {
                            let candidate_idx = &target_idx + BigUint::from(bump);
                            if let Some(bumped) = data_j.from_index(candidate_idx)? {
                                if try_bump_ij(self, i, new_val, j, &bumped).await? {
                                    bumped_any_relative = true;
                                    break;
                                }
                            }
                        }
                        if !bumped_any_relative {
                            let mut p = BigUint::from(1u32);
                            for _ in 0..8 {
                                if p > max_j {
                                    break;
                                }
                                let p_minus_one = &p - BigUint::from(1u32);
                                if let Some(v) = data_j.from_index(p_minus_one)? {
                                    try_bump_ij(self, i, new_val, j, &v).await?;
                                }
                                if let Some(v) = data_j.from_index(p.clone())? {
                                    try_bump_ij(self, i, new_val, j, &v).await?;
                                }
                                p *= BigUint::from(2u32);
                            }
                            if let Some(v) = data_j.from_index(max_j)? {
                                try_bump_ij(self, i, new_val, j, &v).await?;
                            }
                        }
                    }
                }
                idx += 1;
            }
        }
        Ok(())
    }

    /// For each indexed node, try *raising* its index to see if the test
    /// takes a shorter path: a gate whose larger values skip the draw behind
    /// it, or an earlier exit.
    ///
    /// A value shrinker can only make values simpler; sometimes making a
    /// value *less* simple (`false → true`, a coin past its threshold)
    /// removes the draws that follow it, producing a shorter and thus
    /// overall simpler choice sequence. The candidates are larger as
    /// written, so they go through [`Shrinker::try_adopt`], which runs them
    /// and keeps the result only when what comes back is smaller. Two
    /// candidates per node — the next index and the largest — keep the pass
    /// to two test runs per node: the largest value is the one most likely
    /// to fall on the far side of a threshold, and once the shorter shape is
    /// adopted the value-lowering passes bring it down to the smallest value
    /// that keeps it. The last node is skipped: nothing follows it that a
    /// raise could remove.
    pub(super) async fn try_shortening_via_increment(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i + 1 < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();
            if node.was_forced || is_sequence(&node.data) {
                i += 1;
                continue;
            }
            let Some(current_idx) = node.data.to_index()? else {
                i += 1;
                continue;
            };

            let node_value = node.value();
            let mut candidates: Vec<ChoiceValue> = Vec::new();
            let raised = [
                Some(&current_idx + BigUint::from(1u32)),
                node.data.max_index(),
            ];
            for idx in raised.into_iter().flatten() {
                if let Some(v) = node.data.from_index(idx)? {
                    if v != node_value && !candidates.contains(&v) {
                        candidates.push(v);
                    }
                }
            }

            for raised in &candidates {
                let Some(current) = self.current_nodes.get(i) else {
                    break;
                };
                let Some(bumped) = current.with_value(raised) else {
                    continue;
                };
                let mut attempt = self.current_nodes.clone();
                attempt[i] = bumped;
                self.try_adopt(&attempt).await?;
            }
            i += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_index_passes_tests.rs"]
mod tests;

/// Helper for `lower_and_bump`: replace `{i: new_val, j: bump_val}` if the
/// kind at j validates `bump_val`. Returns whether the attempt was
/// interesting.
pub(super) async fn try_bump_ij(
    shrinker: &mut Shrinker<'_>,
    i: usize,
    new_val: &ChoiceValue,
    j: usize,
    bump_val: &ChoiceValue,
) -> ShrinkResult<bool> {
    let replacements: HashMap<usize, ChoiceValue> = [(i, new_val.clone()), (j, bump_val.clone())]
        .into_iter()
        .collect();
    shrinker.replace(&replacements).await
}

//! Index-based shrink passes: `lower_and_bump` and `try_shortening_via_increment`.
//!
//! Both passes use the `to_index`/`from_index` API on `ChoiceData` for
//! type-generic shrinking.

use crate::control::hegel_internal_unwrap;
use crate::native::HashMap;
use alloc::vec::Vec;

use crate::native::bignum::{BigInt, BigUint, Zero};
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
                        }
                    }
                }
                idx += 1;
            }
        }
        Ok(())
    }

    /// For each indexed node, try *incrementing* its index to see if the test
    /// takes a shorter path (e.g. triggering an earlier exit).
    ///
    /// A value shrinker can only make values simpler; sometimes making a
    /// value *less* simple (e.g. `false → true`) causes an earlier exit,
    /// producing a shorter and thus overall simpler choice sequence.
    pub(super) async fn try_shortening_via_increment(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();
            if is_sequence(&node.data) {
                i += 1;
                continue;
            }
            let Some(current_idx) = node.data.to_index()? else {
                i += 1;
                continue;
            };

            let mut candidates: Vec<ChoiceValue> = Vec::new();
            let node_value = node.value();
            for d in [1u32, 2, 4, 8, 16] {
                let t = &current_idx + BigUint::from(d);
                if let Some(v) = node.data.from_index(t)? {
                    if v != node_value && !candidates.contains(&v) {
                        candidates.push(v);
                    }
                }
            }
            if let Some(mi) = node.data.max_index() {
                if let Some(v) = node.data.from_index(mi)? {
                    if v != node_value && !candidates.contains(&v) {
                        candidates.push(v);
                    }
                }
            }

            if let ChoiceData::Integer(ic, _) = &node.data {
                for e in 0u32..11 {
                    let magnitude = BigInt::from(1u64 << e);
                    for sign in [BigInt::from(1), BigInt::from(-1)] {
                        let Some(av) = ic.value_from_bigint(&(sign * &magnitude)) else {
                            continue;
                        };
                        let candidate_val = ChoiceValue::Integer(av);
                        if candidate_val != node_value && !candidates.contains(&candidate_val) {
                            candidates.push(candidate_val);
                        }
                    }
                }
            }

            if candidates.is_empty() {
                i += 1;
                continue;
            }

            for incremented in &candidates {
                if i >= self.current_nodes.len() {
                    break;
                }
                let mut attempt = self.current_nodes.clone();
                let Some(bumped) = attempt[i].with_value(incremented) else {
                    continue;
                };
                attempt[i] = bumped;
                let mut zeroed = attempt.clone();
                for node in &mut zeroed[i + 1..] {
                    *node = node.with_simplest()?;
                }
                self.consider(&zeroed).await?;
            }
            i += 1;
        }
        Ok(())
    }
}

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

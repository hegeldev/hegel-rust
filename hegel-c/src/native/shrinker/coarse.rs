//! Pre-shrink coarse reduction phase.
//!
//! The coarse reduction runs *once* before the main fixate loop, and
//! re-randomises small integer choices that look like they might be
//! `one_of` branch selectors. Lowering such a branch can require
//! regenerating everything after it, so this work doesn't compose well
//! with the lexicographic reductions in the main loop — hence the
//! separate one-shot phase.

use crate::native::bignum::BigInt;
use crate::native::core::{ChoiceData, ChoiceNode, ChoiceValue};
use alloc::vec::Vec;

use super::{ShrinkResult, ShrinkRun, Shrinker};
use crate::control::hegel_internal_debug_assert;

impl<'a> Shrinker<'a> {
    /// Coarse pre-shrink reductions that need their own phase because
    /// they can re-randomise (and thus enlarge) the test case.  Called
    /// from `test_runner.rs` once, before the main `shrink()` loop.
    pub(crate) async fn initial_coarse_reduction(&mut self) -> ShrinkResult<()> {
        self.reduce_each_alternative().await
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
    async fn reduce_each_alternative(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();
            let ChoiceData::Integer(ic, current_val) = &node.data else {
                i += 1;
                continue;
            };
            let (ic, current_val) = (alloc::sync::Arc::clone(ic), current_val.clone());
            if node.was_forced
                || current_val > BigInt::from(10)
                || ic.min_value.clone() != BigInt::from(0)
            {
                i += 1;
                continue;
            }
            let mut zeroed = self.current_nodes.clone();
            zeroed[i] = ChoiceNode::new(
                ChoiceData::Integer(alloc::sync::Arc::clone(&ic), BigInt::from(0)),
                zeroed[i].was_forced,
            );
            let (_, zero_actual, _) = self.run_test_fn(ShrinkRun::Full(&zeroed)).await?;
            let shape_changed = zero_actual.len() != self.current_nodes.len()
                || (i + 1..self.current_nodes.len()).any(|j| {
                    j >= zero_actual.len()
                        || core::mem::discriminant(&self.current_nodes[j].data)
                            != core::mem::discriminant(&zero_actual[j].data)
                });
            if shape_changed {
                let mut v = BigInt::from(0);
                while v < current_val {
                    if self.try_lower_node_as_alternative(i, &v).await? {
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
    /// continuations to repair any shape changes the lower caused.
    async fn try_lower_node_as_alternative(&mut self, i: usize, v: &BigInt) -> ShrinkResult<bool> {
        hegel_internal_debug_assert!(i < self.current_nodes.len());
        if self.replace_int(i, v).await? {
            return Ok(true);
        }
        let mut prefix: Vec<ChoiceValue> =
            self.current_nodes[..i].iter().map(|n| n.value()).collect();
        prefix.push(ChoiceValue::Integer(v.clone()));
        let max_size = crate::native::core::flattened_len(&self.current_nodes) + 16;
        let epoch = self.improvements;
        for _ in 0..3 {
            self.probe(&prefix, max_size).await?;
            if self.improvements > epoch {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_coarse_tests.rs"]
mod tests;

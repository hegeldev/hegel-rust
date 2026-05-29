//! Pre-shrink coarse reduction phase.
//!
//! The coarse reduction runs *once* before the main fixate loop, and
//! re-randomises small integer choices that look like they might be
//! `one_of` branch selectors. Lowering such a branch can require
//! regenerating everything after it, so this work doesn't compose well
//! with the lexicographic reductions in the main loop — hence the
//! separate one-shot phase.

use crate::native::bignum::BigInt;
use crate::native::core::{AnyInteger, ChoiceKind, ChoiceValue};

use super::{ShrinkRun, Shrinker};

impl<'a> Shrinker<'a> {
    /// Coarse pre-shrink reductions that need their own phase because
    /// they can re-randomise (and thus enlarge) the test case.  Called
    /// from `test_runner.rs` once, before the main `shrink()` loop.
    pub(crate) fn initial_coarse_reduction(&mut self) {
        self.reduce_each_alternative();
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
    fn reduce_each_alternative(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();
            let (ic, current_val) = match (&node.kind, &node.value) {
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => (ic.clone(), v.to_bigint()),
                _ => {
                    i += 1;
                    continue;
                }
            };
            if node.was_forced
                || current_val > BigInt::from(10)
                || ic.min_bigint() != BigInt::from(0)
            {
                i += 1;
                continue;
            }
            // Probe: does zeroing the node change the shape?
            let zero_val = ic
                .value_from_bigint(&BigInt::from(0))
                .expect("0 fits a min==0 integer choice");
            let mut zeroed = self.current_nodes.clone();
            zeroed[i] = zeroed[i].with_value(ChoiceValue::Integer(zero_val));
            let (_, zero_actual, _) = (self.test_fn)(ShrinkRun::Full(&zeroed));
            let shape_changed = zero_actual.len() != self.current_nodes.len()
                || (i + 1..self.current_nodes.len()).any(|j| {
                    j >= zero_actual.len()
                        || std::mem::discriminant(&self.current_nodes[j].kind)
                            != std::mem::discriminant(&zero_actual[j].kind)
                });
            if shape_changed {
                let mut v = BigInt::from(0);
                while v < current_val {
                    if self.try_lower_node_as_alternative(i, &v) {
                        break;
                    }
                    v += 1;
                }
            }
            i += 1;
        }
    }

    /// Lower the integer at `i` to `v`, retrying the suffix as random
    /// continuations to repair any shape changes the lower caused.
    fn try_lower_node_as_alternative(&mut self, i: usize, v: &BigInt) -> bool {
        // Callers iterate `i < self.current_nodes.len()`, so this is a
        // documented precondition.
        debug_assert!(i < self.current_nodes.len());
        // First try the bare lowering.
        if self.replace_int(i, v) {
            return true;
        }
        // Couldn't lower directly; re-randomise the suffix via `probe`.
        // Use the lowered prefix as the probe's prefix and let the engine pick
        // a random continuation. The prefix is replayed (and width-coerced) by
        // `for_choices`, so a `BigInt`-wrapped value is fine here.
        let mut prefix: Vec<ChoiceValue> = self.current_nodes[..i]
            .iter()
            .map(|n| n.value.clone())
            .collect();
        prefix.push(ChoiceValue::Integer(AnyInteger::Big(v.clone())));
        let max_size = self.current_nodes.len() + 16;
        let epoch = self.improvements;
        for seed in 0..3u64 {
            self.probe(&prefix, seed, max_size);
            if self.improvements > epoch {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_coarse_tests.rs"]
mod tests;

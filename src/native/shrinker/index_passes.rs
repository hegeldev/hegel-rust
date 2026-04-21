//! Index-based shrink passes: `lower_and_bump` and `try_shortening_via_increment`.
//!
//! Port of pbtkit's `shrinking/index_passes.py`. Both passes use the
//! `to_index`/`from_index` API on `ChoiceKind` for type-generic shrinking.

use std::collections::HashMap;

use crate::native::bignum::{BigUint, Zero};
use crate::native::core::{ChoiceNode, ChoiceValue};

use super::Shrinker;

impl<'a> Shrinker<'a> {
    /// For each indexed node not at simplest, try decrementing it (lowering
    /// the index) and bumping a later node (raising its index).
    ///
    /// Port of pbtkit's `shrinking/index_passes.py::lower_and_bump`. Value
    /// punning (via `with_value` + `for_choices` with `prefix_nodes`) handles
    /// the case where decrementing changes the kind at position `j` (e.g.
    /// a `one_of` branch switch).
    pub(super) fn lower_and_bump(&mut self) {
        let max_gap = std::cmp::min(self.current_nodes.len(), 4);
        for gap in 1..max_gap {
            let mut idx = 0;
            while idx < self.current_nodes.len() {
                let i = idx;
                let node_i = self.current_nodes[i].clone();
                let kind_i = node_i.kind.clone();
                let current_idx = kind_i.to_index(&node_i.value);
                if current_idx.is_zero() {
                    idx += 1;
                    continue;
                }

                // Decrement targets: simplest (index 0), then `current-1`.
                // Trying simplest first handles cases where intermediate steps
                // don't produce interesting results but the full decrement
                // does (e.g. sampled_from where only index 0 satisfies a
                // downstream constraint).
                let mut decrement_targets: Vec<ChoiceValue> = Vec::new();
                if current_idx > BigUint::from(1u32) {
                    let v0 = kind_i
                        .from_index(BigUint::zero())
                        .expect("from_index(0) is simplest and always valid");
                    decrement_targets.push(v0);
                }
                // `from_index(current_idx - 1)` can be None for bounded float
                // ranges with gaps.
                if let Some(v_prev) = kind_i.from_index(&current_idx - BigUint::from(1u32)) {
                    if !decrement_targets.contains(&v_prev) {
                        decrement_targets.push(v_prev);
                    }
                }

                // Find bump target `j`: the gap'th node after i.
                let j_opt = (i + gap)
                    .checked_add(0)
                    .filter(|&j| j < self.current_nodes.len());
                let Some(j) = j_opt else {
                    idx += 1;
                    continue;
                };

                for new_val in &decrement_targets {
                    // Build the decrement attempt and run it. Running both
                    // `attempt` and the zero-padded variant matches pbtkit —
                    // the run is for its side-effect on `current` (the
                    // shrinker auto-updates to smaller interesting results).
                    let mut attempt = self.current_nodes.clone();
                    attempt[i] = attempt[i].with_value(new_val.clone());
                    self.consider(&attempt);

                    let mut zeroed = attempt.clone();
                    for node in &mut zeroed[i + 1..] {
                        let s = node.kind.simplest();
                        *node = node.with_value(s);
                    }
                    self.consider(&zeroed);

                    // Try bumping node `j` at relative and absolute index
                    // offsets. `_try_bump_j` is pbtkit-equivalent replace
                    // with validate — skips when the bumped value doesn't
                    // fit the *current* kind at j (which may have shifted
                    // under punning between iterations).
                    if j < self.current_nodes.len() {
                        let kind_j = self.current_nodes[j].kind.clone();
                        let target_idx = kind_j.to_index(&self.current_nodes[j].value);
                        let mut bumped_any_relative = false;
                        for bump in [1u32, 2, 4] {
                            let candidate_idx = &target_idx + BigUint::from(bump);
                            if let Some(bumped) = kind_j.from_index(candidate_idx) {
                                if try_bump_ij(self, i, new_val, j, &bumped) {
                                    bumped_any_relative = true;
                                    break;
                                }
                            }
                        }
                        if !bumped_any_relative {
                            let max_j = kind_j.max_index();
                            let mut p = BigUint::from(1u32);
                            for _ in 0..8 {
                                if p > max_j {
                                    break;
                                }
                                let p_minus_one = &p - BigUint::from(1u32);
                                if let Some(v) = kind_j.from_index(p_minus_one) {
                                    try_bump_ij(self, i, new_val, j, &v);
                                }
                                if let Some(v) = kind_j.from_index(p.clone()) {
                                    try_bump_ij(self, i, new_val, j, &v);
                                }
                                p *= BigUint::from(2u32);
                            }
                        }
                    }
                }
                idx += 1;
            }
        }
    }

    /// For each indexed node, try *incrementing* its index to see if the test
    /// takes a shorter path (e.g. triggering an earlier exit).
    ///
    /// Port of pbtkit's `shrinking/index_passes.py::try_shortening_via_increment`.
    /// A value shrinker can only make values simpler; sometimes making a value
    /// *less* simple (e.g. `false → true`) causes an earlier exit, producing a
    /// shorter and thus overall simpler choice sequence.
    pub(super) fn try_shortening_via_increment(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();
            let kind = node.kind.clone();
            let current_idx = kind.to_index(&node.value);

            let mut candidates: Vec<ChoiceValue> = Vec::new();
            for d in [1u32, 2, 4, 8, 16] {
                let t = &current_idx + BigUint::from(d);
                if let Some(v) = kind.from_index(t) {
                    if v != node.value && !candidates.contains(&v) {
                        candidates.push(v);
                    }
                }
            }
            if let Some(v) = kind.from_index(kind.max_index()) {
                if v != node.value && !candidates.contains(&v) {
                    candidates.push(v);
                }
            }

            // Also try powers of 2 (and negatives) as raw values. This covers
            // large index-space jumps that exponential index probing misses
            // (e.g. `-128.0` for a float test checking `v < -86`).
            for e in 0u32..11 {
                let magnitude: i128 = 1i128 << e;
                for &sign in &[1i128, -1] {
                    let v_int = sign * magnitude;
                    let mag_f = magnitude as f64;
                    let v_float = (sign as f64) * mag_f;
                    for candidate_val in [ChoiceValue::Integer(v_int), ChoiceValue::Float(v_float)]
                    {
                        if kind.validate(&candidate_val)
                            && candidate_val != node.value
                            && !candidates.contains(&candidate_val)
                        {
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
                attempt[i] = attempt[i].with_value(incremented.clone());
                let mut zeroed = attempt.clone();
                for node in &mut zeroed[i + 1..] {
                    let s = node.kind.simplest();
                    *node = node.with_value(s);
                }
                self.consider(&zeroed);
            }
            i += 1;
        }
    }
}

/// Helper for `lower_and_bump`: replace `{i: new_val, j: bump_val}` if the
/// kind at j validates `bump_val`. Returns whether the attempt was
/// interesting.
fn try_bump_ij(
    shrinker: &mut Shrinker<'_>,
    i: usize,
    new_val: &ChoiceValue,
    j: usize,
    bump_val: &ChoiceValue,
) -> bool {
    let nodes: &[ChoiceNode] = &shrinker.current_nodes;
    if j >= nodes.len() {
        return false;
    }
    if !nodes[j].kind.validate(bump_val) {
        return false;
    }
    let replacements: HashMap<usize, ChoiceValue> = [(i, new_val.clone()), (j, bump_val.clone())]
        .into_iter()
        .collect();
    shrinker.replace(&replacements)
}

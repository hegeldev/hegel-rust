// Integer-based shrink passes: zero_choices, swap_integer_sign,
// binary_search_integer_towards_zero, redistribute_integers, shrink_duplicates,
// lower_common_node_offset.
//
// Integer choice values use `BigInt` directly, so these passes do their
// arithmetic in arbitrary precision and write candidates back through
// `IntegerChoice::value_from_bigint` (which rejects out-of-range candidates).
// Shrinking is far colder than generation, so the `BigInt` allocation here is
// acceptable.

use std::collections::HashMap;

use crate::native::bignum::{BigInt, Sign, Signed};
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceValue};

use super::{ShrinkResult, Shrinker, bin_search_down_big_r, find_integer_r};
use crate::control::hegel_internal_debug_assert;

/// The low `keep` bits of the non-negative `v`, i.e. `v mod 2^keep`.
fn low_bits(v: &BigInt, keep: usize) -> BigInt {
    v - &BigInt::from((v >> keep).magnitude() << keep)
}

impl<'a> Shrinker<'a> {
    /// Current integer value at node `i` as a [`BigInt`].
    pub(super) fn int_value_bigint(&self, i: usize) -> BigInt {
        match &self.current_nodes[i].value {
            ChoiceValue::Integer(v) => v.clone(),
            _ => unreachable!("int_value_bigint on non-integer node"),
        }
    }

    /// Build a width-correct integer replacement value for node `i`. Callers
    /// (`bind_deletion`, `minimize_individual_choices`) only invoke this for an
    /// in-range integer node with a candidate inside `[min, max] ⊆ width`, so
    /// neither the kind nor the width conversion can fail.
    pub(super) fn int_replacement(&self, i: usize, candidate: &BigInt) -> ChoiceValue {
        let ChoiceKind::Integer(ic) = self.current_nodes[i].kind.as_ref() else {
            unreachable!("int_replacement on non-integer node")
        };
        ChoiceValue::Integer(
            ic.value_from_bigint(candidate)
                .unwrap_or_else(|| unreachable!("candidate fits the node's width")),
        )
    }

    /// Attempt to replace node `i` with `candidate`. The candidate is handed to
    /// [`Shrinker::replace`], which range-checks it and coerces it to the
    /// node's width (rejecting out-of-range candidates), so this stays correct
    /// for any node width.
    pub(super) fn replace_int(&mut self, i: usize, candidate: &BigInt) -> ShrinkResult<bool> {
        self.replace(&HashMap::from([(
            i,
            ChoiceValue::Integer(candidate.clone()),
        )]))
    }

    /// Attempt to replace two integer nodes simultaneously; `replace`
    /// range-checks and width-coerces each candidate.
    pub(super) fn replace_two(
        &mut self,
        i: usize,
        vi: &BigInt,
        j: usize,
        vj: &BigInt,
    ) -> ShrinkResult<bool> {
        self.replace(&HashMap::from([
            (i, ChoiceValue::Integer(vi.clone())),
            (j, ChoiceValue::Integer(vj.clone())),
        ]))
    }

    /// Replace blocks of choices with their simplest values.
    pub(super) fn zero_choices(&mut self) -> ShrinkResult<()> {
        let mut k = self.current_nodes.len();
        while k > 0 {
            let mut i = 0;
            while i + k <= self.current_nodes.len() {
                let nodes = &self.current_nodes;
                if nodes[i].value == nodes[i].kind.simplest() {
                    i += 1;
                } else {
                    let replacements: HashMap<usize, ChoiceValue> = (i..i + k)
                        .map(|j| (j, self.current_nodes[j].kind.simplest()))
                        .collect();
                    self.replace(&replacements)?;
                    i += k;
                }
            }
            k /= 2;
        }
        Ok(())
    }

    /// For integer choices: try simplest, then flip negative to positive.
    pub(super) fn swap_integer_sign(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            if let (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) = (
                self.current_nodes[i].kind.as_ref(),
                &self.current_nodes[i].value,
            ) {
                let v = v.clone();
                let simplest = ic.simplest();
                if v != ic.simplest() {
                    self.replace(&HashMap::from([(i, ChoiceValue::Integer(simplest))]))?;
                }
                // Re-read in case the replace changed things.
                if i < self.current_nodes.len() {
                    if let ChoiceValue::Integer(v) = &self.current_nodes[i].value {
                        let v = v.clone();
                        if v.sign() == Sign::Minus {
                            self.replace_int(i, &(-&v))?;
                        }
                    }
                }
            }
            i += 1;
        }
        Ok(())
    }

    /// Shrink each integer node's distance from its clamped
    /// `shrink_towards`, probing both sides of the target.
    ///
    /// Port of Hypothesis's `minimize_individual_nodes` integer handling,
    /// which runs `Integer.shrink(abs(shrink_towards - value))` against both
    /// `shrink_towards + n` and `shrink_towards - n`, with the `Integer`
    /// moves from `shrinking/integer.py`: guaranteed probes of distance 0,
    /// 1, `d - 1` and `d - 2`, plus `mask_high_bits` (drop the top bits of
    /// the distance — predicates like `x & 0xff == 0x77` stall without it),
    /// the squeeze-into-one-byte probes, the shift-right descent, and
    /// multiple-subtraction, iterated to a fixpoint.
    pub(super) fn binary_search_integer_towards_zero(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let ic = match self.current_nodes[i].kind.as_ref() {
                ChoiceKind::Integer(ic) => ic.clone(),
                _ => {
                    i += 1;
                    continue;
                }
            };
            let target = ic.clamped_shrink_towards();

            // short_circuit: distances 0 and 1 are always tried.
            self.try_at_distance(i, &ic, &target, &BigInt::from(0))?;
            self.try_at_distance(i, &ic, &target, &BigInt::from(1))?;

            // mask_high_bits: keep only the low `bits - k` bits of the
            // distance.
            let base = self.distance_from(i, &target);
            let n_bits = base.bits();
            find_integer_r(|k| {
                if k as u64 >= n_bits {
                    return Ok(false);
                }
                let keep = (n_bits - k as u64) as usize;
                let masked = low_bits(&base, keep);
                self.try_at_distance(i, &ic, &target, &masked)
            })?;

            // Squeeze the distance into a single byte: its top byte, then
            // its bottom byte.
            let base = self.distance_from(i, &target);
            if base.bits() > 8 {
                let top = &base >> (base.bits() as usize - 8);
                self.try_at_distance(i, &ic, &target, &top)?;
                let bottom = low_bits(&base, 8);
                self.try_at_distance(i, &ic, &target, &bottom)?;
            }

            // run_step to a fixpoint: shift_right, then multiples of 2 and 1
            // (the latter two guarantee `d - 2` and `d - 1` are probed).
            loop {
                let before = self.distance_from(i, &target);
                if before == BigInt::from(0) {
                    break;
                }
                let max_shift = before.bits() as usize + 1;
                find_integer_r(|k| {
                    let candidate = &before >> k.min(max_shift);
                    self.try_at_distance(i, &ic, &target, &candidate)
                })?;
                for step in [2u64, 1] {
                    let base = self.distance_from(i, &target);
                    find_integer_r(|n| {
                        let sub = BigInt::from(step) * BigInt::from(n as u64);
                        if sub > base {
                            return Ok(false);
                        }
                        self.try_at_distance(i, &ic, &target, &(&base - &sub))
                    })?;
                }
                if self.distance_from(i, &target) == before {
                    break;
                }
            }
            i += 1;
        }
        Ok(())
    }

    /// `|value(i) - target|` as a non-negative `BigInt`.
    fn distance_from(&self, i: usize, target: &BigInt) -> BigInt {
        let v = self.int_value_bigint(i);
        BigInt::from((&v - target).magnitude())
    }

    /// Probe node `i` at `target + d`, then — when that is rejected — at
    /// `target - d`. The sort key orders equal distances above-first, so the
    /// above side is always offered first.
    fn try_at_distance(
        &mut self,
        i: usize,
        ic: &IntegerChoice,
        target: &BigInt,
        d: &BigInt,
    ) -> ShrinkResult<bool> {
        let above = target + d;
        let mut accepted = false;
        if ic.validate(&above) {
            accepted = self.replace_int(i, &above)?;
        }
        if !accepted && d.sign() == Sign::Plus {
            let below = target - d;
            if ic.validate(&below) {
                accepted = self.replace_int(i, &below)?;
            }
        }
        Ok(accepted)
    }

    /// Try redistributing value between pairs of integer choices.
    ///
    /// For each pair of integer nodes at various distances, tries moving
    /// value from i to j (or vice versa) while keeping the total sum
    /// constant. Useful for sum-type constraints where the minimal
    /// counterexample has one small and one large value.
    pub(super) fn redistribute_integers(&mut self) -> ShrinkResult<()> {
        let int_indices: Vec<usize> = self
            .current_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                if matches!(n.kind.as_ref(), ChoiceKind::Integer(_)) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        let max_gap = 8.min(int_indices.len());
        for gap in 1..max_gap {
            let n = int_indices.len();
            let mut pair_idx = n.saturating_sub(gap + 1);
            loop {
                // Re-collect integer indices since earlier passes may have changed the nodes.
                let current_ints: Vec<usize> = self
                    .current_nodes
                    .iter()
                    .enumerate()
                    .filter_map(|(i, node)| {
                        if matches!(node.kind.as_ref(), ChoiceKind::Integer(_)) {
                            Some(i)
                        } else {
                            None
                        }
                    })
                    .collect();

                // Defensive edge case: only reached when a prior shrink removed
                // enough integer nodes that `pair_idx + gap` overshoots the new
                // length.
                if pair_idx + gap >= current_ints.len() {
                    if pair_idx == 0 {
                        break;
                    }
                    pair_idx -= 1;
                    continue;
                }

                let i = current_ints[pair_idx];
                let j = current_ints[pair_idx + gap];

                let prev_i = self.int_value_bigint(i);
                let prev_j = self.int_value_bigint(j);
                let target_i = match self.current_nodes[i].kind.as_ref() {
                    ChoiceKind::Integer(ic) => ic.clamped_shrink_towards(),
                    _ => unreachable!(
                        "kind/value invariant violated: outer match guaranteed this variant"
                    ),
                };

                // Shrink i's distance from its shrink target (staying on its
                // current side, like Hypothesis's `k > abs(m - shrink_towards)`
                // cap), moving the difference onto j so the sum is preserved.
                let prev_dist = BigInt::from((&prev_i - &target_i).magnitude());
                if prev_dist.sign() == Sign::Plus {
                    let on_low_side = prev_i < target_i;
                    bin_search_down_big_r(BigInt::from(0), prev_dist.clone(), &mut |d| {
                        let new_i = if on_low_side {
                            &target_i - d
                        } else {
                            &target_i + d
                        };
                        let new_j = &prev_j + (&prev_i - &new_i);
                        self.replace_two(i, &new_i, j, &new_j)
                    })?;
                }

                if pair_idx == 0 {
                    break;
                }
                pair_idx -= 1;
            }
        }
        Ok(())
    }

    /// Lower pairs of nearby integer choices by the same amount
    /// simultaneously.
    ///
    /// When two values are pinned together by a predicate like `|m - n| == 1`,
    /// neither can move on its own without breaking the predicate, and the
    /// shrinker falls into a zig-zag trap. By probing `(v_i - k, v_j - k)` for
    /// geometrically growing `k` via `find_integer`, this pass reaches the
    /// minimum in `O(log k)` probes.
    pub(super) fn lower_integers_together(&mut self) -> ShrinkResult<()> {
        let int_indices: Vec<usize> = self
            .current_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                if matches!(n.kind.as_ref(), ChoiceKind::Integer(_)) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        for pair_idx in 0..int_indices.len() {
            // Cap the look-ahead at 3 integers to avoid quadratic behaviour
            // on long sequences.
            for gap in 1..=3 {
                if pair_idx + gap >= int_indices.len() {
                    break;
                }
                let i = int_indices[pair_idx];
                let j = int_indices[pair_idx + gap];
                if i >= self.current_nodes.len() || j >= self.current_nodes.len() {
                    break;
                }

                let (ic_i, v_i) = match (
                    self.current_nodes[i].kind.as_ref(),
                    &self.current_nodes[i].value,
                ) {
                    (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => (ic.clone(), v.clone()),
                    _ => unreachable!(
                        "int_indices is rebuilt on entry; kind-pun between iterations would have re-filtered i out"
                    ),
                };
                let v_j = match &self.current_nodes[j].value {
                    ChoiceValue::Integer(v) => v.clone(),
                    _ => unreachable!("kind/value mismatch: Integer kind with non-Integer value"),
                };

                // N10: cap k at the i-th element's distance from
                // `shrink_towards`. The sort_key score is U-shaped around
                // `shrink_towards`, so capping keeps `find_integer`'s probe
                // monotone; `validate()` (via `replace`) trims further if v_j's
                // constraints bind first. Direction is decided by the i-th
                // element (shortlex dominates on element 0): move it toward its
                // own shrink target, the j-th follows.
                let st_i = ic_i.clamped_shrink_towards();

                // Lower direction: run when v_i > st_i. Largest useful k is
                // `v_i - st_i` (the i-th's distance to st).
                if v_i > st_i {
                    let max_k = &v_i - &st_i;
                    find_integer_r(|n| {
                        let k = BigInt::from(n as u64);
                        if k > max_k {
                            return Ok(false);
                        }
                        let new_i = &v_i - &k;
                        let new_j = &v_j - &k;
                        self.replace_two(i, &new_i, j, &new_j)
                    })?;
                }

                // Raise direction: run when v_i < st_i. Largest useful k:
                // `st_i - v_i`.
                if v_i < st_i {
                    let max_k = &st_i - &v_i;
                    find_integer_r(|n| {
                        let k = BigInt::from(n as u64);
                        if k > max_k {
                            return Ok(false);
                        }
                        let new_i = &v_i + &k;
                        let new_j = &v_j + &k;
                        self.replace_two(i, &new_i, j, &new_j)
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Try shrinking duplicate integer values simultaneously.
    ///
    /// For each group of nodes sharing `(ChoiceKind discriminant,
    /// ChoiceValue)`, tries simultaneous shrinking — handling cases
    /// where two duplicates must remain equal (e.g. a list element and a
    /// separate value that must appear in the list).
    ///
    /// All five choice kinds participate: every group tries the
    /// kind-simplest replacement, and integer groups additionally drive
    /// a binary search across all members at once.
    pub(super) fn shrink_duplicates(&mut self) -> ShrinkResult<()> {
        // Group nodes by (kind discriminant, value).  The discriminant
        // gate keeps an Integer and a Bytes that happen to coexist with
        // the same numeric payload apart.
        //
        // `HashMap` iteration order is randomised, so we keep groups in
        // source-position order (by smallest index) before processing —
        // otherwise a `replace` that truncates `current_nodes` invalidates
        // later groups in seed-dependent ways and the shrinker converges
        // on neighbouring rather than canonical minima.
        let mut groups: HashMap<(std::mem::Discriminant<ChoiceKind>, ChoiceValue), Vec<usize>> =
            HashMap::new();
        for (i, node) in self.current_nodes.iter().enumerate() {
            let key = (
                std::mem::discriminant(node.kind.as_ref()),
                node.value.clone(),
            );
            groups.entry(key).or_default().push(i);
        }
        let mut ordered_groups: Vec<_> = groups.into_iter().collect();
        ordered_groups.sort_by_key(|(_, indices)| indices[0]);
        for ((kind_disc, group_value), indices) in ordered_groups.iter() {
            if indices.len() < 2 {
                continue;
            }
            // A prior group's `replace` may have truncated `current_nodes`
            // (the test function can return a shorter realised sequence).
            // Skip any indices that fell out of range, then make sure
            // enough members still match the original group's
            // (kind, value) before proposing a replacement.
            let valid: Vec<usize> = indices
                .iter()
                .copied()
                .filter(|&i| {
                    i < self.current_nodes.len()
                        && self.current_nodes[i].value == *group_value
                        && std::mem::discriminant(self.current_nodes[i].kind.as_ref()) == *kind_disc
                })
                .collect();
            if valid.len() < 2 {
                continue;
            }
            // Try the simplest-replacement step for every group.  For
            // boolean / float / bytes / string this is the main win; the
            // integer branch below adds a deeper binary search.
            let simplest = self.current_nodes[valid[0]].kind.simplest();
            if simplest != *group_value {
                let replacements: HashMap<usize, ChoiceValue> =
                    valid.iter().map(|&i| (i, simplest.clone())).collect();
                self.replace(&replacements)?;
            }
        }
        // The remainder of this function is the legacy integer-only
        // binary-search loop, kept verbatim so the existing tests still
        // pass.
        let mut groups: HashMap<BigInt, Vec<usize>> = HashMap::new();
        for (i, node) in self.current_nodes.iter().enumerate() {
            if let (ChoiceKind::Integer(_), ChoiceValue::Integer(v)) =
                (node.kind.as_ref(), &node.value)
            {
                groups.entry(v.clone()).or_default().push(i);
            }
        }
        // Iterate groups in source-position order; see the comment above
        // the first-half iteration for why HashMap randomisation matters.
        let mut ordered_groups: Vec<_> = groups.into_iter().collect();
        ordered_groups.sort_by_key(|(_, indices)| indices[0]);

        for (value, indices) in ordered_groups {
            if indices.len() < 2 {
                continue;
            }

            // Re-validate that all indices still have the same value.
            let valid: Vec<usize> = indices
                .iter()
                .copied()
                .filter(|&i| {
                    i < self.current_nodes.len()
                        && matches!(&self.current_nodes[i].value, ChoiceValue::Integer(v) if v.clone() == value)
                })
                .collect();

            if valid.len() < 2 {
                continue;
            }

            let ic = match self.current_nodes[valid[0]].kind.as_ref() {
                ChoiceKind::Integer(ic) => ic.clone(),
                _ => unreachable!(
                    "kind/value invariant violated: outer match guaranteed this variant"
                ),
            };

            // Try setting all to simplest simultaneously.
            let simplest = ic.simplest();
            if simplest != value {
                let replacements: HashMap<usize, ChoiceValue> = valid
                    .iter()
                    .map(|&i| (i, ChoiceValue::Integer(simplest.clone())))
                    .collect();
                self.replace(&replacements)?;
            }

            // Re-read current value after possible replacement.
            let cur_value = self.int_value_bigint(valid[0]);

            // Shift-right adaptive descent of all members in lockstep,
            // followed by shrink_by_multiples(2) and (1) to land on the
            // boundary. Each probe re-reads the current value of `valid[0]`
            // so the descent starts from the live shrink target.
            let valid_capture = valid.clone();
            let group_replace = |sh: &mut Shrinker<'_>, candidate: &BigInt| -> ShrinkResult<bool> {
                let current_valid: Vec<usize> = valid_capture
                    .iter()
                    .copied()
                    .filter(|&i| i < sh.current_nodes.len())
                    .collect();
                if current_valid.len() < 2 {
                    return Ok(false);
                }
                let replacements: HashMap<usize, ChoiceValue> = current_valid
                    .iter()
                    .map(|&i| (i, ChoiceValue::Integer(candidate.clone())))
                    .collect();
                sh.replace(&replacements)
            };
            let live_base = |sh: &Shrinker<'_>| -> BigInt {
                match &sh.current_nodes[valid_capture[0]].value {
                    ChoiceValue::Integer(v) => v.clone(),
                    _ => unreachable!("group filter only retains Integer-kind members"),
                }
            };
            if cur_value.sign() == Sign::Plus {
                let lo = ic.simplest().max(BigInt::from(0));
                let dist = &cur_value - &lo;
                if dist.sign() == Sign::Plus {
                    let max_shift = dist.bits() as usize + 1;
                    find_integer_r(|k| {
                        let candidate = &lo + (&dist >> k.min(max_shift));
                        group_replace(self, &candidate)
                    })?;
                }
                if live_base(self) > lo {
                    find_integer_r(|n| {
                        let attempt = live_base(self) - BigInt::from(2u64 * n as u64);
                        group_replace(self, &attempt)
                    })?;
                }
                if live_base(self) > lo {
                    find_integer_r(|n| {
                        let attempt = live_base(self) - BigInt::from(n as u64);
                        group_replace(self, &attempt)
                    })?;
                }
            } else if cur_value.sign() == Sign::Minus {
                let lo = (-ic.simplest()).max(BigInt::from(0));
                let dist = ((-&cur_value) - &lo).max(BigInt::from(0));
                if dist.sign() == Sign::Plus {
                    let max_shift = dist.bits() as usize + 1;
                    find_integer_r(|k| {
                        let candidate_abs = &lo + (&dist >> k.min(max_shift));
                        group_replace(self, &(-&candidate_abs))
                    })?;
                }
                let neg_hi = -&lo;
                if live_base(self) < neg_hi {
                    find_integer_r(|n| {
                        let attempt = live_base(self) + BigInt::from(2u64 * n as u64);
                        group_replace(self, &attempt)
                    })?;
                }
                if live_base(self) < neg_hi {
                    find_integer_r(|n| {
                        let attempt = live_base(self) + BigInt::from(n as u64);
                        group_replace(self, &attempt)
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Break the zig-zag trap by lowering a common offset across every
    /// integer node that's changed since the last checkpoint.
    ///
    /// When two integers `m, n` are linked by a predicate like
    /// `abs(m - n) > 1`, the individual minimization passes can only
    /// step each toward `shrink_towards` by one before the predicate
    /// flips. This pass observes that *all* changed integer nodes shrank by
    /// some non-zero common offset, and tries to lower that offset directly
    /// using a `find_integer` exponential probe.
    ///
    /// Always called after a successful pass that may have changed
    /// integer values; clears the change-tracking set on exit.
    pub(crate) fn lower_common_node_offset(&mut self) -> ShrinkResult<()> {
        let mut changed: Vec<usize> = self.changed_nodes().iter().copied().collect();
        // `changed_nodes` is a `HashSet`; sort for a deterministic, run-to-run
        // stable iteration order.
        changed.sort_unstable();
        if changed.len() <= 1 {
            return Ok(());
        }
        let mut indices: Vec<usize> = Vec::new();
        let mut ic_targets: Vec<BigInt> = Vec::new();
        let mut distances: Vec<BigInt> = Vec::new();
        for &i in &changed {
            // `changed` came from `update_change_tracking`, which only
            // populates indices < current_nodes.len().
            hegel_internal_debug_assert!(i < self.current_nodes.len());
            let (target, v) = match (
                self.current_nodes[i].kind.as_ref(),
                &self.current_nodes[i].value,
            ) {
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => {
                    (ic.clamped_shrink_towards(), v.clone())
                }
                _ => continue,
            };
            if v == target {
                // Already trivial; can't offset further.
                continue;
            }
            distances.push((&v - &target).abs());
            indices.push(i);
            ic_targets.push(target);
        }
        if indices.len() <= 1 {
            return Ok(());
        }
        let offset = distances
            .iter()
            .min()
            .expect("non-empty by check above")
            .clone();
        // `offset > 0`: every entry in `distances` came from a `v != target`
        // node (the loop above skips equal entries), so all are strictly
        // positive.
        hegel_internal_debug_assert!(offset.sign() == Sign::Plus);
        // residual[k] = distance[k] - offset; the "common offset" portion is
        // what we'll try to drive toward zero.
        let residual: Vec<BigInt> = distances.iter().map(|d| d - &offset).collect();

        // The predicate signs are deduced from the sign of `(v - target)` for
        // each node. Shrink the offset in both directions to handle the case
        // where absolute distances are equal but signs differ.
        let signs: Vec<i128> = indices
            .iter()
            .zip(ic_targets.iter())
            .map(|(&i, target)| {
                let v = match &self.current_nodes[i].value {
                    ChoiceValue::Integer(v) => v.clone(),
                    _ => unreachable!(
                        "indices/ic_targets came from the integer-node filter above; \
                         ChoiceNode invariant pairs Integer kind with Integer value"
                    ),
                };
                if &v >= target { 1 } else { -1 }
            })
            .collect();

        // Try lowering by an additional `n` units in both directions.
        for sign_multiplier in [1i128, -1] {
            find_integer_r(|n| {
                let n_big = BigInt::from(n as u64);
                if n_big > offset {
                    return Ok(false);
                }
                let new_offset = &offset - &n_big;
                let mut replacements: HashMap<usize, ChoiceValue> = HashMap::new();
                for k in 0..indices.len() {
                    let new_distance = &new_offset + &residual[k];
                    let effective_sign = signs[k] * sign_multiplier;
                    let new_value = if effective_sign >= 0 {
                        &ic_targets[k] + &new_distance
                    } else {
                        &ic_targets[k] - &new_distance
                    };
                    replacements.insert(indices[k], ChoiceValue::Integer(new_value));
                }
                self.replace(&replacements)
            })?;
        }
        self.clear_change_tracking();
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_lower_common_node_offset_tests.rs"]
mod lower_common_node_offset_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_minimize_duplicated_choices_tests.rs"]
mod minimize_duplicated_choices_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_integers_tests.rs"]
mod integers_tests;

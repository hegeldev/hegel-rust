use crate::native::HashMap;

use crate::native::bignum::{BigInt, Sign, Signed};
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceData, ChoiceValue};

use super::search::{BinSearchDownBig, FindInteger};
use super::{ShrinkResult, Shrinker};
use crate::control::hegel_internal_debug_assert;

/// The low `keep` bits of the non-negative `v`, i.e. `v mod 2^keep`.
fn low_bits(v: &BigInt, keep: usize) -> BigInt {
    v - &BigInt::from((v >> keep).magnitude() << keep)
}

impl<'a> Shrinker<'a> {
    /// Current integer value at node `i`, or `None` when the node is not
    /// (or no longer) an integer — a concurrent shrink can pun the kind at
    /// any position between probes.
    pub(super) fn int_value_bigint(&self, i: usize) -> Option<BigInt> {
        let (_, v) = self.current_nodes.get(i)?.data.as_integer()?;
        Some(v.clone())
    }

    /// Attempt to replace node `i` with `candidate`. The candidate is handed to
    /// [`Shrinker::replace`], which range-checks it against the node's
    /// constraint (rejecting out-of-range candidates), so this stays correct
    /// for any node width.
    pub(super) async fn replace_int(&mut self, i: usize, candidate: &BigInt) -> ShrinkResult<bool> {
        self.replace(&HashMap::from_iter([(
            i,
            ChoiceValue::Integer(candidate.clone()),
        )]))
        .await
    }

    /// Attempt to replace two integer nodes simultaneously; `replace`
    /// range-checks each candidate.
    pub(super) async fn replace_two(
        &mut self,
        i: usize,
        vi: &BigInt,
        j: usize,
        vj: &BigInt,
    ) -> ShrinkResult<bool> {
        self.replace(&HashMap::from_iter([
            (i, ChoiceValue::Integer(vi.clone())),
            (j, ChoiceValue::Integer(vj.clone())),
        ]))
        .await
    }

    /// Replace blocks of choices with their simplest values.
    pub(super) async fn zero_choices(&mut self) -> ShrinkResult<()> {
        let mut k = self.current_nodes.len();
        while k > 0 {
            let mut i = 0;
            while i + k <= self.current_nodes.len() {
                if self.current_nodes[i].data.is_simplest() {
                    i += 1;
                } else {
                    let replacements: HashMap<usize, ChoiceValue> = (i..i + k)
                        .map(|j| (j, self.current_nodes[j].data.simplest_value()))
                        .collect();
                    self.replace(&replacements).await?;
                    i += k;
                }
            }
            k /= 2;
        }
        Ok(())
    }

    /// For integer choices: try simplest, then flip negative to positive.
    pub(super) async fn swap_integer_sign(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            if let ChoiceData::Integer(ic, v) = &self.current_nodes[i].data {
                let v = v.clone();
                let simplest = ic.simplest();
                if v != simplest {
                    self.replace(&HashMap::from_iter([(i, ChoiceValue::Integer(simplest))]))
                        .await?;
                }
                if let Some(v) = self.int_value_bigint(i) {
                    if v.sign() == Sign::Minus {
                        self.replace_int(i, &(-&v)).await?;
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
    pub(super) async fn binary_search_integer_towards_zero(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            self.binary_search_node_towards_zero(i).await?;
            i += 1;
        }
        Ok(())
    }

    async fn binary_search_node_towards_zero(&mut self, i: usize) -> ShrinkResult<()> {
        let Some((ic, _)) = self.current_nodes[i].data.as_integer() else {
            return Ok(());
        };
        let ic = ic.clone();
        let target = ic.clamped_shrink_towards();

        self.try_at_distance(i, &ic, &target, &BigInt::from(0))
            .await?;
        self.try_at_distance(i, &ic, &target, &BigInt::from(1))
            .await?;

        let Some(base) = self.distance_from(i, &target) else {
            return Ok(());
        };
        let n_bits = base.bits();
        let mut search = FindInteger::new();
        while let Some(k) = search.probe() {
            let ok = if k as u64 >= n_bits {
                false
            } else {
                let keep = (n_bits - k as u64) as usize;
                let masked = low_bits(&base, keep);
                self.try_at_distance(i, &ic, &target, &masked).await?
            };
            search.record(ok);
        }

        let Some(base) = self.distance_from(i, &target) else {
            return Ok(());
        };
        if base.bits() > 8 {
            let top = &base >> (base.bits() as usize - 8);
            self.try_at_distance(i, &ic, &target, &top).await?;
            let bottom = low_bits(&base, 8);
            self.try_at_distance(i, &ic, &target, &bottom).await?;
        }

        loop {
            let Some(before) = self.distance_from(i, &target) else {
                return Ok(());
            };
            if before == BigInt::from(0) {
                break;
            }
            let max_shift = before.bits() as usize + 1;
            let mut search = FindInteger::new();
            while let Some(k) = search.probe() {
                let candidate = &before >> k.min(max_shift);
                let ok = self.try_at_distance(i, &ic, &target, &candidate).await?;
                search.record(ok);
            }
            for step in [2u64, 1] {
                let Some(base) = self.distance_from(i, &target) else {
                    return Ok(());
                };
                let mut search = FindInteger::new();
                while let Some(n) = search.probe() {
                    let sub = BigInt::from(step) * BigInt::from(n as u64);
                    let ok = if sub > base {
                        false
                    } else {
                        self.try_at_distance(i, &ic, &target, &(&base - &sub))
                            .await?
                    };
                    search.record(ok);
                }
            }
            if self.distance_from(i, &target) == Some(before) {
                break;
            }
        }
        Ok(())
    }

    /// `|value(i) - target|` as a non-negative `BigInt`, or `None` when
    /// node `i` is no longer an integer.
    fn distance_from(&self, i: usize, target: &BigInt) -> Option<BigInt> {
        let v = self.int_value_bigint(i)?;
        Some(BigInt::from((&v - target).magnitude()))
    }

    /// Probe node `i` at `target + d`, then — when that is rejected — at
    /// `target - d`. The sort key orders equal distances above-first, so the
    /// above side is always offered first.
    async fn try_at_distance(
        &mut self,
        i: usize,
        ic: &IntegerChoice,
        target: &BigInt,
        d: &BigInt,
    ) -> ShrinkResult<bool> {
        let above = target + d;
        let mut accepted = false;
        if ic.validate(&above) {
            accepted = self.replace_int(i, &above).await?;
        }
        if !accepted && d.sign() == Sign::Plus {
            let below = target - d;
            if ic.validate(&below) {
                accepted = self.replace_int(i, &below).await?;
            }
        }
        Ok(accepted)
    }

    /// The integer nodes of the current shrink target: index, constraint,
    /// and value, snapshotted together so pair passes work from a proven
    /// view instead of re-matching each node.
    fn integer_entries(&self) -> Vec<(usize, std::sync::Arc<IntegerChoice>, BigInt)> {
        self.current_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| match &n.data {
                ChoiceData::Integer(ic, v) => Some((i, std::sync::Arc::clone(ic), v.clone())),
                _ => None,
            })
            .collect()
    }

    /// Try redistributing value between pairs of integer choices.
    ///
    /// For each pair of integer nodes at various distances, tries moving
    /// value from i to j (or vice versa) while keeping the total sum
    /// constant. Useful for sum-type constraints where the minimal
    /// counterexample has one small and one large value.
    pub(super) async fn redistribute_integers(&mut self) -> ShrinkResult<()> {
        let n = self.integer_entries().len();

        let max_gap = 8.min(n);
        for gap in 1..max_gap {
            let mut pair_idx = n.saturating_sub(gap + 1);
            loop {
                let current_ints = self.integer_entries();

                if pair_idx + gap >= current_ints.len() {
                    if pair_idx == 0 {
                        break;
                    }
                    pair_idx -= 1;
                    continue;
                }

                let (i, ic_i, prev_i) = current_ints[pair_idx].clone();
                let (j, _, prev_j) = current_ints[pair_idx + gap].clone();
                let target_i = ic_i.clamped_shrink_towards();

                let prev_dist = BigInt::from((&prev_i - &target_i).magnitude());
                if prev_dist.sign() == Sign::Plus {
                    let on_low_side = prev_i < target_i;
                    let mut search = BinSearchDownBig::new(BigInt::from(0), prev_dist.clone());
                    while let Some(d) = search.probe() {
                        let new_i = if on_low_side {
                            &target_i - &d
                        } else {
                            &target_i + &d
                        };
                        let new_j = &prev_j + (&prev_i - &new_i);
                        let ok = self.replace_two(i, &new_i, j, &new_j).await?;
                        search.record(ok);
                    }
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
    pub(super) async fn lower_integers_together(&mut self) -> ShrinkResult<()> {
        let mut pair_idx = 0;
        loop {
            for gap in 1..=3 {
                let int_entries = self.integer_entries();
                if pair_idx >= int_entries.len() {
                    return Ok(());
                }
                if pair_idx + gap >= int_entries.len() {
                    break;
                }
                let (i, ic_i, v_i) = int_entries[pair_idx].clone();
                let (j, _, v_j) = int_entries[pair_idx + gap].clone();

                let st_i = ic_i.clamped_shrink_towards();

                if v_i > st_i {
                    let max_k = &v_i - &st_i;
                    let mut search = FindInteger::new();
                    while let Some(n) = search.probe() {
                        let k = BigInt::from(n as u64);
                        let ok = if k > max_k {
                            false
                        } else {
                            let new_i = &v_i - &k;
                            let new_j = &v_j - &k;
                            self.replace_two(i, &new_i, j, &new_j).await?
                        };
                        search.record(ok);
                    }
                }

                if v_i < st_i {
                    let max_k = &st_i - &v_i;
                    let mut search = FindInteger::new();
                    while let Some(n) = search.probe() {
                        let k = BigInt::from(n as u64);
                        let ok = if k > max_k {
                            false
                        } else {
                            let new_i = &v_i + &k;
                            let new_j = &v_j + &k;
                            self.replace_two(i, &new_i, j, &new_j).await?
                        };
                        search.record(ok);
                    }
                }
            }
            pair_idx += 1;
        }
    }

    /// Try shrinking duplicate integer values simultaneously.
    ///
    /// For each group of nodes sharing `(ChoiceData discriminant,
    /// ChoiceValue)`, tries simultaneous shrinking — handling cases
    /// where two duplicates must remain equal (e.g. a list element and a
    /// separate value that must appear in the list).
    ///
    /// All five choice kinds participate: every group tries the
    /// kind-simplest replacement, and integer groups additionally drive
    /// a binary search across all members at once.
    pub(super) async fn shrink_duplicates(&mut self) -> ShrinkResult<()> {
        async fn group_replace(
            sh: &mut Shrinker<'_>,
            valid: &[usize],
            candidate: &BigInt,
        ) -> ShrinkResult<bool> {
            let current_valid: Vec<usize> = valid
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
            sh.replace(&replacements).await
        }

        let mut groups: HashMap<(std::mem::Discriminant<ChoiceData>, ChoiceValue), Vec<usize>> =
            HashMap::default();
        for (i, node) in self.current_nodes.iter().enumerate() {
            let key = (std::mem::discriminant(&node.data), node.value());
            groups.entry(key).or_default().push(i);
        }
        let mut ordered_groups: Vec<_> = groups.into_iter().collect();
        ordered_groups.sort_by_key(|(_, indices)| indices[0]);
        for ((kind_disc, group_value), indices) in ordered_groups.iter() {
            if indices.len() < 2 {
                continue;
            }
            let valid: Vec<usize> = indices
                .iter()
                .copied()
                .filter(|&i| {
                    i < self.current_nodes.len()
                        && self.current_nodes[i].data.value_ref() == *group_value
                        && std::mem::discriminant(&self.current_nodes[i].data) == *kind_disc
                })
                .collect();
            if valid.len() < 2 {
                continue;
            }
            let simplest = self.current_nodes[valid[0]].data.simplest_value();
            if simplest != *group_value {
                let replacements: HashMap<usize, ChoiceValue> =
                    valid.iter().map(|&i| (i, simplest.clone())).collect();
                self.replace(&replacements).await?;
            }
        }
        let mut groups: HashMap<BigInt, Vec<usize>> = HashMap::default();
        for (i, node) in self.current_nodes.iter().enumerate() {
            if let Some((_, v)) = node.data.as_integer() {
                groups.entry(v.clone()).or_default().push(i);
            }
        }
        let mut ordered_groups: Vec<_> = groups.into_iter().collect();
        ordered_groups.sort_by_key(|(_, indices)| indices[0]);

        for (value, indices) in ordered_groups {
            if indices.len() < 2 {
                continue;
            }

            let members: Vec<(usize, std::sync::Arc<IntegerChoice>)> = indices
                .iter()
                .filter_map(|&i| match self.current_nodes.get(i).map(|n| &n.data) {
                    Some(ChoiceData::Integer(ic, v)) if *v == value => {
                        Some((i, std::sync::Arc::clone(ic)))
                    }
                    _ => None,
                })
                .collect();

            if members.len() < 2 {
                continue;
            }
            let valid: Vec<usize> = members.iter().map(|&(i, _)| i).collect();
            let ic = std::sync::Arc::clone(&members[0].1);

            let simplest = ic.simplest();
            if simplest != value {
                let replacements: HashMap<usize, ChoiceValue> = valid
                    .iter()
                    .map(|&i| (i, ChoiceValue::Integer(simplest.clone())))
                    .collect();
                self.replace(&replacements).await?;
            }

            let live_base = |sh: &Shrinker<'_>| -> Option<BigInt> { sh.int_value_bigint(valid[0]) };
            let Some(cur_value) = live_base(self) else {
                continue;
            };
            if cur_value.sign() == Sign::Plus {
                let lo = ic.simplest().max(BigInt::from(0));
                let dist = &cur_value - &lo;
                if dist.sign() == Sign::Plus {
                    let max_shift = dist.bits() as usize + 1;
                    let mut search = FindInteger::new();
                    while let Some(k) = search.probe() {
                        let candidate = &lo + (&dist >> k.min(max_shift));
                        let ok = group_replace(self, &valid, &candidate).await?;
                        search.record(ok);
                    }
                }
                if live_base(self).is_some_and(|b| b > lo) {
                    let mut search = FindInteger::new();
                    while let Some(n) = search.probe() {
                        let Some(base) = live_base(self) else { break };
                        let attempt = base - BigInt::from(2u128 * n as u128);
                        let ok = group_replace(self, &valid, &attempt).await?;
                        search.record(ok);
                    }
                }
                if live_base(self).is_some_and(|b| b > lo) {
                    let mut search = FindInteger::new();
                    while let Some(n) = search.probe() {
                        let Some(base) = live_base(self) else { break };
                        let attempt = base - BigInt::from(n as u64);
                        let ok = group_replace(self, &valid, &attempt).await?;
                        search.record(ok);
                    }
                }
            } else if cur_value.sign() == Sign::Minus {
                let lo = (-ic.simplest()).max(BigInt::from(0));
                let dist = ((-&cur_value) - &lo).max(BigInt::from(0));
                if dist.sign() == Sign::Plus {
                    let max_shift = dist.bits() as usize + 1;
                    let mut search = FindInteger::new();
                    while let Some(k) = search.probe() {
                        let candidate_abs = &lo + (&dist >> k.min(max_shift));
                        let ok = group_replace(self, &valid, &(-&candidate_abs)).await?;
                        search.record(ok);
                    }
                }
                let neg_hi = -&lo;
                if live_base(self).is_some_and(|b| b < neg_hi) {
                    let mut search = FindInteger::new();
                    while let Some(n) = search.probe() {
                        let Some(base) = live_base(self) else { break };
                        let attempt = base + BigInt::from(2u128 * n as u128);
                        let ok = group_replace(self, &valid, &attempt).await?;
                        search.record(ok);
                    }
                }
                if live_base(self).is_some_and(|b| b < neg_hi) {
                    let mut search = FindInteger::new();
                    while let Some(n) = search.probe() {
                        let Some(base) = live_base(self) else { break };
                        let attempt = base + BigInt::from(n as u64);
                        let ok = group_replace(self, &valid, &attempt).await?;
                        search.record(ok);
                    }
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
    pub(crate) async fn lower_common_node_offset(&mut self) -> ShrinkResult<()> {
        let mut changed: Vec<usize> = self.changed_nodes().iter().copied().collect();
        changed.sort_unstable();
        if changed.len() <= 1 {
            return Ok(());
        }
        let mut indices: Vec<usize> = Vec::new();
        let mut ic_targets: Vec<BigInt> = Vec::new();
        let mut distances: Vec<BigInt> = Vec::new();
        let mut signs: Vec<i128> = Vec::new();
        for &i in &changed {
            hegel_internal_debug_assert!(i < self.current_nodes.len());
            let Some((ic, v)) = self.current_nodes[i].data.as_integer() else {
                continue;
            };
            let target = ic.clamped_shrink_towards();
            let v = v.clone();
            if v == target {
                continue;
            }
            distances.push((&v - &target).abs());
            signs.push(if v >= target { 1 } else { -1 });
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
        hegel_internal_debug_assert!(offset.sign() == Sign::Plus);
        let residual: Vec<BigInt> = distances.iter().map(|d| d - &offset).collect();

        for sign_multiplier in [1i128, -1] {
            let mut search = FindInteger::new();
            while let Some(n) = search.probe() {
                let n_big = BigInt::from(n as u64);
                let ok = if n_big > offset {
                    false
                } else {
                    let new_offset = &offset - &n_big;
                    let mut replacements: HashMap<usize, ChoiceValue> = HashMap::default();
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
                    self.replace(&replacements).await?
                };
                search.record(ok);
            }
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

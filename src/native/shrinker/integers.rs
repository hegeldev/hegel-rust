// Integer-based shrink passes: zero_choices, swap_integer_sign,
// binary_search_integer_towards_zero, redistribute_integers, shrink_duplicates,
// lower_common_node_offset.

use std::collections::HashMap;

use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceValue};

use super::{Shrinker, bin_search_down, find_integer};

/// Saturating conversion of a `BigInt` to `i128`: values beyond the `i128`
/// range clamp to the nearest endpoint. The specialized integer-shrinking
/// passes below run their arithmetic in `i128` (for speed and to preserve the
/// exact pre-existing shrink behaviour); they consult this only for *bounds*,
/// where a saturated endpoint is a safe over-approximation — `replace()`
/// re-validates every candidate against the true `BigInt` bounds, and every
/// candidate they propose is itself an `i128`, hence in range.
fn saturate_i128(b: &BigInt) -> i128 {
    i128::try_from(b).unwrap_or(if *b < BigInt::zero() {
        i128::MIN
    } else {
        i128::MAX
    })
}

/// The `i128` view of an integer choice's bounds used by the specialized
/// passes.
struct IntView {
    simplest: i128,
    min_value: i128,
    max_value: i128,
}

impl IntView {
    fn validate(&self, x: i128) -> bool {
        self.min_value <= x && x <= self.max_value
    }
}

/// Extract `(value, bounds)` as `i128` for an integer node, or `None` when the
/// *value itself* exceeds `i128` — those genuinely-huge values are left to the
/// generic index-based passes (`zero_choices`, `lower_and_bump`, …).
fn int_view(ic: &IntegerChoice, value: &BigInt) -> Option<(i128, IntView)> {
    let value = i128::try_from(value).ok()?;
    Some((
        value,
        IntView {
            simplest: saturate_i128(&ic.simplest()),
            min_value: saturate_i128(&ic.min_value),
            max_value: saturate_i128(&ic.max_value),
        },
    ))
}

impl<'a> Shrinker<'a> {
    /// The current integer value at node `i`. Panics if node `i` isn't an
    /// integer node — callers in the integer passes have just matched one.
    fn integer_value_at(&self, i: usize) -> &BigInt {
        match &self.current_nodes[i].value {
            ChoiceValue::Integer(v) => v,
            _ => unreachable!("integer pass operates only on integer nodes"),
        }
    }

    /// Replace blocks of choices with their simplest values.
    pub(super) fn zero_choices(&mut self) {
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
                    self.replace(&replacements);
                    i += k;
                }
            }
            k /= 2;
        }
    }

    /// For integer choices: try simplest, then flip negative to positive.
    pub(super) fn swap_integer_sign(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = &self.current_nodes[i];
            if let (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) = (&node.kind, &node.value) {
                if let Some((v, view)) = int_view(ic, v) {
                    if v != view.simplest {
                        self.replace(&HashMap::from([(
                            i,
                            ChoiceValue::Integer(BigInt::from(view.simplest)),
                        )]));
                    }
                    // Re-read in case the replace changed things
                    if i < self.current_nodes.len() {
                        if let (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) =
                            (&self.current_nodes[i].kind, &self.current_nodes[i].value)
                        {
                            if let Some((v, view)) = int_view(ic, v) {
                                if v < 0 && view.validate(-v) {
                                    self.replace(&HashMap::from([(
                                        i,
                                        ChoiceValue::Integer(BigInt::from(-v)),
                                    )]));
                                }
                            }
                        }
                    }
                }
            }
            i += 1;
        }
    }

    /// Binary search integer values toward zero.
    ///
    /// Includes a linear scan of small values after binary search to
    /// handle non-monotonic functions (e.g. sampled_from or test functions
    /// that panic on boundary values).
    pub(super) fn binary_search_integer_towards_zero(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = &self.current_nodes[i];
            if let (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) = (&node.kind, &node.value) {
                let Some((v, view)) = int_view(ic, v) else {
                    i += 1;
                    continue;
                };
                if v > 0 {
                    let lo = view.simplest.max(0);
                    // shift_right adaptive descent. Probes
                    // `lo + (v - lo) >> k` for k = 1, 2, 4, 8, ... via
                    // `find_integer`, which is O(log log distance) rather
                    // than the O(log distance) of a full
                    // `bin_search_down`. For distance 10^15 that's
                    // ~7 probes vs ~50.
                    let dist = (v - lo) as u128;
                    if dist > 0 {
                        find_integer(|k| {
                            let shifted = (dist >> k.min(127)) as i128;
                            // dist is u128 ≥ 0, so shifted ≥ 0 and
                            // lo + shifted ≥ lo unconditionally — no
                            // out-of-range guard needed.
                            let candidate = lo + shifted;
                            self.replace(&HashMap::from([(
                                i,
                                ChoiceValue::Integer(BigInt::from(candidate)),
                            )]))
                        });
                    }
                    // Linear scan small values for non-monotonic functions.
                    let range_size = view
                        .max_value
                        .saturating_sub(view.min_value)
                        .saturating_add(1);
                    let scan_count = if range_size <= 128 {
                        range_size.min(32)
                    } else {
                        8
                    };
                    let cur_v = match &self.current_nodes[i].value {
                        ChoiceValue::Integer(v) => saturate_i128(v),
                        _ => unreachable!("integer pass operates only on integer nodes"),
                    };
                    for c in lo..lo.saturating_add(scan_count).min(cur_v) {
                        if !self
                            .replace(&HashMap::from([(i, ChoiceValue::Integer(BigInt::from(c)))]))
                        {
                            // Continue scanning even if not successful
                        }
                    }
                    // shrink_by_multiples(2) / (1): with a non-monotonic
                    // predicate (e.g. `|m - n| == 1`), pure bin_search_down
                    // converges to the current value without ever probing
                    // `cur - 2`. Hitting `cur - 2` is what lets the
                    // shrinker flip a linked pair from `(m, m+1)` down to
                    // `(m, m-1)` at the cost of one extra probe.
                    let base = match &self.current_nodes[i].value {
                        ChoiceValue::Integer(v) => saturate_i128(v),
                        _ => unreachable!("integer pass operates only on integer nodes"),
                    };
                    if base > lo {
                        find_integer(|n| {
                            let attempt = base - 2 * (n as i128);
                            if attempt < lo {
                                return false;
                            }
                            self.replace(&HashMap::from([(
                                i,
                                ChoiceValue::Integer(BigInt::from(attempt)),
                            )]))
                        });
                    }
                    let base = match &self.current_nodes[i].value {
                        ChoiceValue::Integer(v) => saturate_i128(v),
                        _ => unreachable!("integer pass operates only on integer nodes"),
                    };
                    if base > lo {
                        find_integer(|n| {
                            let attempt = base - (n as i128);
                            if attempt < lo {
                                return false;
                            }
                            self.replace(&HashMap::from([(
                                i,
                                ChoiceValue::Integer(BigInt::from(attempt)),
                            )]))
                        });
                    }
                    // Also try negative values with smaller absolute value (simpler).
                    if view.min_value < 0 {
                        let cur_v = match &self.current_nodes[i].value {
                            ChoiceValue::Integer(v) => saturate_i128(v),
                            _ => unreachable!("integer pass operates only on integer nodes"),
                        };
                        if cur_v > 0 {
                            let upper = (cur_v - 1).min(view.min_value.saturating_neg());
                            if upper >= 1 {
                                // Seed at -upper, then shift-right-descend the
                                // absolute value toward 1 via find_integer.
                                self.replace(&HashMap::from([(
                                    i,
                                    ChoiceValue::Integer(BigInt::from(-upper)),
                                )]));
                                let dist = (upper - 1) as u128;
                                if dist > 0 {
                                    find_integer(|k| {
                                        let shifted = (dist >> k.min(127)) as i128;
                                        let candidate_abs = 1 + shifted;
                                        if candidate_abs < 1 {
                                            return false;
                                        }
                                        self.replace(&HashMap::from([(
                                            i,
                                            ChoiceValue::Integer(BigInt::from(-candidate_abs)),
                                        )]))
                                    });
                                }
                            }
                        }
                    }
                } else if v < 0 {
                    // Mirror of the positive branch. `lo` is the
                    // absolute value of the simplest (clamped to 0
                    // below) and we shrink toward `lo` from `-v`
                    // before flipping the sign back.
                    let lo = view.simplest.min(0).saturating_abs();
                    let dist = ((-v) as u128).saturating_sub(lo as u128);
                    if dist > 0 {
                        find_integer(|k| {
                            let shifted = (dist >> k.min(127)) as i128;
                            // Same monotonicity argument as the positive
                            // branch: shifted ≥ 0, so lo + shifted ≥ lo.
                            let candidate_abs = lo + shifted;
                            self.replace(&HashMap::from([(
                                i,
                                ChoiceValue::Integer(BigInt::from(-candidate_abs)),
                            )]))
                        });
                    }
                    // Linear scan small negative values for non-monotonic functions.
                    let range_size = view
                        .max_value
                        .saturating_sub(view.min_value)
                        .saturating_add(1);
                    let neg_scan = if range_size <= 128 { (-v).min(32) } else { 8 };
                    for c in 1..neg_scan {
                        self.replace(&HashMap::from([(
                            i,
                            ChoiceValue::Integer(BigInt::from(-c)),
                        )]));
                    }
                    // shrink_by_multiples for the negative branch: probe
                    // `cur + 2*n` / `cur + n` (moving toward zero). Mirror
                    // of the positive-side block above.
                    let base = match &self.current_nodes[i].value {
                        ChoiceValue::Integer(v) => saturate_i128(v),
                        _ => unreachable!("integer pass operates only on integer nodes"),
                    };
                    let neg_hi = -lo;
                    if base < neg_hi {
                        find_integer(|n| {
                            let attempt = base + 2 * (n as i128);
                            if attempt > neg_hi {
                                return false;
                            }
                            self.replace(&HashMap::from([(
                                i,
                                ChoiceValue::Integer(BigInt::from(attempt)),
                            )]))
                        });
                    }
                    let base = match &self.current_nodes[i].value {
                        ChoiceValue::Integer(v) => saturate_i128(v),
                        _ => unreachable!("integer pass operates only on integer nodes"),
                    };
                    if base < neg_hi {
                        find_integer(|n| {
                            let attempt = base + (n as i128);
                            if attempt > neg_hi {
                                return false;
                            }
                            self.replace(&HashMap::from([(
                                i,
                                ChoiceValue::Integer(BigInt::from(attempt)),
                            )]))
                        });
                    }
                    // Also try positive values with smaller absolute value (simpler).
                    if view.max_value > 0 {
                        let cur_v = match &self.current_nodes[i].value {
                            ChoiceValue::Integer(v) => saturate_i128(v),
                            _ => unreachable!("integer pass operates only on integer nodes"),
                        };
                        if cur_v < 0 {
                            let upper = (-cur_v - 1).min(view.max_value);
                            if upper >= 1 {
                                // Seed at +upper, then shift-right-descend
                                // toward lo_pos via find_integer.
                                self.replace(&HashMap::from([(
                                    i,
                                    ChoiceValue::Integer(BigInt::from(upper)),
                                )]));
                                let lo_pos = view.simplest.max(0);
                                let dist = (upper - lo_pos) as u128;
                                if dist > 0 {
                                    find_integer(|k| {
                                        let shifted = (dist >> k.min(127)) as i128;
                                        let candidate = lo_pos + shifted;
                                        self.replace(&HashMap::from([(
                                            i,
                                            ChoiceValue::Integer(BigInt::from(candidate)),
                                        )]))
                                    });
                                }
                                // Linear scan positive values.
                                let scan_count = if range_size <= 128 {
                                    range_size.min(32)
                                } else {
                                    8
                                };
                                for c in lo_pos..lo_pos.saturating_add(scan_count).min(upper + 1) {
                                    self.replace(&HashMap::from([(
                                        i,
                                        ChoiceValue::Integer(BigInt::from(c)),
                                    )]));
                                }
                            }
                        }
                    }
                }
            }
            i += 1;
        }
    }

    /// Try redistributing value between pairs of integer choices.
    ///
    /// For each pair of integer nodes at various distances, tries moving
    /// value from i to j (or vice versa) while keeping the total sum
    /// constant. Useful for sum-type constraints where the minimal
    /// counterexample has one small and one large value.
    pub(super) fn redistribute_integers(&mut self) {
        let int_indices: Vec<usize> = self
            .current_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                if matches!(n.kind, ChoiceKind::Integer(_)) {
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
                        if matches!(node.kind, ChoiceKind::Integer(_)) {
                            Some(i)
                        } else {
                            None
                        }
                    })
                    .collect();

                // Defensive edge case: only reached when a prior
                // shrink removed enough integer nodes that
                // `pair_idx + gap` overshoots the new length.
                if pair_idx + gap >= current_ints.len() {
                    if pair_idx == 0 {
                        break;
                    }
                    pair_idx -= 1;
                    continue;
                }

                let i = current_ints[pair_idx];
                let j = current_ints[pair_idx + gap];

                // Redistribution arithmetic runs in i128; a pair whose value
                // exceeds i128 is left to the generic index passes.
                let (Ok(prev_i), Ok(prev_j)) = (
                    i128::try_from(self.integer_value_at(i)),
                    i128::try_from(self.integer_value_at(j)),
                ) else {
                    if pair_idx == 0 {
                        break;
                    }
                    pair_idx -= 1;
                    continue;
                };

                let simplest_i = match &self.current_nodes[i].kind {
                    ChoiceKind::Integer(ic) => saturate_i128(&ic.simplest()),
                    _ => unreachable!("integer index list only retains integer nodes"),
                };

                if prev_i != simplest_i {
                    if prev_i > 0 {
                        bin_search_down(0, prev_i, &mut |v| {
                            let delta = prev_i - v;
                            self.replace(&HashMap::from([
                                (i, ChoiceValue::Integer(BigInt::from(v))),
                                (j, ChoiceValue::Integer(BigInt::from(prev_j + delta))),
                            ]))
                        });
                    } else if prev_i < 0 {
                        bin_search_down(0, -prev_i, &mut |a| {
                            let delta = prev_i + a; // = -(|prev_i| - a)
                            self.replace(&HashMap::from([
                                (i, ChoiceValue::Integer(BigInt::from(-a))),
                                (j, ChoiceValue::Integer(BigInt::from(prev_j + delta))),
                            ]))
                        });
                    }
                }

                if pair_idx == 0 {
                    break;
                }
                pair_idx -= 1;
            }
        }
    }
    /// Lower pairs of nearby integer choices by the same amount
    /// simultaneously.
    ///
    /// The individual passes (`binary_search_integer_towards_zero`,
    /// `redistribute_integers`) walk each integer alone; when two values
    /// are pinned together by a predicate like `|m - n| == 1`, neither
    /// can move on its own without breaking the predicate, and the
    /// shrinker falls into a zig-zag trap that takes `O(m)` iterations
    /// to crawl down. By probing `(v_i - k, v_j - k)` for geometrically
    /// growing `k` via `find_integer`, this pass reaches the minimum in
    /// `O(log k)` probes.
    pub(super) fn lower_integers_together(&mut self) {
        let int_indices: Vec<usize> = self
            .current_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                if matches!(n.kind, ChoiceKind::Integer(_)) {
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

                // This pass moves a pair of nearby integers together in i128
                // space; a value beyond i128 is left to the generic index
                // passes.
                let (Ok(v_i), Ok(v_j)) = (
                    i128::try_from(self.integer_value_at(i)),
                    i128::try_from(self.integer_value_at(j)),
                ) else {
                    break;
                };

                // N10: cap k at the i-th element's distance from
                // `shrink_towards`. Pre-N10 each direction's `find_integer`
                // probe assumed a monotone predicate in k, but A21's
                // `shrink_towards`-aware sort_key turned the score
                // U-shaped: moving past `shrink_towards` makes sort_key
                // grow again. The exponential probe (5, 10, 20, …) then
                // jumped past the elbow and committed a worse-than-optimal
                // pair (e.g. `[-3, -2]` with `st=5` ended at `[7, 8]`
                // instead of `[5, 6]`).
                //
                // Why d_i (the i-th element's distance), not min/max of
                // both? Sort_key compares element-wise via shortlex; the
                // 0-th element (= sort_key of i-th node) dominates the
                // tuple comparison. Sort_key(v_i + k) is uniquely
                // minimised at k = st_i - v_i (raise) or k = v_i - st_i
                // (lower), where v_i lands exactly at st_i. Beyond that,
                // it grows. So the optimal k for the pair is d_i; capping
                // there keeps find_integer's predicate monotone, and
                // validate() trims further if v_j's constraints kick in
                // first.
                let st_i = match &self.current_nodes[i].kind {
                    ChoiceKind::Integer(ic) => saturate_i128(&ic.clamped_shrink_towards()),
                    _ => unreachable!("int_indices only retains integer nodes"),
                };

                // Direction is determined by the i-th element (shortlex
                // dominates on element 0): move it toward its own st.
                // The j-th element follows. If j is on the same side of
                // its st, joint motion is unambiguously better; if j is
                // on the opposite side, j's sort_key grows but i's gain
                // wins the shortlex comparison.

                // Lower direction: run when v_i > st_i. The largest
                // useful k is `v_i - st_i` (the i-th's distance to st).
                if v_i > st_i {
                    let max_k = v_i - st_i;
                    find_integer(|n| {
                        let k = n as i128;
                        if k > max_k {
                            return false;
                        }
                        let new_i = v_i - k;
                        let new_j = v_j - k;
                        // `replace` already calls `kind.validate`; the
                        // pre-check here is redundant, so let invalid
                        // candidates fall through to replace's check.
                        self.replace(&HashMap::from([
                            (i, ChoiceValue::Integer(BigInt::from(new_i))),
                            (j, ChoiceValue::Integer(BigInt::from(new_j))),
                        ]))
                    });
                }

                // Raise direction: run when v_i < st_i. Largest useful
                // k: `st_i - v_i`.
                if v_i < st_i {
                    let max_k = st_i - v_i;
                    find_integer(|n| {
                        let k = n as i128;
                        if k > max_k {
                            return false;
                        }
                        let new_i = v_i + k;
                        let new_j = v_j + k;
                        self.replace(&HashMap::from([
                            (i, ChoiceValue::Integer(BigInt::from(new_i))),
                            (j, ChoiceValue::Integer(BigInt::from(new_j))),
                        ]))
                    });
                }
            }
        }
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
    pub(super) fn shrink_duplicates(&mut self) {
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
            let key = (std::mem::discriminant(&node.kind), node.value.clone());
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
                        && std::mem::discriminant(&self.current_nodes[i].kind) == *kind_disc
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
                self.replace(&replacements);
            }
        }
        // The remainder of this function is the legacy integer-only
        // binary-search loop, kept verbatim so the existing tests still
        // pass; conceptually this work could move into the unified
        // `minimize_individual_choices` driver, but
        // `shrink_duplicates`' "step duplicates together" semantics
        // aren't covered there.
        // Group by i128 value. Members whose value exceeds i128 are skipped
        // here and handled by the generic index passes.
        let mut groups: HashMap<i128, Vec<usize>> = HashMap::new();
        for (i, node) in self.current_nodes.iter().enumerate() {
            if let (ChoiceKind::Integer(_), ChoiceValue::Integer(v)) = (&node.kind, &node.value) {
                if let Ok(vi) = i128::try_from(v) {
                    groups.entry(vi).or_default().push(i);
                }
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
                        && matches!(&self.current_nodes[i].value, ChoiceValue::Integer(v) if *v == value)
                })
                .collect();

            if valid.len() < 2 {
                continue;
            }

            let ChoiceKind::Integer(ic) = &self.current_nodes[valid[0]].kind else {
                unreachable!("kind/value invariant violated: outer match guaranteed this variant")
            };
            let ic = ic.clone();

            // Try setting all to simplest simultaneously.
            let simplest = ic.simplest();
            if simplest != value {
                let replacements: HashMap<usize, ChoiceValue> = valid
                    .iter()
                    .map(|&i| (i, ChoiceValue::Integer(simplest.clone())))
                    .collect();
                self.replace(&replacements);
            }

            // Re-read current value after possible replacement.
            let cur_value = saturate_i128(self.integer_value_at(valid[0]));

            // Shift-right adaptive descent of all members in lockstep,
            // followed by shrink_by_multiples(2) and (1) to land on the
            // boundary. Each probe re-reads the current value of
            // `valid[0]` so the descent starts from the live shrink
            // target — the previous bin_search_down captured the entry
            // value and stalled on the second probe because every member
            // had moved.
            let valid_capture = valid.clone();
            let group_replace = |sh: &mut Shrinker<'_>, candidate: i128| -> bool {
                let current_valid: Vec<usize> = valid_capture
                    .iter()
                    .copied()
                    .filter(|&i| i < sh.current_nodes.len())
                    .collect();
                if current_valid.len() < 2 {
                    return false;
                }
                let replacements: HashMap<usize, ChoiceValue> = current_valid
                    .iter()
                    .map(|&i| (i, ChoiceValue::Integer(BigInt::from(candidate))))
                    .collect();
                sh.replace(&replacements)
            };
            if cur_value > 0 {
                let lo = saturate_i128(&ic.simplest()).max(0);
                let dist = (cur_value - lo) as u128;
                if dist > 0 {
                    find_integer(|k| {
                        let shifted = (dist >> k.min(127)) as i128;
                        let candidate = lo + shifted;
                        group_replace(self, candidate)
                    });
                }
                let live_base = |sh: &Shrinker<'_>| -> i128 {
                    match &sh.current_nodes[valid_capture[0]].value {
                        ChoiceValue::Integer(v) => saturate_i128(v),
                        _ => unreachable!("group filter only retains Integer-kind members"),
                    }
                };
                if live_base(self) > lo {
                    find_integer(|n| {
                        let attempt = live_base(self).saturating_sub(2 * (n as i128));
                        group_replace(self, attempt)
                    });
                }
                if live_base(self) > lo {
                    find_integer(|n| {
                        let attempt = live_base(self).saturating_sub(n as i128);
                        group_replace(self, attempt)
                    });
                }
            } else if cur_value < 0 {
                let lo = saturate_i128(&ic.simplest()).min(0).saturating_abs();
                let v_abs = -cur_value;
                let dist = (v_abs - lo) as u128;
                if dist > 0 {
                    find_integer(|k| {
                        let shifted = (dist >> k.min(127)) as i128;
                        let candidate_abs = lo + shifted;
                        group_replace(self, -candidate_abs)
                    });
                }
                let live_base = |sh: &Shrinker<'_>| -> i128 {
                    match &sh.current_nodes[valid_capture[0]].value {
                        ChoiceValue::Integer(v) => saturate_i128(v),
                        _ => unreachable!("group filter only retains Integer-kind members"),
                    }
                };
                let neg_hi = -lo;
                if live_base(self) < neg_hi {
                    find_integer(|n| {
                        let attempt = live_base(self).saturating_add(2 * (n as i128));
                        group_replace(self, attempt)
                    });
                }
                if live_base(self) < neg_hi {
                    find_integer(|n| {
                        let attempt = live_base(self).saturating_add(n as i128);
                        group_replace(self, attempt)
                    });
                }
            }
        }
    }

    /// Break the zig-zag trap by lowering a common offset across every
    /// integer node that's changed since the last checkpoint.
    ///
    /// When two integers `m, n` are linked by a predicate like
    /// `abs(m - n) > 1`, the individual minimization passes can only
    /// step each toward `shrink_towards` by one before the predicate
    /// flips; the next iteration steps the other one by one; result:
    /// O(initial value) iterations to zig-zag toward zero.
    ///
    /// This pass observes that *all* changed integer nodes shrank by some
    /// non-zero common offset, and tries to lower that offset directly
    /// using a `find_integer` exponential probe — collapsing the zig-zag
    /// into O(log v) iterations. It probes both signs because the
    /// nodes may be sitting above or below their shrink targets.
    ///
    /// Always called after a successful pass that may have changed
    /// integer values; clears the change-tracking set on exit so the
    /// next round starts from the new shrink target.
    pub(crate) fn lower_common_node_offset(&mut self) {
        let changed: Vec<usize> = self.changed_nodes().iter().copied().collect();
        if changed.len() <= 1 {
            return;
        }
        let mut indices: Vec<usize> = Vec::new();
        let mut ic_targets: Vec<i128> = Vec::new();
        let mut distances: Vec<u128> = Vec::new();
        for &i in &changed {
            // `changed` came from `update_change_tracking`, which only
            // populates indices < current_nodes.len() (it clears the
            // set when the sequence shape changes).  A debug_assert
            // documents the invariant.
            debug_assert!(i < self.current_nodes.len());
            let node = &self.current_nodes[i];
            // Runs in i128; nodes whose value exceeds i128 are skipped (the
            // generic index passes handle them).
            let (v, target) = match (&node.kind, &node.value) {
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => {
                    let Ok(v) = i128::try_from(v) else { continue };
                    (v, saturate_i128(&ic.clamped_shrink_towards()))
                }
                _ => continue,
            };
            if v == target {
                // Already trivial; can't offset further.
                continue;
            }
            indices.push(i);
            ic_targets.push(target);
            distances.push(v.abs_diff(target));
        }
        if indices.len() <= 1 {
            return;
        }
        let offset = *distances.iter().min().expect("non-empty by check above");
        // `offset > 0`: every entry in `distances` came from `v.abs_diff(target)`
        // for `v != target` (the loop above skips equal entries), so all
        // distances are strictly positive.
        debug_assert!(offset > 0);
        // residual_v[k] = distance[k] - offset; the "common offset" portion
        // is what we'll try to drive toward zero.
        let residual: Vec<u128> = distances.iter().map(|d| d - offset).collect();

        // The predicate signs are deduced from the sign of `(v - target)`
        // for each node. Shrink the offset in both directions to handle
        // the case where the absolute distances are equal but the signs
        // differ.
        let signs: Vec<i128> = indices
            .iter()
            .zip(ic_targets.iter())
            .map(|(&i, &target)| {
                let v = match &self.current_nodes[i].value {
                    ChoiceValue::Integer(v) => saturate_i128(v),
                    _ => unreachable!(
                        "indices/ic_targets came from the integer-node filter above; \
                         ChoiceNode invariant pairs Integer kind with Integer value"
                    ),
                };
                if v >= target { 1 } else { -1 }
            })
            .collect();

        // Try lowering by an additional `n` units in both directions.
        // The candidate distance is `offset - n + residual`, applied with
        // the original signs. `find_integer` finds the maximum n.
        for sign_multiplier in [1i128, -1] {
            find_integer(|n| {
                if (n as u128) > offset {
                    return false;
                }
                let new_offset = offset - n as u128;
                let mut replacements: HashMap<usize, ChoiceValue> = HashMap::new();
                for k in 0..indices.len() {
                    let new_distance = new_offset + residual[k];
                    // `new_distance <= original_distances[k]` is guaranteed
                    // by the `(n as u128) > offset` check above:
                    // `new_distance = (offset - n) + residual[k]
                    //                = (distance[k] - n)
                    //                ≤ distance[k]`.
                    let effective_sign = signs[k] * sign_multiplier;
                    let new_value = if effective_sign >= 0 {
                        ic_targets[k].saturating_add_unsigned(new_distance)
                    } else {
                        ic_targets[k].saturating_sub_unsigned(new_distance)
                    };
                    replacements.insert(indices[k], ChoiceValue::Integer(BigInt::from(new_value)));
                }
                self.replace(&replacements)
            });
        }
        self.clear_change_tracking();
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_lower_common_node_offset_tests.rs"]
mod lower_common_node_offset_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_minimize_duplicated_choices_tests.rs"]
mod minimize_duplicated_choices_tests;

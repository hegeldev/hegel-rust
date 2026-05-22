// Integer-based shrink passes: zero_choices, swap_integer_sign,
// binary_search_integer_towards_zero, redistribute_integers, shrink_duplicates,
// lower_common_node_offset.

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceValue};

use super::{Shrinker, bin_search_down, find_integer};

impl<'a> Shrinker<'a> {
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
                let v = *v;
                if v != ic.simplest() {
                    self.replace(&HashMap::from([(i, ChoiceValue::Integer(ic.simplest()))]));
                }
                // Re-read in case the replace changed things
                if i < self.current_nodes.len() {
                    if let (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) =
                        (&self.current_nodes[i].kind, &self.current_nodes[i].value)
                    {
                        if *v < 0 && ic.validate(-*v) {
                            self.replace(&HashMap::from([(i, ChoiceValue::Integer(-*v))]));
                        }
                    }
                }
            }
            i += 1;
        }
    }

    /// Binary search integer values toward zero.
    ///
    /// Port of Hypothesis's `binary_search_integer_towards_zero`. Includes a linear
    /// scan of small values after binary search to handle non-monotonic functions
    /// (e.g. sampled_from or test functions that panic on boundary values).
    pub(super) fn binary_search_integer_towards_zero(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = &self.current_nodes[i];
            if let (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) = (&node.kind, &node.value) {
                let v = *v;
                let ic = ic.clone();
                if v > 0 {
                    let lo = ic.simplest().max(0);
                    // shift_right adaptive descent (Hypothesis's
                    // `Integer.shift_right`).  Probes `lo + (v - lo) >>
                    // k` for k = 1, 2, 4, 8, ... via `find_integer`,
                    // which is O(log log distance) rather than the
                    // O(log distance) of a full `bin_search_down`.  For
                    // distance 10^15 that's ~7 probes vs ~50.
                    let dist = (v - lo) as u128;
                    if dist > 0 {
                        find_integer(|k| {
                            let shifted = (dist >> k.min(127)) as i128;
                            // dist is u128 ≥ 0, so shifted ≥ 0 and
                            // lo + shifted ≥ lo unconditionally — no
                            // out-of-range guard needed.
                            let candidate = lo + shifted;
                            self.replace(&HashMap::from([(i, ChoiceValue::Integer(candidate))]))
                        });
                    }
                    // Linear scan small values for non-monotonic functions.
                    let range_size = ic.max_value.saturating_sub(ic.min_value).saturating_add(1);
                    let scan_count = if range_size <= 128 {
                        range_size.min(32)
                    } else {
                        8
                    };
                    let ChoiceValue::Integer(cur_v) = self.current_nodes[i].value else {
                        unreachable!(
                            "kind/value invariant violated: outer match guaranteed this variant"
                        )
                    };
                    for c in lo..lo.saturating_add(scan_count).min(cur_v) {
                        if !self.replace(&HashMap::from([(i, ChoiceValue::Integer(c))])) {
                            // Continue scanning even if not successful
                        }
                    }
                    // Hypothesis's `Integer.shrink_by_multiples(2)` / `(1)`:
                    // with a non-monotonic predicate (e.g. `|m - n| == 1`), pure
                    // bin_search_down converges to the current value without ever
                    // probing `cur - 2`. Hitting `cur - 2` is what lets the
                    // shrinker flip a linked pair from `(m, m+1)` down to
                    // `(m, m-1)` at the cost of one extra probe.
                    let ChoiceValue::Integer(base) = self.current_nodes[i].value else {
                        unreachable!(
                            "kind/value invariant violated: outer match guaranteed this variant"
                        )
                    };
                    if base > lo {
                        find_integer(|n| {
                            let attempt = base - 2 * (n as i128);
                            if attempt < lo {
                                return false;
                            }
                            self.replace(&HashMap::from([(i, ChoiceValue::Integer(attempt))]))
                        });
                    }
                    let ChoiceValue::Integer(base) = self.current_nodes[i].value else {
                        unreachable!(
                            "kind/value invariant violated: outer match guaranteed this variant"
                        )
                    };
                    if base > lo {
                        find_integer(|n| {
                            let attempt = base - (n as i128);
                            if attempt < lo {
                                return false;
                            }
                            self.replace(&HashMap::from([(i, ChoiceValue::Integer(attempt))]))
                        });
                    }
                    // Also try negative values with smaller absolute value (simpler).
                    if ic.min_value < 0 {
                        let ChoiceValue::Integer(cur_v) = self.current_nodes[i].value else {
                            unreachable!(
                                "kind/value invariant violated: outer match guaranteed this variant"
                            )
                        };
                        if cur_v > 0 {
                            let upper = (cur_v - 1).min(ic.min_value.saturating_neg());
                            if upper >= 1 {
                                // Seed at -upper, then shift-right-descend the
                                // absolute value toward 1 via find_integer.
                                self.replace(&HashMap::from([(i, ChoiceValue::Integer(-upper))]));
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
                                            ChoiceValue::Integer(-candidate_abs),
                                        )]))
                                    });
                                }
                            }
                        }
                    }
                } else if v < 0 {
                    // Mirror of the positive branch.  `lo` is the
                    // absolute value of the simplest (clamped to 0
                    // below) and we shrink toward `lo` from `-v`
                    // before flipping the sign back.
                    let lo = ic.simplest().min(0).saturating_abs();
                    let dist = ((-v) as u128).saturating_sub(lo as u128);
                    if dist > 0 {
                        find_integer(|k| {
                            let shifted = (dist >> k.min(127)) as i128;
                            // Same monotonicity argument as the positive
                            // branch: shifted ≥ 0, so lo + shifted ≥ lo.
                            let candidate_abs = lo + shifted;
                            self.replace(&HashMap::from([(
                                i,
                                ChoiceValue::Integer(-candidate_abs),
                            )]))
                        });
                    }
                    // Linear scan small negative values for non-monotonic functions.
                    let range_size = ic.max_value.saturating_sub(ic.min_value).saturating_add(1);
                    let neg_scan = if range_size <= 128 { (-v).min(32) } else { 8 };
                    for c in 1..neg_scan {
                        self.replace(&HashMap::from([(i, ChoiceValue::Integer(-c))]));
                    }
                    // shrink_by_multiples for the negative branch: probe
                    // `cur + 2*n` / `cur + n` (moving toward zero). Mirrors
                    // the positive-side block above.
                    let ChoiceValue::Integer(base) = self.current_nodes[i].value else {
                        unreachable!(
                            "kind/value invariant violated: outer match guaranteed this variant"
                        )
                    };
                    let neg_hi = -lo;
                    if base < neg_hi {
                        find_integer(|n| {
                            let attempt = base + 2 * (n as i128);
                            if attempt > neg_hi {
                                return false;
                            }
                            self.replace(&HashMap::from([(i, ChoiceValue::Integer(attempt))]))
                        });
                    }
                    let ChoiceValue::Integer(base) = self.current_nodes[i].value else {
                        unreachable!(
                            "kind/value invariant violated: outer match guaranteed this variant"
                        )
                    };
                    if base < neg_hi {
                        find_integer(|n| {
                            let attempt = base + (n as i128);
                            if attempt > neg_hi {
                                return false;
                            }
                            self.replace(&HashMap::from([(i, ChoiceValue::Integer(attempt))]))
                        });
                    }
                    // Also try positive values with smaller absolute value (simpler).
                    if ic.max_value > 0 {
                        let ChoiceValue::Integer(cur_v) = self.current_nodes[i].value else {
                            unreachable!(
                                "kind/value invariant violated: outer match guaranteed this variant"
                            )
                        };
                        if cur_v < 0 {
                            let upper = (-cur_v - 1).min(ic.max_value);
                            if upper >= 1 {
                                // Seed at +upper, then shift-right-descend
                                // toward lo_pos via find_integer.
                                self.replace(&HashMap::from([(i, ChoiceValue::Integer(upper))]));
                                let lo_pos = ic.simplest().max(0);
                                let dist = (upper - lo_pos) as u128;
                                if dist > 0 {
                                    find_integer(|k| {
                                        let shifted = (dist >> k.min(127)) as i128;
                                        let candidate = lo_pos + shifted;
                                        self.replace(&HashMap::from([(
                                            i,
                                            ChoiceValue::Integer(candidate),
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
                                    self.replace(&HashMap::from([(i, ChoiceValue::Integer(c))]));
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
    /// Port of Hypothesis's `redistribute_integers`. For each pair of integer
    /// nodes at various distances, tries moving value from i to j (or vice
    /// versa) while keeping the total sum constant. Useful for sum-type
    /// constraints where the minimal counterexample has one small and one
    /// large value.
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

                let ChoiceValue::Integer(prev_i) = self.current_nodes[i].value else {
                    unreachable!(
                        "kind/value invariant violated: outer match guaranteed this variant"
                    )
                };
                let ChoiceValue::Integer(prev_j) = self.current_nodes[j].value else {
                    unreachable!(
                        "kind/value invariant violated: outer match guaranteed this variant"
                    )
                };

                let ChoiceKind::Integer(ic_i) = &self.current_nodes[i].kind else {
                    unreachable!(
                        "kind/value invariant violated: outer match guaranteed this variant"
                    )
                };
                let simplest_i = ic_i.simplest();

                if prev_i != simplest_i {
                    if prev_i > 0 {
                        bin_search_down(0, prev_i, &mut |v| {
                            let delta = prev_i - v;
                            self.replace(&HashMap::from([
                                (i, ChoiceValue::Integer(v)),
                                (j, ChoiceValue::Integer(prev_j + delta)),
                            ]))
                        });
                    } else if prev_i < 0 {
                        bin_search_down(0, -prev_i, &mut |a| {
                            let delta = prev_i + a; // = -(|prev_i| - a)
                            self.replace(&HashMap::from([
                                (i, ChoiceValue::Integer(-a)),
                                (j, ChoiceValue::Integer(prev_j + delta)),
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
    /// Port of Hypothesis's `lower_integers_together`. The individual passes
    /// (`binary_search_integer_towards_zero`, `redistribute_integers`) walk
    /// each integer alone; when two values are pinned together by a
    /// predicate like `|m - n| == 1`, neither can move on its own without
    /// breaking the predicate, and the shrinker falls into a zig-zag trap
    /// that takes `O(m)` iterations to crawl down. By probing
    /// `(v_i - k, v_j - k)` for geometrically growing `k` via
    /// `find_integer`, this pass reaches the minimum in `O(log k)` probes.
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
            // Hypothesis caps the look-ahead at 3 integers to avoid
            // quadratic behaviour on long sequences.
            for gap in 1..=3 {
                if pair_idx + gap >= int_indices.len() {
                    break;
                }
                let i = int_indices[pair_idx];
                let j = int_indices[pair_idx + gap];
                if i >= self.current_nodes.len() || j >= self.current_nodes.len() {
                    break;
                }

                let (ChoiceKind::Integer(ic_i), ChoiceValue::Integer(v_i)) =
                    (&self.current_nodes[i].kind, &self.current_nodes[i].value)
                else {
                    continue;
                };
                let ChoiceKind::Integer(ic_j) = &self.current_nodes[j].kind else {
                    continue;
                };
                let ChoiceValue::Integer(v_j) = self.current_nodes[j].value else {
                    unreachable!("kind/value mismatch: Integer kind with non-Integer value");
                };
                let v_i = *v_i;
                let ic_i = ic_i.clone();
                let ic_j = ic_j.clone();

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
                let st_i = ic_i.clamped_shrink_towards();
                let st_j = ic_j.clamped_shrink_towards();

                // Direction is determined by the i-th element (shortlex
                // dominates on element 0): move it toward its own st.
                // The j-th element follows. If j is on the same side of
                // its st, joint motion is unambiguously better; if j is
                // on the opposite side, j's sort_key grows but i's gain
                // wins the shortlex comparison.
                let _ = st_j;

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
                        if !ic_i.validate(new_i) || !ic_j.validate(new_j) {
                            return false;
                        }
                        self.replace(&HashMap::from([
                            (i, ChoiceValue::Integer(new_i)),
                            (j, ChoiceValue::Integer(new_j)),
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
                        if !ic_i.validate(new_i) || !ic_j.validate(new_j) {
                            return false;
                        }
                        self.replace(&HashMap::from([
                            (i, ChoiceValue::Integer(new_i)),
                            (j, ChoiceValue::Integer(new_j)),
                        ]))
                    });
                }
            }
        }
    }

    /// Try shrinking duplicate integer values simultaneously.
    ///
    /// Port of Hypothesis's `minimize_duplicated_choices` (`shrinker.py:1379-1406`).
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
        let mut groups: HashMap<(std::mem::Discriminant<ChoiceKind>, ChoiceValue), Vec<usize>> =
            HashMap::new();
        for (i, node) in self.current_nodes.iter().enumerate() {
            let key = (std::mem::discriminant(&node.kind), node.value.clone());
            groups.entry(key).or_default().push(i);
        }
        for (_key, indices) in groups.iter() {
            if indices.len() < 2 {
                continue;
            }
            // Try the simplest-replacement step for every group.  For
            // boolean / float / bytes / string this is the main win; the
            // integer branch below adds a deeper binary search.
            let first = &self.current_nodes[indices[0]];
            let simplest = first.kind.simplest();
            if simplest != first.value {
                let replacements: HashMap<usize, ChoiceValue> = indices
                    .iter()
                    .filter(|&&i| {
                        i < self.current_nodes.len()
                            && self.current_nodes[i].value == first.value
                            && std::mem::discriminant(&self.current_nodes[i].kind)
                                == std::mem::discriminant(&first.kind)
                    })
                    .map(|&i| (i, simplest.clone()))
                    .collect();
                if replacements.len() >= 2 {
                    self.replace(&replacements);
                }
            }
        }
        // The remainder of this function is the legacy integer-only
        // binary-search loop, kept verbatim so the existing tests still
        // pass; conceptually this work could move into the unified
        // `minimize_individual_choices` driver, but
        // `shrink_duplicates`' "step duplicates together" semantics
        // aren't covered there.
        let mut groups: HashMap<i128, Vec<usize>> = HashMap::new();
        for (i, node) in self.current_nodes.iter().enumerate() {
            if let (ChoiceKind::Integer(_), ChoiceValue::Integer(v)) = (&node.kind, &node.value) {
                groups.entry(*v).or_default().push(i);
            }
        }

        for (value, indices) in groups {
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
                    .map(|&i| (i, ChoiceValue::Integer(simplest)))
                    .collect();
                self.replace(&replacements);
            }

            // Re-read current value after possible replacement.
            let ChoiceValue::Integer(cur_value) = self.current_nodes[valid[0]].value else {
                unreachable!("kind/value invariant violated: outer match guaranteed this variant")
            };

            // Shift-right adaptive descent of all members in lockstep,
            // followed by shrink_by_multiples(2) and (1) to land on the
            // boundary.  Mirrors Hypothesis's `Integer.shift_right`
            // applied to a duplicate group, and the per-node block
            // earlier in this file.  Each probe re-reads the current
            // value of `valid[0]` so the descent starts from the live
            // shrink target — the previous bin_search_down captured the
            // entry value and stalled on the second probe because every
            // member had moved.
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
                    .map(|&i| (i, ChoiceValue::Integer(candidate)))
                    .collect();
                sh.replace(&replacements)
            };
            if cur_value > 0 {
                let lo = ic.simplest().max(0);
                let dist = (cur_value - lo) as u128;
                if dist > 0 {
                    find_integer(|k| {
                        let shifted = (dist >> k.min(127)) as i128;
                        let candidate = lo + shifted;
                        group_replace(self, candidate)
                    });
                }
                let live_base = |sh: &Shrinker<'_>| -> i128 {
                    match sh.current_nodes[valid_capture[0]].value {
                        ChoiceValue::Integer(v) => v,
                        _ => i128::MAX,
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
                let lo = ic.simplest().min(0).saturating_abs();
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
                    match sh.current_nodes[valid_capture[0]].value {
                        ChoiceValue::Integer(v) => v,
                        _ => i128::MIN,
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
    /// Port of `shrinker.py:1017-1095` (`lower_common_node_offset`).  When
    /// two integers `m, n` are linked by a predicate like `abs(m - n) > 1`,
    /// the individual minimization passes can only step each toward
    /// `shrink_towards` by one before the predicate flips; the next
    /// iteration steps the other one by one; result: O(initial value)
    /// iterations to zig-zag toward zero.
    ///
    /// This pass observes that *all* changed integer nodes shrank by some
    /// non-zero common offset, and tries to lower that offset directly
    /// using a `find_integer` exponential probe — collapsing the zig-zag
    /// into O(log v) iterations.  It probes both signs because the
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
            let (ic, v) = match (&node.kind, &node.value) {
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => (ic.clone(), *v),
                _ => continue,
            };
            let target = ic.clamped_shrink_towards();
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
        // for each node.  Hypothesis shrinks the offset both directions
        // to handle the case where the absolute distances are equal but
        // the signs differ.
        let signs: Vec<i128> = indices
            .iter()
            .zip(ic_targets.iter())
            .map(|(&i, &target)| {
                let v = match self.current_nodes[i].value {
                    ChoiceValue::Integer(v) => v,
                    _ => unreachable!(),
                };
                if v >= target { 1 } else { -1 }
            })
            .collect();

        // Try lowering by an additional `n` units in both directions.
        // The candidate distance is `offset - n + residual`, applied with
        // the original signs.  Hypothesis uses `Integer.shrink(offset, ...)`
        // which is binary search; `find_integer` for the maximum n is the
        // equivalent in our infrastructure.
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
                    replacements.insert(indices[k], ChoiceValue::Integer(new_value));
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

// Integer-based shrink passes: zero_choices, swap_integer_sign,
// binary_search_integer_towards_zero, redistribute_integers, shrink_duplicates.

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
                    bin_search_down(lo, v, &mut |candidate| {
                        self.replace(&HashMap::from([(i, ChoiceValue::Integer(candidate))]))
                    });
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
                                self.replace(&HashMap::from([(i, ChoiceValue::Integer(-upper))]));
                                bin_search_down(1, upper, &mut |a| {
                                    self.replace(&HashMap::from([(i, ChoiceValue::Integer(-a))]))
                                });
                            }
                        }
                    }
                } else if v < 0 {
                    let lo = ic.simplest().min(0).saturating_abs();
                    bin_search_down(lo, -v, &mut |candidate| {
                        self.replace(&HashMap::from([(i, ChoiceValue::Integer(-candidate))]))
                    });
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
                                self.replace(&HashMap::from([(i, ChoiceValue::Integer(upper))]));
                                let lo_pos = ic.simplest().max(0);
                                bin_search_down(lo_pos, upper, &mut |c| {
                                    self.replace(&HashMap::from([(i, ChoiceValue::Integer(c))]))
                                });
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
                // nocov start
                if pair_idx + gap >= current_ints.len() {
                    if pair_idx == 0 {
                        break;
                    }
                    pair_idx -= 1;
                    continue;
                }
                // nocov end

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
                    break; // nocov — indices guarded by int_indices construction
                }

                let (ChoiceKind::Integer(ic_i), ChoiceValue::Integer(v_i)) =
                    (&self.current_nodes[i].kind, &self.current_nodes[i].value)
                else {
                    continue; // nocov — int_indices only collects Integer-kind nodes
                };
                let ChoiceKind::Integer(ic_j) = &self.current_nodes[j].kind else {
                    continue; // nocov — int_indices only collects Integer-kind nodes
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
                            return false; // nocov — k upper bound reached, find_integer terminates
                        }
                        let new_i = v_i + k;
                        let new_j = v_j + k;
                        if !ic_i.validate(new_i) || !ic_j.validate(new_j) {
                            return false; // nocov — out-of-range proposal, find_integer skips
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
    /// Port of Hypothesis's `shrink_duplicates`. For each group of integer nodes
    /// with the same value, applies binary search to all simultaneously. This
    /// handles cases where two integers must remain equal (e.g. a vec element
    /// and a separate integer that must be in the vec).
    pub(super) fn shrink_duplicates(&mut self) {
        // Find groups of integer node indices that share the same value.
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
                continue; // nocov — re-validation failure for groups that lost members
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

            // Binary search all simultaneously toward zero.
            if cur_value > 0 {
                let lo = ic.simplest().max(0);
                let v_cur = cur_value;
                bin_search_down(lo, v_cur, &mut |candidate| {
                    // Re-validate indices.
                    let current_valid: Vec<usize> = valid
                        .iter()
                        .copied()
                        .filter(|&i| {
                            i < self.current_nodes.len()
                                && matches!(&self.current_nodes[i].value, ChoiceValue::Integer(v) if *v == cur_value)
                        })
                        .collect();
                    if current_valid.len() < 2 {
                        return false; // nocov — concurrent re-validation guard
                    }
                    let replacements: HashMap<usize, ChoiceValue> = current_valid
                        .iter()
                        .map(|&i| (i, ChoiceValue::Integer(candidate)))
                        .collect();
                    self.replace(&replacements)
                });
            } else if cur_value < 0 {
                let lo = ic.simplest().min(0).saturating_abs();
                let v_abs = -cur_value;
                bin_search_down(lo, v_abs, &mut |candidate| {
                    let current_valid: Vec<usize> = valid
                        .iter()
                        .copied()
                        .filter(|&i| {
                            i < self.current_nodes.len()
                                && matches!(&self.current_nodes[i].value, ChoiceValue::Integer(v) if *v == cur_value)
                        })
                        .collect();
                    if current_valid.len() < 2 {
                        return false; // nocov — concurrent re-validation guard
                    }
                    let replacements: HashMap<usize, ChoiceValue> = current_valid
                        .iter()
                        .map(|&i| (i, ChoiceValue::Integer(-candidate)))
                        .collect();
                    self.replace(&replacements)
                });
            }
        }
    }
}

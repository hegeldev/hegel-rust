// Integer-based shrink passes: zero_choices, swap_integer_sign,
// binary_search_integer_towards_zero, redistribute_integers, shrink_duplicates.

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceValue};

use super::{Shrinker, bin_search_down};

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
    /// Port of pbtkit's `binary_search_integer_towards_zero`. Includes a linear
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
                        unreachable!()
                    };
                    for c in lo..lo.saturating_add(scan_count).min(cur_v) {
                        if !self.replace(&HashMap::from([(i, ChoiceValue::Integer(c))])) {
                            // Continue scanning even if not successful
                        }
                    }
                    // Also try negative values with smaller absolute value (simpler).
                    if ic.min_value < 0 {
                        let ChoiceValue::Integer(cur_v) = self.current_nodes[i].value else {
                            unreachable!()
                        };
                        if cur_v > 0 {
                            let upper = (cur_v - 1).min(-ic.min_value);
                            if upper >= 1 {
                                self.replace(&HashMap::from([(i, ChoiceValue::Integer(-upper))]));
                                bin_search_down(1, upper, &mut |a| {
                                    self.replace(&HashMap::from([(i, ChoiceValue::Integer(-a))]))
                                });
                            }
                        }
                    }
                } else if v < 0 {
                    let lo = ic.simplest().min(0).abs();
                    bin_search_down(lo, -v, &mut |candidate| {
                        self.replace(&HashMap::from([(i, ChoiceValue::Integer(-candidate))]))
                    });
                    // Linear scan small negative values for non-monotonic functions.
                    let range_size = ic.max_value.saturating_sub(ic.min_value).saturating_add(1);
                    let neg_scan = if range_size <= 128 { (-v).min(32) } else { 8 };
                    for c in 1..neg_scan {
                        self.replace(&HashMap::from([(i, ChoiceValue::Integer(-c))]));
                    }
                    // Also try positive values with smaller absolute value (simpler).
                    if ic.max_value > 0 {
                        let ChoiceValue::Integer(cur_v) = self.current_nodes[i].value else {
                            unreachable!()
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
    /// Port of pbtkit's `redistribute_integers`. For each pair of integer
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
                    unreachable!()
                };
                let ChoiceValue::Integer(prev_j) = self.current_nodes[j].value else {
                    unreachable!()
                };

                let ChoiceKind::Integer(ic_i) = &self.current_nodes[i].kind else {
                    unreachable!()
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

    /// Try shrinking duplicate integer values simultaneously.
    ///
    /// Port of pbtkit's `shrink_duplicates`. For each group of integer nodes
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
                continue;
            }

            let ChoiceKind::Integer(ic) = &self.current_nodes[valid[0]].kind else {
                unreachable!()
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
                unreachable!()
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
                        return false;
                    }
                    let replacements: HashMap<usize, ChoiceValue> = current_valid
                        .iter()
                        .map(|&i| (i, ChoiceValue::Integer(candidate)))
                        .collect();
                    self.replace(&replacements)
                });
            } else if cur_value < 0 {
                let lo = ic.simplest().min(0).abs();
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
                        return false;
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

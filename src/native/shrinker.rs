// Shrinker for the native backend.
//
// Ported from pbtkit core.py. Reduces failing test cases to minimal
// counterexamples by systematically simplifying the choice sequence.

use std::collections::HashMap;

use crate::native::core::{
    ChoiceKind, ChoiceNode, ChoiceValue, NodeSortKey,
    MAX_SHRINK_ITERATIONS, float_to_index, index_to_float, sort_key,
};

/// A callback that runs a test case from a choice sequence.
/// Returns `(is_interesting, actual_nodes_consumed)`.
/// `actual_nodes_consumed` is how many ChoiceNodes were produced
/// during the run (may be less than candidate length for flatmap bindings).
pub type TestFn<'a> = dyn FnMut(&[ChoiceNode]) -> (bool, usize) + 'a;

pub struct Shrinker<'a> {
    test_fn: Box<TestFn<'a>>,
    pub current_nodes: Vec<ChoiceNode>,
}

impl<'a> Shrinker<'a> {
    pub fn new(
        test_fn: Box<TestFn<'a>>,
        initial_nodes: Vec<ChoiceNode>,
    ) -> Self {
        Shrinker {
            test_fn,
            current_nodes: initial_nodes,
        }
    }

    /// Try a candidate choice sequence. If interesting and smaller than
    /// the current best, update current_nodes. Returns whether interesting.
    pub fn consider(&mut self, nodes: &[ChoiceNode]) -> bool {
        if sort_key(nodes) == sort_key(&self.current_nodes) {
            return true;
        }
        let (is_interesting, _) = (self.test_fn)(nodes);
        if is_interesting && sort_key(nodes) < sort_key(&self.current_nodes) {
            self.current_nodes = nodes.to_vec();
        }
        is_interesting
    }

    /// Try replacing values at specific indices.
    pub fn replace(&mut self, values: &HashMap<usize, ChoiceValue>) -> bool {
        let mut attempt: Vec<ChoiceNode> = self.current_nodes.clone();
        for (&i, v) in values {
            assert!(i < attempt.len());
            attempt[i] = attempt[i].with_value(v.clone());
        }
        self.consider(&attempt)
    }

    /// Run all shrink passes repeatedly until no more progress or iteration cap.
    pub fn shrink(&mut self) {
        let mut prev: Vec<NodeSortKey> = Vec::new();
        let mut iterations = 0;

        loop {
            let current_key: Vec<NodeSortKey> =
                self.current_nodes.iter().map(|n| n.sort_key()).collect();
            if current_key == prev || iterations >= MAX_SHRINK_ITERATIONS {
                break;
            }
            prev = current_key;
            iterations += 1;

            self.delete_chunks();
            self.zero_choices();
            self.swap_integer_sign();
            self.binary_search_integer_towards_zero();
            self.bind_deletion();
            self.redistribute_integers();
            self.shrink_duplicates();
            self.sort_values();
            self.swap_adjacent_blocks();
            self.shrink_floats();
        }
    }

    /// Try deleting chunks of choices from the sequence.
    ///
    /// Longer chunks allow deleting composite elements (e.g. a list element
    /// requires deleting both the "include?" choice and the element itself).
    /// Iterates backwards since later choices tend to depend on earlier ones.
    fn delete_chunks(&mut self) {
        let mut k: usize = 8;
        while k > 0 {
            let mut i = self.current_nodes.len().saturating_sub(k + 1);
            loop {
                if i >= self.current_nodes.len() {
                    if i == 0 {
                        break;
                    }
                    i -= 1;
                    continue;
                }
                let end = (i + k).min(self.current_nodes.len());
                let mut attempt: Vec<ChoiceNode> = self.current_nodes[..i].to_vec();
                attempt.extend_from_slice(&self.current_nodes[end..]);
                assert!(attempt.len() < self.current_nodes.len());

                if !self.consider(&attempt) {
                    // Try decrementing the preceding choice (helps with
                    // collection length counters).
                    if i > 0 {
                        let prev = &attempt[i - 1];
                        if let (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) =
                            (&prev.kind, &prev.value)
                        {
                            if *v != ic.simplest() {
                                let mut modified = attempt.clone();
                                modified[i - 1] =
                                    modified[i - 1].with_value(ChoiceValue::Integer(v - 1));
                                if self.consider(&modified) {
                                    if i == 0 {
                                        break;
                                    }
                                    i -= 1;
                                    continue;
                                }
                            }
                        }
                        if let (ChoiceKind::Boolean(_), ChoiceValue::Boolean(true)) =
                            (&prev.kind, &prev.value)
                        {
                            let mut modified = attempt.clone();
                            modified[i - 1] =
                                modified[i - 1].with_value(ChoiceValue::Boolean(false));
                            if self.consider(&modified) {
                                if i == 0 {
                                    break;
                                }
                                i -= 1;
                                continue;
                            }
                        }
                    }
                    if i == 0 {
                        break;
                    }
                    i -= 1;
                } else if i == 0 {
                    break;
                } else {
                    i -= 1;
                }
            }
            k -= 1;
        }
    }

    /// Replace blocks of choices with their simplest values.
    fn zero_choices(&mut self) {
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
    fn swap_integer_sign(&mut self) {
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

    /// Shrink float choices toward simpler values using Hypothesis lex ordering.
    ///
    /// Steps per float node:
    /// 1. Try replacing with simplest().
    /// 2. If sign-negative, try negating (positive is simpler).
    /// 3. Binary search on absolute-value lex index from 0 toward current value.
    ///    Searching from 0 ensures we can find "nice" integer floats (like 2.0)
    ///    even when they have smaller lex indices than the boundary values.
    fn shrink_floats(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = &self.current_nodes[i];
            if let (ChoiceKind::Float(fc), ChoiceValue::Float(v)) = (&node.kind, &node.value) {
                let v = *v;
                let fc = fc.clone();

                // Step 1: Try simplest.
                let s = fc.simplest();
                if ChoiceValue::Float(s) != ChoiceValue::Float(v) {
                    self.replace(&std::collections::HashMap::from([(i, ChoiceValue::Float(s))]));
                }

                // Re-read current value.
                let v = {
                    let Some(n) = self.current_nodes.get(i) else { break; };
                    let ChoiceValue::Float(f) = n.value else { i += 1; continue; };
                    f
                };

                // Skip NaN — can't binary search on NaN.
                if v.is_nan() {
                    i += 1;
                    continue;
                }

                // Step 2: Try negating if sign-negative (positive is simpler).
                if v.is_sign_negative() {
                    let neg = -v;
                    if fc.validate(neg) {
                        self.replace(&std::collections::HashMap::from([(i, ChoiceValue::Float(neg))]));
                    }
                }

                // Re-read after possible negation.
                let v = {
                    let Some(n) = self.current_nodes.get(i) else { break; };
                    let ChoiceValue::Float(f) = n.value else { i += 1; continue; };
                    f
                };

                if v.is_nan() {
                    i += 1;
                    continue;
                }

                // Step 3: Binary search on absolute-value lex index toward 0.
                // float_to_index handles both finite and infinite non-NaN non-negative floats.
                let v_abs = v.abs();
                let current_idx = float_to_index(v_abs);
                let is_neg = v.is_sign_negative();
                if current_idx > 0 {
                    bin_search_down(0, current_idx as i128, &mut |idx| {
                        let candidate_mag = index_to_float(idx as u64);
                        let candidate = if is_neg { -candidate_mag } else { candidate_mag };
                        if fc.validate(candidate) {
                            self.replace(&std::collections::HashMap::from([(
                                i,
                                ChoiceValue::Float(candidate),
                            )]))
                        } else {
                            false
                        }
                    });
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
    fn binary_search_integer_towards_zero(&mut self) {
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
                    let scan_count = if range_size <= 128 { range_size.min(32) } else { 8 };
                    let cur_v = if let ChoiceValue::Integer(cv) = self.current_nodes[i].value { cv } else { v };
                    for c in lo..lo.saturating_add(scan_count).min(cur_v) {
                        if !self.replace(&HashMap::from([(i, ChoiceValue::Integer(c))])) {
                            // Continue scanning even if not successful
                        }
                    }
                    // Also try negative values with smaller absolute value (simpler).
                    if ic.min_value < 0 {
                        let cur_v = if let ChoiceValue::Integer(cv) = self.current_nodes[i].value { cv } else { v };
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
                        let cur_v = if let ChoiceValue::Integer(cv) = self.current_nodes[i].value { cv } else { v };
                        if cur_v < 0 {
                            let upper = (-cur_v - 1).min(ic.max_value);
                            if upper >= 1 {
                                self.replace(&HashMap::from([(i, ChoiceValue::Integer(upper))]));
                                let lo_pos = ic.simplest().max(0);
                                bin_search_down(lo_pos, upper, &mut |c| {
                                    self.replace(&HashMap::from([(i, ChoiceValue::Integer(c))]))
                                });
                                // Linear scan positive values.
                                let scan_count = if range_size <= 128 { range_size.min(32) } else { 8 };
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
    fn redistribute_integers(&mut self) {
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

                let (prev_i, prev_j) = {
                    let ni = &self.current_nodes[i];
                    let nj = &self.current_nodes[j];
                    match (&ni.value, &nj.value) {
                        (ChoiceValue::Integer(a), ChoiceValue::Integer(b)) => (*a, *b),
                        _ => {
                            if pair_idx == 0 {
                                break;
                            }
                            pair_idx -= 1;
                            continue;
                        }
                    }
                };

                let simplest_i = if let ChoiceKind::Integer(ic) = &self.current_nodes[i].kind {
                    ic.simplest()
                } else {
                    0
                };

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
    fn shrink_duplicates(&mut self) {
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

            let ic = if let ChoiceKind::Integer(ic) = &self.current_nodes[valid[0]].kind {
                ic.clone()
            } else {
                continue;
            };

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
            let cur_value = if let ChoiceValue::Integer(v) = self.current_nodes[valid[0]].value {
                v
            } else {
                continue;
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

    /// Try sorting groups of same-type choices by sort key.
    ///
    /// Port of pbtkit's `sort_values`. Groups choices by type and tries
    /// sorting each group so simpler values come first, enabling other
    /// passes to further reduce the leading choices.
    fn sort_values(&mut self) {
        // Sort integer choices by absolute value.
        self.sort_values_integers();
        // Sort boolean choices: false (0) before true (1).
        self.sort_values_booleans();
    }

    fn sort_values_integers(&mut self) {
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

        if int_indices.len() < 2 {
            return;
        }

        let values: Vec<ChoiceValue> = int_indices
            .iter()
            .map(|&i| self.current_nodes[i].value.clone())
            .collect();
        let mut sorted = values.clone();
        sorted.sort_by(|a, b| {
            let key_a = match a {
                ChoiceValue::Integer(v) => v.unsigned_abs(),
                _ => u128::MAX,
            };
            let key_b = match b {
                ChoiceValue::Integer(v) => v.unsigned_abs(),
                _ => u128::MAX,
            };
            key_a.cmp(&key_b)
        });

        if sorted != values {
            let replacements: HashMap<usize, ChoiceValue> = int_indices
                .iter()
                .zip(sorted.iter())
                .map(|(&i, v)| (i, v.clone()))
                .collect();
            self.replace(&replacements);
        }
    }

    /// Port of pbtkit's `swap_adjacent_blocks`.
    ///
    /// For each block size 2..=8, tries swapping adjacent blocks of the same
    /// type structure (same sequence of choice kinds). This handles cases like
    /// list entries where each entry spans multiple choices (e.g. [continue,
    /// value]) and the sorting pass can't swap individual values without
    /// breaking structure.
    fn swap_adjacent_blocks(&mut self) {
        for block_size in 2usize..=8 {
            let mut i = 0;
            while i + 2 * block_size <= self.current_nodes.len() {
                let j = i + block_size;

                // Check that both blocks have matching type structure.
                let types_a: Vec<std::mem::Discriminant<ChoiceKind>> = (0..block_size)
                    .map(|k| std::mem::discriminant(&self.current_nodes[i + k].kind))
                    .collect();
                let types_b: Vec<std::mem::Discriminant<ChoiceKind>> = (0..block_size)
                    .map(|k| std::mem::discriminant(&self.current_nodes[j + k].kind))
                    .collect();

                if types_a != types_b {
                    i += 1;
                    continue;
                }

                let block_a: Vec<ChoiceValue> = (0..block_size)
                    .map(|k| self.current_nodes[i + k].value.clone())
                    .collect();
                let block_b: Vec<ChoiceValue> = (0..block_size)
                    .map(|k| self.current_nodes[j + k].value.clone())
                    .collect();

                if block_a == block_b {
                    i += 1;
                    continue;
                }

                // Try swapping block_a and block_b.
                let mut swap = HashMap::new();
                for k in 0..block_size {
                    swap.insert(i + k, block_b[k].clone());
                    swap.insert(j + k, block_a[k].clone());
                }
                self.replace(&swap);
                i += 1;
            }
        }
    }

    /// Port of pbtkit's `bind_deletion`.
    ///
    /// When a value controls the length of a downstream sequence (e.g.
    /// via flat_map), reducing that value may shorten the test case without
    /// keeping the result interesting. This pass detects that situation and
    /// tries deleting the now-excess choices to recover an interesting result.
    fn bind_deletion(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();

            // Only process integer nodes — these control sequence lengths.
            let (current_val, ic) = match (&node.kind, &node.value) {
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => (*v, ic.clone()),
                _ => {
                    i += 1;
                    continue;
                }
            };

            let simplest = ic.simplest();
            if current_val == simplest {
                i += 1;
                continue;
            }

            let expected_len = self.current_nodes.len();

            // Binary-search smaller integer values; for each candidate, try
            // replace-with-deletion.
            let changed = bin_search_down(simplest, current_val, &mut |v| {
                self.try_replace_with_deletion(i, ChoiceValue::Integer(v), expected_len)
            });
            let _ = changed;

            i += 1;
        }
    }

    /// Try replacing the value at `idx`. If the result is interesting, done.
    /// If the result is valid but used fewer nodes than `expected_len`, try
    /// deleting regions after `idx` to recover an interesting result.
    fn try_replace_with_deletion(
        &mut self,
        idx: usize,
        value: ChoiceValue,
        expected_len: usize,
    ) -> bool {
        // First try a straight replace.
        if self.replace(&HashMap::from([(idx, value.clone())])) {
            return true;
        }

        // Build the attempt with new value and probe the test.
        if idx >= self.current_nodes.len() {
            return false;
        }
        let mut attempt = self.current_nodes.clone();
        attempt[idx] = attempt[idx].with_value(value);

        let (is_interesting, actual_len) = (self.test_fn)(&attempt);
        if is_interesting {
            if sort_key(&attempt) < sort_key(&self.current_nodes) {
                self.current_nodes = attempt.clone();
            }
            return true;
        }

        if actual_len >= expected_len {
            return false;
        }

        // The test used fewer nodes. Try deleting regions after idx.
        let k = expected_len - actual_len;
        for size in (1..=k).rev() {
            // Start near the end and work backward.
            let start = attempt.len().saturating_sub(size);
            let mut j = if start > idx { start } else { continue };
            loop {
                let end = j + size;
                if end > attempt.len() {
                    if j == 0 || j <= idx {
                        break;
                    }
                    j -= 1;
                    continue;
                }
                let mut candidate = attempt[..j].to_vec();
                candidate.extend_from_slice(&attempt[end..]);
                if self.consider(&candidate) {
                    return true;
                }
                if j <= idx {
                    break;
                }
                j -= 1;
            }
        }
        false
    }

    fn sort_values_booleans(&mut self) {
        let bool_indices: Vec<usize> = self
            .current_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                if matches!(n.kind, ChoiceKind::Boolean(_)) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        if bool_indices.len() < 2 {
            return;
        }

        let values: Vec<ChoiceValue> = bool_indices
            .iter()
            .map(|&i| self.current_nodes[i].value.clone())
            .collect();
        let mut sorted = values.clone();
        // Sort: false (0) before true (1).
        sorted.sort_by(|a, b| {
            let key_a = match a {
                ChoiceValue::Boolean(v) => u8::from(*v),
                _ => u8::MAX,
            };
            let key_b = match b {
                ChoiceValue::Boolean(v) => u8::from(*v),
                _ => u8::MAX,
            };
            key_a.cmp(&key_b)
        });

        if sorted != values {
            let replacements: HashMap<usize, ChoiceValue> = bool_indices
                .iter()
                .zip(sorted.iter())
                .map(|(&i, v)| (i, v.clone()))
                .collect();
            self.replace(&replacements);
        }
    }
}

/// Binary search for the smallest value in [lo, hi] where f returns true.
///
/// Assumes f(hi) is true (not checked). Returns lo if f(lo) is true,
/// otherwise finds a locally minimal true value.
fn bin_search_down(lo: i128, hi: i128, f: &mut impl FnMut(i128) -> bool) -> i128 {
    if f(lo) {
        return lo;
    }
    let mut lo = lo;
    let mut hi = hi;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if f(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

// Sequence-ordering shrink passes: sort_values, swap_adjacent_blocks.

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceValue};

use super::Shrinker;

impl<'a> Shrinker<'a> {
    /// Try sorting groups of same-type choices by sort key.
    ///
    /// Port of pbtkit's `sort_values`. Groups choices by type and tries
    /// sorting each group so simpler values come first, enabling other
    /// passes to further reduce the leading choices. First attempts a
    /// full sort; if that fails the `consider` predicate, falls back to
    /// an insertion-sort loop where each adjacent swap is validated
    /// individually. The fallback matters when earlier swaps cause
    /// structural changes (e.g. value punning on collection-bearing
    /// kinds) that would make the full sort's replace unreachable.
    pub(super) fn sort_values(&mut self) {
        // Sort integer choices by absolute value.
        self.sort_values_integers();
        // Sort boolean choices: false (0) before true (1).
        self.sort_values_booleans();
    }

    pub(super) fn sort_values_integers(&mut self) {
        self.try_sort_group(|k| matches!(k, ChoiceKind::Integer(_)));
    }

    pub(super) fn sort_values_booleans(&mut self) {
        self.try_sort_group(|k| matches!(k, ChoiceKind::Boolean(_)));
    }

    fn try_sort_group<F>(&mut self, matches_kind: F)
    where
        F: Fn(&ChoiceKind) -> bool,
    {
        let indices: Vec<usize> = self
            .current_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| if matches_kind(&n.kind) { Some(i) } else { None })
            .collect();

        if indices.len() < 2 {
            return;
        }

        let values: Vec<ChoiceValue> = indices
            .iter()
            .map(|&i| self.current_nodes[i].value.clone())
            .collect();
        let mut keyed: Vec<_> = indices
            .iter()
            .map(|&i| {
                (
                    self.current_nodes[i].sort_key(),
                    self.current_nodes[i].value.clone(),
                )
            })
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        let sorted_values: Vec<ChoiceValue> = keyed.into_iter().map(|(_, v)| v).collect();

        if sorted_values != values {
            let replacements: HashMap<usize, ChoiceValue> = indices
                .iter()
                .zip(sorted_values.iter())
                .map(|(&i, v)| (i, v.clone()))
                .collect();
            if self.replace(&replacements) {
                return;
            }
        }

        // Insertion-sort fallback (pbtkit's `feature_enabled("collections")`
        // branch of `_try_sort_group`). Each iteration refreshes the valid
        // indices because a prior successful swap can shorten current_nodes
        // or change kinds at fixed positions via value punning.
        for pos in 1..indices.len() {
            let mut j = pos;
            while j > 0 {
                let valid: Vec<usize> = indices
                    .iter()
                    .copied()
                    .filter(|&i| {
                        i < self.current_nodes.len() && matches_kind(&self.current_nodes[i].kind)
                    })
                    .collect();
                if j >= valid.len() {
                    break;
                }
                let idx_j = valid[j];
                let idx_prev = valid[j - 1];
                if self.current_nodes[idx_prev].sort_key() <= self.current_nodes[idx_j].sort_key() {
                    break;
                }
                let v_j = self.current_nodes[idx_j].value.clone();
                let v_prev = self.current_nodes[idx_prev].value.clone();
                let mut swap = HashMap::new();
                swap.insert(idx_prev, v_j);
                swap.insert(idx_j, v_prev);
                if self.replace(&swap) {
                    j -= 1;
                    continue;
                }
                break;
            }
        }
    }

    /// Port of pbtkit's `swap_adjacent_blocks`.
    ///
    /// For each block size 2..=8, tries swapping adjacent blocks of the same
    /// type structure (same sequence of choice kinds). This handles cases like
    /// list entries where each entry spans multiple choices (e.g. [continue,
    /// value]) and the sorting pass can't swap individual values without
    /// breaking structure.
    pub(super) fn swap_adjacent_blocks(&mut self) {
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
}

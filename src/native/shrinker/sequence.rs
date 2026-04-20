// Sequence-ordering shrink passes: sort_values, swap_adjacent_blocks.

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceValue};

use super::Shrinker;

impl<'a> Shrinker<'a> {
    /// Try sorting groups of same-type choices by sort key.
    ///
    /// Port of pbtkit's `sort_values`. Groups choices by type and tries
    /// sorting each group so simpler values come first, enabling other
    /// passes to further reduce the leading choices.
    pub(super) fn sort_values(&mut self) {
        // Sort integer choices by absolute value.
        self.sort_values_integers();
        // Sort boolean choices: false (0) before true (1).
        self.sort_values_booleans();
    }

    pub(super) fn sort_values_integers(&mut self) {
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
            let ChoiceValue::Integer(va) = a else {
                unreachable!()
            };
            let ChoiceValue::Integer(vb) = b else {
                unreachable!()
            };
            va.unsigned_abs().cmp(&vb.unsigned_abs())
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

    pub(super) fn sort_values_booleans(&mut self) {
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
            let ChoiceValue::Boolean(va) = a else {
                unreachable!()
            };
            let ChoiceValue::Boolean(vb) = b else {
                unreachable!()
            };
            u8::from(*va).cmp(&u8::from(*vb))
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

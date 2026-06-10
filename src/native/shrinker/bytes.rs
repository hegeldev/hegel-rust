// Bytes shrink passes: `shrink_bytes` reduces individual `BytesChoice`
// nodes toward shorter / lex-smaller values, and `redistribute_bytes_pairs`
// rebalances length between adjacent bytes nodes for sum-of-length style
// predicates.

use std::collections::HashMap;

use crate::native::core::{BytesChoice, ChoiceKind, ChoiceValue};

use super::collection::CollectionAccess;
use super::{ShrinkResult, Shrinker, bin_search_down_r};

impl<'a> Shrinker<'a> {
    pub(super) fn shrink_bytes(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        // Fires the common-offset lowering for the *previous* node's
        // accepted shrinks at the top of each iteration (and once more
        // after the loop) — Hypothesis runs it after every successful
        // try_shrinking_nodes.
        let mut offset_epoch = self.improvements;
        while i < self.current_nodes.len() {
            self.lower_offset_if_shrunk(offset_epoch)?;
            offset_epoch = self.improvements;
            let min_size = match self.current_nodes[i].kind.as_ref() {
                ChoiceKind::Bytes(bc) => bc.min_size,
                _ => {
                    i += 1;
                    continue;
                }
            };

            // Hypothesis's `Bytes.shrink` is `Collection.shrink` with byte
            // values as their own order keys.
            let node_idx = i;
            let read = move |sh: &Shrinker<'_>| -> Option<Vec<u64>> {
                match sh.current_nodes.get(node_idx).map(|n| &n.value) {
                    Some(ChoiceValue::Bytes(v)) => Some(v.iter().map(|&b| u64::from(b)).collect()),
                    _ => None,
                }
            };
            let write = |keys: &[u64]| -> Option<ChoiceValue> {
                let mut out = Vec::with_capacity(keys.len());
                for &k in keys {
                    out.push(u8::try_from(k).ok()?);
                }
                Some(ChoiceValue::Bytes(out))
            };
            self.shrink_collection(
                node_idx,
                min_size,
                &CollectionAccess {
                    read: &read,
                    write: &write,
                },
            )?;

            i += 1;
        }
        self.lower_offset_if_shrunk(offset_epoch)?;
        Ok(())
    }

    fn current_byte_value(&self, i: usize) -> Vec<u8> {
        match &self.current_nodes[i].value {
            ChoiceValue::Bytes(v) => v.clone(),
            _ => unreachable!("kind/value invariant violated: outer match guaranteed this variant"),
        }
    }

    /// Try redistributing length between pairs of bytes values.
    ///
    /// For adjacent and skip-one-adjacent pairs of `BytesChoice` nodes,
    /// try moving bytes from the earlier node's value to the later one's.
    /// Useful for tests with a total-length constraint across two bytes
    /// values, where the minimal counterexample has the first as short
    /// as possible.
    pub(super) fn redistribute_bytes_pairs(&mut self) -> ShrinkResult<()> {
        for gap in 1..3usize {
            let mut idx = 0;
            loop {
                let indices = self.bytes_indices();
                if idx + gap >= indices.len() {
                    break;
                }
                let i = indices[idx];
                let j = indices[idx + gap];
                self.redistribute_bytes_pair(i, j)?;
                idx += 1;
            }
        }
        Ok(())
    }

    fn bytes_indices(&self) -> Vec<usize> {
        self.current_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| match n.kind.as_ref() {
                ChoiceKind::Bytes(_) => Some(i),
                _ => None,
            })
            .collect()
    }

    fn redistribute_bytes_pair(&mut self, i: usize, j: usize) -> ShrinkResult<()> {
        let s = self.current_byte_value(i);
        let t = self.current_byte_value(j);
        let kind_j = match self.current_nodes[j].kind.as_ref() {
            ChoiceKind::Bytes(kj) => kj.clone(),
            _ => unreachable!("kind/value invariant violated: outer match guaranteed this variant"),
        };

        if s.is_empty() {
            return Ok(());
        }

        // Try moving everything from s to t.
        let combined: Vec<u8> = s.iter().copied().chain(t.iter().copied()).collect();
        if self.try_redistribute_bytes(i, j, Vec::new(), combined, &kind_j)? {
            return Ok(());
        }

        // Try moving the last byte of s to the start of t.
        let (last, s_init) = s.split_last().unwrap();
        let mut t_prepended = Vec::with_capacity(t.len() + 1);
        t_prepended.push(*last);
        t_prepended.extend_from_slice(&t);
        if !self.try_redistribute_bytes(i, j, s_init.to_vec(), t_prepended, &kind_j)? {
            return Ok(());
        }

        // Binary search for the longest suffix of s that can be moved.
        let s_len = s.len();
        bin_search_down_r(1, s_len as i128, &mut |n| {
            let n = n as usize;
            let new_s = s[..s_len - n].to_vec();
            let mut new_t = s[s_len - n..].to_vec();
            new_t.extend_from_slice(&t);
            self.try_redistribute_bytes(i, j, new_s, new_t, &kind_j)
        })?;
        Ok(())
    }

    fn try_redistribute_bytes(
        &mut self,
        i: usize,
        j: usize,
        new_s: Vec<u8>,
        new_t: Vec<u8>,
        kind_j: &BytesChoice,
    ) -> ShrinkResult<bool> {
        if !kind_j.validate(&new_t) {
            return Ok(false);
        }
        self.replace(&HashMap::from([
            (i, ChoiceValue::Bytes(new_s)),
            (j, ChoiceValue::Bytes(new_t)),
        ]))
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_bytes_tests.rs"]
mod tests;

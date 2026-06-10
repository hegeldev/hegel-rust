// Port of Hypothesis's `shrinking/collection.py` and
// `shrinking/ordering.py`: a generic shrinker for a variable-length
// sequence node, expressed in element shrink-order keys.
//
// The element domain is `u64` order keys — byte values for `Bytes` nodes,
// alphabet shrink-order positions for `String` nodes — so the moves
// (deletion, reordering, joint duplicate minimization, per-element
// `Integer.shrink`) are written once, exactly as Python writes `Collection`
// once and instantiates `Bytes` / `String` from it.

use std::collections::HashMap;

use crate::native::core::ChoiceValue;

use super::integers::shrink_integer;
use super::{ShrinkResult, Shrinker, find_integer_r};

/// Conversion layer between a node's value and its key-vector form.
pub(super) struct CollectionAccess<'f> {
    /// Current value of the node as element order keys, or `None` when the
    /// node no longer holds a value of the expected kind (a concurrent
    /// shrink may pun or truncate it).
    pub read: &'f dyn Fn(&Shrinker<'_>) -> Option<Vec<u64>>,
    /// Rebuild a `ChoiceValue` from keys; `None` when a key has no
    /// representable element.
    pub write: &'f dyn Fn(&[u64]) -> Option<ChoiceValue>,
}

fn read(sh: &Shrinker<'_>, access: &CollectionAccess<'_>) -> Vec<u64> {
    (access.read)(sh).unwrap_or_default()
}

/// Propose the candidate key vector for the node. A candidate equal to the
/// node's current value reports `true` without executing (the `consider`
/// pre-check), matching Python's `Shrinker.consider` semantics that the
/// adaptive loops below rely on.
fn probe(
    sh: &mut Shrinker<'_>,
    node_idx: usize,
    access: &CollectionAccess<'_>,
    keys: &[u64],
) -> ShrinkResult<bool> {
    let Some(value) = (access.write)(keys) else {
        return Ok(false);
    };
    sh.replace(&HashMap::from([(node_idx, value)]))
}

impl<'a> Shrinker<'a> {
    /// `Collection.shrink` for the value held by `current_nodes[node_idx]`.
    pub(super) fn shrink_collection(
        &mut self,
        node_idx: usize,
        min_size: usize,
        access: &CollectionAccess<'_>,
    ) -> ShrinkResult<()> {
        // short_circuit: all-simplest at the minimum size. Success
        // (including vacuously, when that already is the value) ends the
        // shrink.
        if probe(self, node_idx, access, &vec![0u64; min_size])? {
            return Ok(());
        }

        // All-simplest at the *current* length — the probe that handles
        // values whose elements are linked (changing any one alone breaks
        // the predicate) in a single call.
        let len = read(self, access).len();
        probe(self, node_idx, access, &vec![0u64; len])?;

        // Adaptive deletion, back to front: the chunk ending at index `i`
        // grows via find_integer, so a run of deletable elements costs
        // O(log n) calls rather than O(n).
        let mut i = read(self, access).len() as isize - 1;
        while i >= 0 {
            let base = read(self, access);
            let iu = (i as usize).min(base.len().saturating_sub(1));
            if base.is_empty() {
                break;
            }
            let deleted = find_integer_r(|k| {
                if k > iu + 1 {
                    return Ok(false);
                }
                let mut cand = base[..iu + 1 - k].to_vec();
                cand.extend_from_slice(&base[iu + 1..]);
                probe(self, node_idx, access, &cand)
            })?;
            i = iu as isize - (deleted as isize).max(1);
        }

        // Reordering.
        self.ordering_shrink(node_idx, access)?;

        // Minimize all duplicated elements together — handles values where
        // several positions must hold the same element to keep failing.
        let cur = read(self, access);
        let mut counts: HashMap<u64, usize> = HashMap::new();
        for &k in &cur {
            *counts.entry(k).or_default() += 1;
        }
        let mut dups: Vec<u64> = counts
            .into_iter()
            .filter(|&(_, c)| c > 1)
            .map(|(k, _)| k)
            .collect();
        dups.sort_unstable();
        for val in dups {
            shrink_integer(u128::from(val), &mut |v| {
                let cur = read(self, access);
                let cand: Vec<u64> = cur
                    .iter()
                    .map(|&x| if x == val { v as u64 } else { x })
                    .collect();
                probe(self, node_idx, access, &cand)
            })?;
        }

        // Minimize each element in turn with the full Integer move set.
        let mut idx = 0;
        loop {
            let cur = read(self, access);
            if idx >= cur.len() {
                break;
            }
            shrink_integer(u128::from(cur[idx]), &mut |v| {
                let mut cand = read(self, access);
                if idx >= cand.len() {
                    return Ok(false);
                }
                cand[idx] = v as u64;
                probe(self, node_idx, access, &cand)
            })?;
            idx += 1;
        }
        Ok(())
    }

    /// `Ordering.shrink`: make the key vector more sorted without changing
    /// its multiset of elements.
    fn ordering_shrink(
        &mut self,
        node_idx: usize,
        access: &CollectionAccess<'_>,
    ) -> ShrinkResult<()> {
        // short_circuit: if the fully sorted form is accepted (or already
        // is the value), there is nothing more reordering can do.
        let cur = read(self, access);
        let mut sorted_keys = cur.clone();
        sorted_keys.sort_unstable();
        if probe(self, node_idx, access, &sorted_keys)? {
            return Ok(());
        }

        // sort_regions: guarantees every adjacent swap is attempted, by
        // adaptively sorting contiguous regions starting at each index.
        let mut i = 0;
        loop {
            let cur = read(self, access);
            if i + 1 >= cur.len() {
                break;
            }
            let k = find_integer_r(|k| {
                let cur = read(self, access);
                if i + k > cur.len() {
                    return Ok(false);
                }
                let mut cand = cur.clone();
                cand[i..i + k].sort_unstable();
                probe(self, node_idx, access, &cand)
            })?;
            i += k.max(1);
        }

        // sort_regions_with_gaps: guarantees every swap of index i with
        // i + 2 is attempted, by sorting regions around a fixed element.
        let len = read(self, access).len();
        for i in 1..len.saturating_sub(1) {
            let cur = read(self, access);
            if i + 1 >= cur.len() {
                break;
            }
            if cur[i - 1] <= cur[i] && cur[i] <= cur[i + 1] {
                continue;
            }
            let left = i;
            let mut right = i + 1;
            right +=
                find_integer_r(|k| self.gap_sort(node_idx, access, i, left as isize, right + k))?;
            find_integer_r(|k| {
                self.gap_sort(node_idx, access, i, left as isize - k as isize, right)
            })?;
        }
        Ok(())
    }

    /// `Ordering.sort_regions_with_gaps`' `can_sort(a, b)`: sort the
    /// elements of `[a, b)` around the pinned element at `i`.
    fn gap_sort(
        &mut self,
        node_idx: usize,
        access: &CollectionAccess<'_>,
        i: usize,
        a: isize,
        b: usize,
    ) -> ShrinkResult<bool> {
        if a < 0 {
            return Ok(false);
        }
        let a = a as usize;
        let cur = read(self, access);
        if b > cur.len() || !(a <= i && i < b) {
            return Ok(false);
        }
        let split = i - a;
        let mut values: Vec<u64> = cur[a..i].iter().chain(&cur[i + 1..b]).copied().collect();
        values.sort_unstable();
        let mut cand = cur[..a].to_vec();
        cand.extend_from_slice(&values[..split]);
        cand.push(cur[i]);
        cand.extend_from_slice(&values[split..]);
        cand.extend_from_slice(&cur[b..]);
        probe(self, node_idx, access, &cand)
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_collection_tests.rs"]
mod tests;

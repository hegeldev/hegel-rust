// Bytes shrink pass.
//
// Port of pbtkit's shrink_sequence applied to BytesChoice: try simplest,
// shorten (binary search + linear scan fallback), delete individual
// elements, reduce each byte toward 0, and insertion-sort to normalize
// order. Mirrors the Python version in pbtkit/shrinking/sequence.py.

use std::collections::HashMap;

use crate::native::core::{BytesChoice, ChoiceKind, ChoiceValue};

use super::{Shrinker, bin_search_down};

impl<'a> Shrinker<'a> {
    pub(super) fn shrink_bytes(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let (min_size, current) = match (
                self.current_nodes[i].kind.clone(),
                self.current_nodes[i].value.clone(),
            ) {
                (ChoiceKind::Bytes(bc), ChoiceValue::Bytes(v)) => (bc.min_size, v),
                _ => {
                    i += 1;
                    continue;
                }
            };

            // Try the simplest (min_size zeros) first.
            let simplest = vec![0u8; min_size];
            if simplest != current {
                self.replace(&HashMap::from([(i, ChoiceValue::Bytes(simplest))]));
            }

            // Shorten via binary search.
            let cur_len = self.current_byte_value(i).len();
            if cur_len > min_size {
                let captured = self.current_byte_value(i);
                bin_search_down(min_size as i128, cur_len as i128, &mut |sz| {
                    let sz = sz as usize;
                    let cand = captured[..sz].to_vec();
                    self.replace(&HashMap::from([(i, ChoiceValue::Bytes(cand))]))
                });
            }

            // Linear scan small lengths (non-monotonic fallback).
            let cur_len = self.current_byte_value(i).len();
            let scan_end = (min_size + 8).min(cur_len);
            for sz in min_size..scan_end {
                let cur = self.current_byte_value(i);
                if sz > cur.len() {
                    break;
                }
                let cand = cur[..sz].to_vec();
                self.replace(&HashMap::from([(i, ChoiceValue::Bytes(cand))]));
            }

            // Delete individual elements, from right to left.
            let mut j = self.current_byte_value(i).len();
            while j > 0 {
                j -= 1;
                let cur = self.current_byte_value(i);
                if cur.len() <= min_size {
                    continue;
                }
                let mut cand = cur.clone();
                cand.remove(j);
                self.replace(&HashMap::from([(i, ChoiceValue::Bytes(cand))]));
            }

            // Reduce each byte toward 0, from right to left.
            let mut j = self.current_byte_value(i).len();
            while j > 0 {
                j -= 1;
                let cur = self.current_byte_value(i);
                if cur[j] == 0 {
                    continue;
                }
                let hi = cur[j] as i128;
                bin_search_down(0, hi, &mut |e| {
                    let mut cand = self.current_byte_value(i);
                    cand[j] = e as u8;
                    self.replace(&HashMap::from([(i, ChoiceValue::Bytes(cand))]))
                });
            }

            // Insertion-sort pass: swap adjacent out-of-order bytes.
            let mut pos = 1;
            loop {
                let cur_len = self.current_byte_value(i).len();
                if pos >= cur_len {
                    break;
                }
                let mut j = pos;
                while j > 0 {
                    let cur = self.current_byte_value(i);
                    if cur[j - 1] <= cur[j] {
                        break;
                    }
                    let mut swapped = cur.clone();
                    swapped.swap(j - 1, j);
                    if self.replace(&HashMap::from([(i, ChoiceValue::Bytes(swapped))])) {
                        j -= 1;
                    } else {
                        break;
                    }
                }
                pos += 1;
            }

            i += 1;
        }
    }

    fn current_byte_value(&self, i: usize) -> Vec<u8> {
        match &self.current_nodes[i].value {
            ChoiceValue::Bytes(v) => v.clone(),
            _ => unreachable!(),
        }
    }

    /// Try redistributing length between pairs of bytes values.
    ///
    /// Port of pbtkit's `redistribute_bytes_pairs`
    /// (`shrinking/advanced_bytes_passes.py`). For adjacent and
    /// skip-one-adjacent pairs of `BytesChoice` nodes, try moving bytes
    /// from the earlier node's value to the later one's. Useful for
    /// tests with a total-length constraint across two bytes values,
    /// where the minimal counterexample has the first as short as
    /// possible.
    pub(super) fn redistribute_bytes_pairs(&mut self) {
        for gap in 1..3usize {
            let mut idx = 0;
            loop {
                let indices = self.bytes_indices();
                if idx + gap >= indices.len() {
                    break;
                }
                let i = indices[idx];
                let j = indices[idx + gap];
                self.redistribute_bytes_pair(i, j);
                idx += 1;
            }
        }
    }

    fn bytes_indices(&self) -> Vec<usize> {
        self.current_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| match &n.kind {
                ChoiceKind::Bytes(_) => Some(i),
                _ => None,
            })
            .collect()
    }

    fn redistribute_bytes_pair(&mut self, i: usize, j: usize) {
        let s = self.current_byte_value(i);
        let t = self.current_byte_value(j);
        let kind_j = match &self.current_nodes[j].kind {
            ChoiceKind::Bytes(kj) => kj.clone(),
            _ => unreachable!(),
        };

        if s.is_empty() {
            return;
        }

        // Port of pbtkit's `redistribute_sequence_pair`.

        // Try moving everything from s to t.
        let combined: Vec<u8> = s.iter().copied().chain(t.iter().copied()).collect();
        if self.try_redistribute_bytes(i, j, Vec::new(), combined, &kind_j) {
            return;
        }

        // Try moving the last byte of s to the start of t.
        let (last, s_init) = s.split_last().unwrap();
        let mut t_prepended = Vec::with_capacity(t.len() + 1);
        t_prepended.push(*last);
        t_prepended.extend_from_slice(&t);
        if !self.try_redistribute_bytes(i, j, s_init.to_vec(), t_prepended, &kind_j) {
            return;
        }

        // Binary search for the longest suffix of s that can be moved.
        let s_len = s.len();
        bin_search_down(1, s_len as i128, &mut |n| {
            let n = n as usize;
            let new_s = s[..s_len - n].to_vec();
            let mut new_t = s[s_len - n..].to_vec();
            new_t.extend_from_slice(&t);
            self.try_redistribute_bytes(i, j, new_s, new_t, &kind_j)
        });
    }

    fn try_redistribute_bytes(
        &mut self,
        i: usize,
        j: usize,
        new_s: Vec<u8>,
        new_t: Vec<u8>,
        kind_j: &BytesChoice,
    ) -> bool {
        if !kind_j.validate(&new_t) {
            return false;
        }
        self.replace(&HashMap::from([
            (i, ChoiceValue::Bytes(new_s)),
            (j, ChoiceValue::Bytes(new_t)),
        ]))
    }
}

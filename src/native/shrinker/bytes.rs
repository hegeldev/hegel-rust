// Bytes shrink pass.
//
// Port of pbtkit's shrink_sequence applied to BytesChoice: try simplest,
// shorten (binary search + linear scan fallback), delete individual
// elements, reduce each byte toward 0, and insertion-sort to normalize
// order. Mirrors the Python version in pbtkit/shrinking/sequence.py.

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceValue};

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
            let cur_len = self.current_byte_value(i).map(|v| v.len()).unwrap_or(0);
            if cur_len > min_size {
                let captured = self.current_byte_value(i).unwrap_or_default();
                bin_search_down(min_size as i128, cur_len as i128, &mut |sz| {
                    let sz = sz as usize;
                    if sz > captured.len() {
                        return false;
                    }
                    let cand = captured[..sz].to_vec();
                    self.replace(&HashMap::from([(i, ChoiceValue::Bytes(cand))]))
                });
            }

            // Linear scan small lengths (non-monotonic fallback).
            let cur_len = self.current_byte_value(i).map(|v| v.len()).unwrap_or(0);
            let scan_end = (min_size + 8).min(cur_len);
            for sz in min_size..scan_end {
                let Some(cur) = self.current_byte_value(i) else {
                    break;
                };
                if sz > cur.len() {
                    break;
                }
                let cand = cur[..sz].to_vec();
                self.replace(&HashMap::from([(i, ChoiceValue::Bytes(cand))]));
            }

            // Delete individual elements, from right to left.
            let mut j = self.current_byte_value(i).map(|v| v.len()).unwrap_or(0);
            while j > 0 {
                j -= 1;
                let Some(cur) = self.current_byte_value(i) else {
                    break;
                };
                if j >= cur.len() || cur.len() <= min_size {
                    continue;
                }
                let mut cand = cur.clone();
                cand.remove(j);
                self.replace(&HashMap::from([(i, ChoiceValue::Bytes(cand))]));
            }

            // Reduce each byte toward 0, from right to left.
            let mut j = self.current_byte_value(i).map(|v| v.len()).unwrap_or(0);
            while j > 0 {
                j -= 1;
                let Some(cur) = self.current_byte_value(i) else {
                    break;
                };
                if j >= cur.len() || cur[j] == 0 {
                    continue;
                }
                let hi = cur[j] as i128;
                bin_search_down(0, hi, &mut |e| {
                    let Some(cur_now) = self.current_byte_value(i) else {
                        return false;
                    };
                    if j >= cur_now.len() {
                        return false;
                    }
                    let mut cand = cur_now;
                    cand[j] = e as u8;
                    self.replace(&HashMap::from([(i, ChoiceValue::Bytes(cand))]))
                });
            }

            // Insertion-sort pass: swap adjacent out-of-order bytes.
            let mut pos = 1;
            loop {
                let cur_len = self.current_byte_value(i).map(|v| v.len()).unwrap_or(0);
                if pos >= cur_len {
                    break;
                }
                let mut j = pos;
                while j > 0 {
                    let Some(cur) = self.current_byte_value(i) else {
                        break;
                    };
                    if j >= cur.len() {
                        break;
                    }
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

    fn current_byte_value(&self, i: usize) -> Option<Vec<u8>> {
        match self.current_nodes.get(i).map(|n| &n.value) {
            Some(ChoiceValue::Bytes(v)) => Some(v.clone()),
            _ => None,
        }
    }
}

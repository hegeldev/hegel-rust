// String shrink pass.
//
// Port of pbtkit's shrink_sequence applied to StringChoice: try simplest,
// shorten (linear scan from min_size), delete single codepoints, reduce each
// codepoint toward the simplest codepoint in the alphabet, and
// insertion-sort to normalise by `codepoint_key`.
//
// The alphabet of valid codepoints comes from the StringChoice itself
// (min_codepoint..=max_codepoint, surrogates excluded). Reduction of a
// codepoint toward "simpler" is performed in `codepoint_key` space, which
// makes '0' the simplest, then '1', then the rest of ASCII in a specific
// order, then non-ASCII codepoints in natural order.
//
// Values are manipulated directly as `Vec<u32>` codepoints — no `char`
// round-tripping, no surrogate special-casing at this layer (the engine
// doesn't produce surrogates; `key_to_codepoint_in_range` filters them out
// of the reduction candidate set as a defensive measure).

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceValue, StringChoice, codepoint_key};

use super::{Shrinker, bin_search_down};

impl<'a> Shrinker<'a> {
    pub(super) fn shrink_strings(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let (kind, current) = match (
                self.current_nodes[i].kind.clone(),
                self.current_nodes[i].value.clone(),
            ) {
                (ChoiceKind::String(sc), ChoiceValue::String(s)) => (sc, s),
                _ => {
                    i += 1;
                    continue;
                }
            };

            // Step 1: try simplest.
            let simplest = kind.simplest();
            if simplest != current {
                self.replace(&HashMap::from([(i, ChoiceValue::String(simplest))]));
            }

            // Step 2: shorten via linear scan up from min_size. For strings the
            // per-codepoint key is not monotonic under prefix-taking (the suffix
            // we drop may have been the only "interesting" part), so a linear
            // scan is simpler and small.
            let cur_len = self.current_string(i).len();
            if cur_len > kind.min_size {
                for target_len in kind.min_size..cur_len {
                    let cand: Vec<u32> = self.current_string(i)[..target_len].to_vec();
                    if self.replace(&HashMap::from([(i, ChoiceValue::String(cand))])) {
                        break;
                    }
                }
            }

            // Step 3: delete individual codepoints, right-to-left.
            let mut j = self.current_string(i).len();
            while j > 0 {
                j -= 1;
                let cur = self.current_string(i);
                if cur.len() <= kind.min_size {
                    continue;
                }
                let mut cand = cur.clone();
                cand.remove(j);
                self.replace(&HashMap::from([(i, ChoiceValue::String(cand))]));
            }

            // Step 4: reduce each codepoint toward the simplest codepoint.
            let mut j = self.current_string(i).len();
            while j > 0 {
                j -= 1;
                let current_key = codepoint_key(self.current_string(i)[j]);
                if current_key == 0 {
                    continue;
                }
                // Scan candidate keys from 0 up to current_key-1, accepting
                // the first that produces an interesting reduction.
                for k in 0..current_key {
                    let Some(cp) = key_to_codepoint_in_range(k, &kind) else {
                        continue;
                    };
                    let mut cand = self.current_string(i);
                    cand[j] = cp;
                    if self.replace(&HashMap::from([(i, ChoiceValue::String(cand))])) {
                        break;
                    }
                }
            }

            // Step 5: insertion-sort pass — swap adjacent out-of-order
            // codepoints (under codepoint_key ordering).
            let mut pos = 1;
            loop {
                let cur_len = self.current_string(i).len();
                if pos >= cur_len {
                    break;
                }
                let mut j = pos;
                while j > 0 {
                    let cur = self.current_string(i);
                    let prev_key = codepoint_key(cur[j - 1]);
                    let cur_key = codepoint_key(cur[j]);
                    if prev_key <= cur_key {
                        break;
                    }
                    let mut swapped = cur.clone();
                    swapped.swap(j - 1, j);
                    if self.replace(&HashMap::from([(i, ChoiceValue::String(swapped))])) {
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

    fn current_string(&self, i: usize) -> Vec<u32> {
        match &self.current_nodes[i].value {
            ChoiceValue::String(s) => s.clone(),
            _ => unreachable!(),
        }
    }

    /// Try redistributing length between pairs of string values.
    ///
    /// Port of pbtkit's `redistribute_string_pairs`
    /// (`shrinking/advanced_string_passes.py`). For adjacent and
    /// skip-one-adjacent pairs of `StringChoice` nodes, try moving
    /// characters from the earlier node's value to the later one's.
    /// Useful for tests with a total-length constraint across two
    /// strings, where the minimal counterexample has the first string
    /// as short as possible.
    pub(super) fn redistribute_string_pairs(&mut self) {
        for gap in 1..3usize {
            let mut idx = 0;
            loop {
                let indices = self.string_indices();
                if idx + gap >= indices.len() {
                    break;
                }
                let i = indices[idx];
                let j = indices[idx + gap];
                self.redistribute_string_pair(i, j);
                idx += 1;
            }
        }
    }

    fn string_indices(&self) -> Vec<usize> {
        self.current_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| match &n.kind {
                ChoiceKind::String(_) => Some(i),
                _ => None,
            })
            .collect()
    }

    fn redistribute_string_pair(&mut self, i: usize, j: usize) {
        let s = self.current_string(i);
        let t = self.current_string(j);
        let kind_j = match &self.current_nodes[j].kind {
            ChoiceKind::String(kj) => kj.clone(),
            _ => unreachable!(),
        };

        if s.is_empty() {
            return;
        }

        // Port of pbtkit's `redistribute_sequence_pair`.

        // Try moving everything from s to t.
        let combined: Vec<u32> = s.iter().copied().chain(t.iter().copied()).collect();
        if self.try_redistribute(i, j, Vec::new(), combined, &kind_j) {
            return;
        }

        // Try moving the last codepoint of s to the start of t.
        let (last, s_init) = s.split_last().unwrap();
        let mut t_prepended = Vec::with_capacity(t.len() + 1);
        t_prepended.push(*last);
        t_prepended.extend_from_slice(&t);
        if !self.try_redistribute(i, j, s_init.to_vec(), t_prepended, &kind_j) {
            return;
        }

        // Binary search for the longest suffix of s that can be moved.
        let s_len = s.len();
        bin_search_down(1, s_len as i128, &mut |n| {
            let n = n as usize;
            let new_s = s[..s_len - n].to_vec();
            let mut new_t = s[s_len - n..].to_vec();
            new_t.extend_from_slice(&t);
            self.try_redistribute(i, j, new_s, new_t, &kind_j)
        });
    }

    fn try_redistribute(
        &mut self,
        i: usize,
        j: usize,
        new_s: Vec<u32>,
        new_t: Vec<u32>,
        kind_j: &StringChoice,
    ) -> bool {
        if !kind_j.validate(&new_t) {
            return false;
        }
        self.replace(&HashMap::from([
            (i, ChoiceValue::String(new_s)),
            (j, ChoiceValue::String(new_t)),
        ]))
    }
}

/// Return the codepoint corresponding to sort-key `k`, if it lies within the
/// valid range (excluding surrogates).
pub(super) fn key_to_codepoint_in_range(k: u32, kind: &StringChoice) -> Option<u32> {
    let cp = if k < 128 { (k + b'0' as u32) % 128 } else { k };
    if cp < kind.min_codepoint || cp > kind.max_codepoint {
        return None;
    }
    if (0xD800..=0xDFFF).contains(&cp) {
        return None;
    }
    Some(cp)
}

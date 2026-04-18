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

use super::Shrinker;

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
            let cur_len = self.current_string(i).map(|v| v.len()).unwrap_or(0);
            if cur_len > kind.min_size {
                for target_len in kind.min_size..cur_len {
                    let Some(cur) = self.current_string(i) else {
                        break;
                    };
                    if target_len >= cur.len() {
                        break;
                    }
                    let cand: Vec<u32> = cur[..target_len].to_vec();
                    if self.replace(&HashMap::from([(i, ChoiceValue::String(cand))])) {
                        break;
                    }
                }
            }

            // Step 3: delete individual codepoints, right-to-left.
            let mut j = self.current_string(i).map(|v| v.len()).unwrap_or(0);
            while j > 0 {
                j -= 1;
                let Some(cur) = self.current_string(i) else {
                    break;
                };
                if j >= cur.len() || cur.len() <= kind.min_size {
                    continue;
                }
                let mut cand = cur.clone();
                cand.remove(j);
                self.replace(&HashMap::from([(i, ChoiceValue::String(cand))]));
            }

            // Step 4: reduce each codepoint toward the simplest codepoint.
            let mut j = self.current_string(i).map(|v| v.len()).unwrap_or(0);
            while j > 0 {
                j -= 1;
                let Some(cur) = self.current_string(i) else {
                    break;
                };
                if j >= cur.len() {
                    continue;
                }
                let current_key = codepoint_key(cur[j]);
                if current_key == 0 {
                    continue;
                }
                // Scan candidate keys from 0 up to current_key-1, accepting
                // the first that produces an interesting reduction.
                for k in 0..current_key {
                    let Some(cp) = key_to_codepoint_in_range(k, &kind) else {
                        continue;
                    };
                    let Some(cur_now) = self.current_string(i) else {
                        break;
                    };
                    if j >= cur_now.len() {
                        break;
                    }
                    let mut cand = cur_now.clone();
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
                let cur_len = self.current_string(i).map(|v| v.len()).unwrap_or(0);
                if pos >= cur_len {
                    break;
                }
                let mut j = pos;
                while j > 0 {
                    let Some(cur) = self.current_string(i) else {
                        break;
                    };
                    if j >= cur.len() {
                        break;
                    }
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

    fn current_string(&self, i: usize) -> Option<Vec<u32>> {
        match self.current_nodes.get(i).map(|n| &n.value) {
            Some(ChoiceValue::String(s)) => Some(s.clone()),
            _ => None,
        }
    }
}

/// Return the codepoint corresponding to sort-key `k`, if it lies within the
/// valid range (excluding surrogates).
fn key_to_codepoint_in_range(k: u32, kind: &StringChoice) -> Option<u32> {
    let cp = if k < 128 { (k + b'0' as u32) % 128 } else { k };
    if cp < kind.min_codepoint || cp > kind.max_codepoint {
        return None;
    }
    if (0xD800..=0xDFFF).contains(&cp) {
        return None;
    }
    Some(cp)
}

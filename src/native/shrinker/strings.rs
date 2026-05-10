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
use crate::native::unicodedata;

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

            // Step 3.5: shrink duplicated codepoints simultaneously.
            //
            // When two or more positions hold the same codepoint and the
            // predicate links them (e.g. `decode(rle_encode(s)) != s`
            // requires at least two positions to share a value to trigger
            // the bug), reducing one position alone breaks the link. This
            // pass tries replacing *every* instance of a duplicated
            // codepoint at once, mirroring the per-value pass in
            // Hypothesis's `Collection.run_step`.
            let dup_codepoints: Vec<u32> = {
                let cur = self.current_string(i);
                let mut counts: HashMap<u32, usize> = HashMap::new();
                for &cp in &cur {
                    *counts.entry(cp).or_default() += 1;
                }
                // E9: sort the duplicated-codepoint list by `codepoint_key`
                // (the shrink-towards order this pass uses for candidate
                // generation) so the iteration order is deterministic
                // regardless of `HashMap`'s unspecified bucketing.
                // Pre-fix the order varied across builds and could shadow
                // shrink-quality regressions that show up only on a
                // particular hash seed.
                let mut dups: Vec<u32> = counts
                    .into_iter()
                    .filter(|(_, n)| *n > 1)
                    .map(|(cp, _)| cp)
                    .collect();
                dups.sort_by_key(|&cp| codepoint_key(cp));
                dups
            };
            for val in dup_codepoints {
                if codepoint_key(val) == 0 {
                    continue;
                }
                // Skip if the previous step replaced every instance of `val`
                // already.
                if !self.current_string(i).contains(&val) {
                    continue;
                }

                let try_replace_all = |sh: &mut Shrinker<'_>, cand_cp: u32| -> bool {
                    let mut new_str = sh.current_string(i);
                    let mut changed = false;
                    for c in new_str.iter_mut() {
                        if *c == val {
                            *c = cand_cp;
                            changed = true;
                        }
                    }
                    if !changed {
                        return false;
                    }
                    sh.replace(&HashMap::from([(i, ChoiceValue::String(new_str))]))
                };

                for cand_cp in semantic_candidates(val, &kind) {
                    if codepoint_key(cand_cp) >= codepoint_key(val) {
                        continue;
                    }
                    try_replace_all(self, cand_cp);
                    if !self.current_string(i).contains(&val) {
                        break;
                    }
                }

                if self.current_string(i).contains(&val) {
                    let cur_key = codepoint_key(val);
                    if cur_key > 0 {
                        bin_search_down(0, cur_key as i128, &mut |k| {
                            let Some(cp) = key_to_codepoint_in_range(k as u32, &kind) else {
                                return false;
                            };
                            try_replace_all(self, cp)
                        });
                    }
                }
            }

            // Step 4: reduce each codepoint via a small set of semantic
            // candidates (digits, ASCII letters, NFD base) followed by
            // `bin_search_down` over the remaining key range.
            //
            // Why not a linear scan over all keys < current_key? The default
            // `gs::text()` alphabet has ~1.1M valid codepoints, so a worst-
            // case scan from a high-codepoint character is prohibitive.
            //
            // Why not just `bin_search_down`? It's not robust to non-monotone
            // predicates: midpoint probes can miss valid simpler characters
            // sitting between failing midpoints (e.g. 'A' at key 17 when
            // shrinking from 'À' at key 192 — bin_search probes 96, 144, 168,
            // ..., never trying 17). This is the same basin trap that
            // afflicts upstream Hypothesis's per-element Integer shrinker
            // (HypothesisWorks/hypothesis#4725).
            //
            // The hybrid: try a fixed list of "obvious smaller candidates"
            // first to cover the common ASCII / Latin-with-diacritic basins,
            // then `bin_search_down` for the long tail. Bounded at roughly
            // 62 + log2(0x10FFFF) ≈ 84 probes per character per pass.
            let mut j = self.current_string(i).len();
            while j > 0 {
                j -= 1;
                if codepoint_key(self.current_string(i)[j]) == 0 {
                    continue;
                }
                let original_cp = self.current_string(i)[j];

                for cand_cp in semantic_candidates(original_cp, &kind) {
                    let cur_key = codepoint_key(self.current_string(i)[j]);
                    if codepoint_key(cand_cp) >= cur_key {
                        continue;
                    }
                    let mut cand = self.current_string(i);
                    cand[j] = cand_cp;
                    self.replace(&HashMap::from([(i, ChoiceValue::String(cand))]));
                }

                let cur_key = codepoint_key(self.current_string(i)[j]);
                if cur_key > 0 {
                    bin_search_down(0, cur_key as i128, &mut |k| {
                        let Some(cp) = key_to_codepoint_in_range(k as u32, &kind) else {
                            return false;
                        };
                        let mut cand = self.current_string(i);
                        cand[j] = cp;
                        self.replace(&HashMap::from([(i, ChoiceValue::String(cand))]))
                    });
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
            _ => unreachable!("kind/value invariant violated: outer match guaranteed this variant"),
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
            _ => unreachable!("kind/value invariant violated: outer match guaranteed this variant"),
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
    let cp = crate::native::core::key_to_codepoint(k);
    if cp < kind.min_codepoint || cp > kind.max_codepoint {
        return None;
    }
    if (0xD800..=0xDFFF).contains(&cp) {
        return None;
    }
    Some(cp)
}

/// "Obvious smaller" replacement codepoints to try for a character with
/// codepoint `cp` in a [`StringChoice`] with the given alphabet, in
/// shrink-key order. Filtered to in-range, non-surrogate values only.
///
/// Roughly: ASCII digits, then ASCII uppercase letters, then ASCII lowercase
/// letters, then the recursive NFD base of `cp` if it's a non-ASCII
/// codepoint with a canonical decomposition (e.g. `À` → `A`, `ñ` → `n`).
/// Pure ASCII inputs already fall inside the digit/letter ranges so NFD
/// adds nothing for them and is skipped.
///
/// Used by `shrink_strings` to escape predicate basins where neither a
/// pure binary search nor a Hypothesis-style `find_integer` descent would
/// reach the smaller-key target.
fn semantic_candidates(cp: u32, kind: &StringChoice) -> Vec<u32> {
    let mut out = Vec::with_capacity(64);

    let push_key = |out: &mut Vec<u32>, k: u32| {
        if let Some(cp) = key_to_codepoint_in_range(k, kind) {
            out.push(cp);
        }
    };

    // '0' (key 0) is also covered by Step 1's `kind.simplest()`, but
    // re-trying it per-position handles strings whose simplest_codepoint
    // pass lost the lock when an earlier position couldn't shrink.
    push_key(&mut out, 0);

    // Digits '1'..'9'.
    for k in 1..=9 {
        push_key(&mut out, k);
    }

    // ASCII uppercase 'A'..'Z' (keys 17..=42).
    for k in 17..=42 {
        push_key(&mut out, k);
    }

    // ASCII lowercase 'a'..'z' (keys 49..=74).
    for k in 49..=74 {
        push_key(&mut out, k);
    }

    if cp >= 0x80 {
        if let Some(base) = unicodedata::nfd_base(cp) {
            if base >= kind.min_codepoint
                && base <= kind.max_codepoint
                && !(0xD800..=0xDFFF).contains(&base)
                && codepoint_key(base) < codepoint_key(cp)
            {
                out.push(base);
            }
        }
    }

    out
}

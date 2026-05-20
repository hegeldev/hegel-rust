// String shrink passes. The main `shrink_strings` pass runs, for each
// `StringChoice` node in the choice sequence: try the simplest value;
// shorten from `min_size` upward; delete single codepoints; reduce each
// codepoint toward the alphabet's simplest in shrink-order; and
// insertion-sort the resulting codepoints. `redistribute_string_pairs`
// moves codepoints between adjacent string nodes for sum-of-length-style
// predicates.
//
// Reduction order is alphabet-relative: `StringChoice::codepoint_key`
// returns each codepoint's position in `IntervalSet::char_in_shrink_order`,
// so shrinking on `[a-z]` walks toward `'a'` while shrinking on
// `[0-9A-Za-z]` walks toward `'0'`. Mirrors Hypothesis's per-element
// shrinking in `internal/conjecture/shrinking/string.py`.

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceValue, StringChoice};
use crate::unicodedata;

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
                // Sort the duplicated-codepoint list by alphabet-relative
                // shrink-order position so the iteration order is
                // deterministic regardless of `HashMap`'s unspecified
                // bucketing.
                let mut dups: Vec<u32> = counts
                    .into_iter()
                    .filter(|(_, n)| *n > 1)
                    .map(|(cp, _)| cp)
                    .collect();
                dups.sort_by_key(|&cp| kind.codepoint_key(cp));
                dups
            };
            for val in dup_codepoints {
                if kind.codepoint_key(val) == 0 {
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
                    // `semantic_candidates` only returns codepoints with
                    // strictly smaller shrink-key than `val`.
                    try_replace_all(self, cand_cp);
                    if !self.current_string(i).contains(&val) {
                        break;
                    }
                }

                if self.current_string(i).contains(&val) {
                    let cur_key = kind.codepoint_key(val);
                    if cur_key > 0 {
                        bin_search_down(0, cur_key as i128, &mut |k| {
                            // `key_to_codepoint(k)` is `Some` for every
                            // `k < alpha_size`, and our upper bound `cur_key`
                            // is itself a valid position in the alphabet.
                            let cp = kind
                                .key_to_codepoint(k as u32)
                                .expect("bin_search probe stays within alpha_size");
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
            // shrinking from 'À' at a higher key — bin_search probes
            // midpoints and might miss the basin). Same trap as upstream
            // Hypothesis's per-element Integer shrinker
            // (HypothesisWorks/hypothesis#4725).
            //
            // The hybrid: try a fixed list of "obvious smaller candidates"
            // first to cover the common ASCII / Latin-with-diacritic basins,
            // then `bin_search_down` for the long tail.
            let mut j = self.current_string(i).len();
            while j > 0 {
                j -= 1;
                if kind.codepoint_key(self.current_string(i)[j]) == 0 {
                    continue;
                }
                let original_cp = self.current_string(i)[j];

                for cand_cp in semantic_candidates(original_cp, &kind) {
                    let cur_key = kind.codepoint_key(self.current_string(i)[j]);
                    if kind.codepoint_key(cand_cp) >= cur_key {
                        continue;
                    }
                    let mut cand = self.current_string(i);
                    cand[j] = cand_cp;
                    self.replace(&HashMap::from([(i, ChoiceValue::String(cand))]));
                }

                let cur_key = kind.codepoint_key(self.current_string(i)[j]);
                if cur_key > 0 {
                    bin_search_down(0, cur_key as i128, &mut |k| {
                        let cp = kind
                            .key_to_codepoint(k as u32)
                            .expect("bin_search probe stays within alpha_size");
                        let mut cand = self.current_string(i);
                        cand[j] = cp;
                        self.replace(&HashMap::from([(i, ChoiceValue::String(cand))]))
                    });
                }
            }

            // Step 5: insertion-sort pass — swap adjacent out-of-order
            // codepoints (under the alphabet's shrink ordering).
            let mut pos = 1;
            loop {
                let cur_len = self.current_string(i).len();
                if pos >= cur_len {
                    break;
                }
                let mut j = pos;
                while j > 0 {
                    let cur = self.current_string(i);
                    let prev_key = kind.codepoint_key(cur[j - 1]);
                    let cur_key = kind.codepoint_key(cur[j]);
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

    /// Try redistributing length between pairs of string values. For
    /// adjacent and skip-one-adjacent pairs of `StringChoice` nodes, move
    /// codepoints from the earlier node's value to the later one's —
    /// useful for tests with a total-length constraint across two strings,
    /// where the minimal counterexample has the first string as short as
    /// possible.
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

    /// For each pair of string nodes within distance 4, lower every
    /// occurrence of a shared codepoint in *both* strings simultaneously.
    ///
    /// Port of `shrinker.py:1519-1581` (`lower_duplicated_characters`).
    /// Handles the case where two strings must contain the same
    /// character but the actual character value is free — we want to
    /// drive both occurrences toward the alphabet's smallest member at
    /// once.
    pub(crate) fn lower_duplicated_characters(&mut self) {
        let len = self.current_nodes.len();
        for i in 0..len {
            for j in (i + 1)..(i + 1 + 4).min(len) {
                // Both must be String kinds.
                let (kind_i, val_i) =
                    match (&self.current_nodes[i].kind, &self.current_nodes[i].value) {
                        (ChoiceKind::String(k), ChoiceValue::String(v)) => (k.clone(), v.clone()),
                        _ => continue,
                    };
                let (kind_j, val_j) =
                    match (&self.current_nodes[j].kind, &self.current_nodes[j].value) {
                        (ChoiceKind::String(k), ChoiceValue::String(v)) => (k.clone(), v.clone()),
                        _ => continue,
                    };
                let set_i: std::collections::BTreeSet<u32> = val_i.iter().copied().collect();
                let set_j: std::collections::BTreeSet<u32> = val_j.iter().copied().collect();
                let shared: Vec<u32> = set_i.intersection(&set_j).copied().collect();
                for ch in shared {
                    // Binary-search the codepoint key downward.
                    let original_key = kind_i.codepoint_key(ch);
                    if original_key == 0 {
                        continue;
                    }
                    super::bin_search_down(0, original_key as i128, &mut |new_key| {
                        // `key_to_codepoint(new_key)` is `Some` for
                        // every key in `0..alpha_size`, and our search
                        // upper bound is `original_key` which is itself
                        // a valid alphabet position.  Likewise the
                        // resulting `new_cp` differs from `ch` (whose
                        // key was `original_key > new_key`) and the
                        // validate calls succeed since both strings
                        // stay within the alphabet.
                        let new_cp = kind_i
                            .key_to_codepoint(new_key as u32)
                            .expect("key < original_key < alpha_size");
                        debug_assert_ne!(new_cp, ch);
                        let new_i: Vec<u32> = val_i
                            .iter()
                            .map(|&c| if c == ch { new_cp } else { c })
                            .collect();
                        let new_j: Vec<u32> = val_j
                            .iter()
                            .map(|&c| if c == ch { new_cp } else { c })
                            .collect();
                        debug_assert!(kind_i.validate(&new_i) && kind_j.validate(&new_j));
                        self.replace(&HashMap::from([
                            (i, ChoiceValue::String(new_i)),
                            (j, ChoiceValue::String(new_j)),
                        ]))
                    });
                }
            }
        }
    }

    /// Walk every string node and try replacing each codepoint with one
    /// of its "natural simpler" variants — NFD base + case mappings.
    ///
    /// Port of `shrinker.py:1583-1617` (`normalize_unicode_chars`).
    /// Complements `shrink_strings`' per-position search by trying the
    /// semantically obvious replacements that lex-index bisection can
    /// skip over.
    pub(crate) fn normalize_unicode_chars(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let (kind, value) = match (&self.current_nodes[i].kind, &self.current_nodes[i].value) {
                (ChoiceKind::String(k), ChoiceValue::String(v)) => (k.clone(), v.clone()),
                _ => {
                    i += 1;
                    continue;
                }
            };
            for pos in 0..value.len() {
                let cp = value[pos];
                let candidates = natural_simpler_chars(cp, &kind);
                // `current_nodes[i]` is the same kind we matched at the
                // top of the loop; only its value may have changed
                // under intervening `replace` calls.
                let cur = match &self.current_nodes[i].value {
                    ChoiceValue::String(v) => v.clone(),
                    _ => unreachable!("kind invariant violated mid-pass"),
                };
                if pos >= cur.len() || cur[pos] != cp {
                    continue;
                }
                for replacement in candidates {
                    let mut new_value = cur.clone();
                    new_value[pos] = replacement;
                    // `natural_simpler_chars` already filters
                    // candidates to those `intervals.contains(c)`, and
                    // the alphabet check is the only validate gate for
                    // single-char replacements at fixed-length —
                    // therefore the candidate is always valid.
                    debug_assert!(kind.validate(&new_value));
                    if self.replace(&HashMap::from([(i, ChoiceValue::String(new_value))])) {
                        break;
                    }
                }
            }
            i += 1;
        }
    }
}

/// "Obvious smaller" replacement codepoints to try for a character with
/// codepoint `cp` in a [`StringChoice`] with the given alphabet, in
/// shrink-key order. Walks the first 62 alphabet positions (digits + ASCII
/// letters when present) and then the NFD base of `cp` (e.g. `'À' → 'A'`)
/// if it's a non-ASCII codepoint with a canonical decomposition that lands
/// in-alphabet.
///
/// Cross-string codepoint candidates from natural text transformations.
///
/// Port of Hypothesis's `_natural_simpler_chars` (`shrinker.py:94-119`).
/// For codepoint `cp` under alphabet `intervals`, returns the
/// candidates produced by:
///
/// * NFD decomposition (collapsing accented forms onto their base).
/// * `to_lowercase` and `to_uppercase` case mappings.
///
/// Candidates are filtered to those that (a) lie inside `intervals`
/// and (b) have a strictly smaller shrink-order key than the original,
/// then sorted by that key.  Used by `normalize_unicode_chars` to
/// directly try the most semantically obvious replacements.
fn natural_simpler_chars(cp: u32, kind: &StringChoice) -> Vec<u32> {
    use std::collections::BTreeSet;
    let cur_key = kind.codepoint_key(cp);
    let mut candidates: BTreeSet<u32> = BTreeSet::new();
    if let Some(c) = char::from_u32(cp) {
        for sub in c.to_lowercase() {
            candidates.insert(sub as u32);
        }
        for sub in c.to_uppercase() {
            candidates.insert(sub as u32);
        }
    }
    if let Some(base) = unicodedata::nfd_base(cp) {
        candidates.insert(base);
    }
    candidates.remove(&cp);
    let mut filtered: Vec<(u32, u32)> = candidates
        .into_iter()
        .filter(|c| kind.intervals.contains(*c) && kind.codepoint_key(*c) < cur_key)
        .map(|c| (kind.codepoint_key(c), c))
        .collect();
    filtered.sort();
    filtered.into_iter().map(|(_, c)| c).collect()
}

/// Used by `shrink_strings` to escape predicate basins where neither a
/// pure binary search nor a Hypothesis-style `find_integer` descent would
/// reach the smaller-key target.
fn semantic_candidates(cp: u32, kind: &StringChoice) -> Vec<u32> {
    let mut out = Vec::with_capacity(64);
    let cur_key = kind.codepoint_key(cp);

    // The first ~62 alphabet positions in shrink order are digits + ASCII
    // letters when the alphabet contains them. Walking them directly gives
    // exactly the "ASCII basin" candidates without needing fixed key indices.
    let cap = 62u32.min(kind.alpha_size() as u32);
    for k in 0..cap {
        if k >= cur_key {
            break;
        }
        if let Some(c) = kind.key_to_codepoint(k) {
            out.push(c);
        }
    }

    if cp >= 0x80 {
        if let Some(base) = unicodedata::nfd_base(cp) {
            if kind.intervals.contains(base) && kind.codepoint_key(base) < cur_key {
                out.push(base);
            }
        }
    }

    out
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_strings_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_string_passes_tests.rs"]
mod string_passes_tests;

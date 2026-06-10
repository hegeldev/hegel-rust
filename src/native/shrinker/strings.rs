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
// `[0-9A-Za-z]` walks toward `'0'`.

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceValue, StringChoice};
use crate::unicodedata;

use super::collection::CollectionAccess;
use super::{ShrinkResult, Shrinker, bin_search_down_r};

impl<'a> Shrinker<'a> {
    pub(super) fn shrink_strings(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        // Fires the common-offset lowering for the *previous* node's
        // accepted shrinks at the top of each iteration (and once more
        // after the loop) — Hypothesis runs it after every successful
        // try_shrinking_nodes.
        let mut offset_epoch = self.improvements;
        while i < self.current_nodes.len() {
            self.lower_offset_if_shrunk(offset_epoch)?;
            offset_epoch = self.improvements;
            let kind = match self.current_nodes[i].kind.as_ref() {
                ChoiceKind::String(sc) => sc.clone(),
                _ => {
                    i += 1;
                    continue;
                }
            };

            // Hypothesis's `String.shrink` is `Collection.shrink` over
            // alphabet shrink-order keys: the all-simplest probes, adaptive
            // back-to-front deletion, reordering, joint duplicate
            // minimization, and per-element Integer moves all live there.
            let node_idx = i;
            let kind_for_read = kind.clone();
            let read = move |sh: &Shrinker<'_>| -> Option<Vec<u64>> {
                match sh.current_nodes.get(node_idx).map(|n| &n.value) {
                    Some(ChoiceValue::String(s)) => Some(
                        s.iter()
                            .map(|&cp| u64::from(kind_for_read.codepoint_key(cp)))
                            .collect(),
                    ),
                    _ => None,
                }
            };
            let kind_for_write = kind.clone();
            let write = move |keys: &[u64]| -> Option<ChoiceValue> {
                let mut out = Vec::with_capacity(keys.len());
                for &k in keys {
                    let k = u32::try_from(k).ok()?;
                    out.push(kind_for_write.key_to_codepoint(k)?);
                }
                Some(ChoiceValue::String(out))
            };
            self.shrink_collection(
                node_idx,
                kind.min_size,
                &CollectionAccess {
                    read: &read,
                    write: &write,
                },
            )?;

            // A realised run may have punned the node to a different kind
            // while the collection driver was accepting candidates; the
            // remaining string-specific sections require a live String.
            if !matches!(self.current_nodes[i].value, ChoiceValue::String(_)) {
                i += 1;
                continue;
            }

            // Shrink duplicated codepoints simultaneously.
            //
            // When two or more positions hold the same codepoint and the
            // predicate links them (e.g. `decode(rle_encode(s)) != s`
            // requires at least two positions to share a value to trigger
            // the bug), reducing one position alone breaks the link. This
            // pass tries replacing *every* instance of a duplicated
            // codepoint at once.
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

                let try_replace_all = |sh: &mut Shrinker<'_>, cand_cp: u32| -> ShrinkResult<bool> {
                    let mut new_str = sh.current_string(i);
                    let mut changed = false;
                    for c in new_str.iter_mut() {
                        if *c == val {
                            *c = cand_cp;
                            changed = true;
                        }
                    }
                    if !changed {
                        return Ok(false);
                    }
                    sh.replace(&HashMap::from([(i, ChoiceValue::String(new_str))]))
                };

                for cand_cp in semantic_candidates(val, &kind) {
                    // `semantic_candidates` only returns codepoints with
                    // strictly smaller shrink-key than `val`.
                    try_replace_all(self, cand_cp)?;
                    if !self.current_string(i).contains(&val) {
                        break;
                    }
                }
            }

            // Hegel extra on top of Collection.shrink: per-position
            // semantic candidates (digits, ASCII letters, NFD base). The
            // Integer moves search the key space numerically; these jump
            // straight into the common ASCII / Latin-with-diacritic basins
            // that a numeric descent over a ~1.1M-codepoint alphabet can
            // miss under non-monotone predicates.
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
                    self.replace(&HashMap::from([(i, ChoiceValue::String(cand))]))?;
                }
            }

            i += 1;
        }
        self.lower_offset_if_shrunk(offset_epoch)?;
        Ok(())
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
    pub(super) fn redistribute_string_pairs(&mut self) -> ShrinkResult<()> {
        for gap in 1..3usize {
            let mut idx = 0;
            loop {
                let indices = self.string_indices();
                if idx + gap >= indices.len() {
                    break;
                }
                let i = indices[idx];
                let j = indices[idx + gap];
                self.redistribute_string_pair(i, j)?;
                idx += 1;
            }
        }
        Ok(())
    }

    fn string_indices(&self) -> Vec<usize> {
        self.current_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| match n.kind.as_ref() {
                ChoiceKind::String(_) => Some(i),
                _ => None,
            })
            .collect()
    }

    fn redistribute_string_pair(&mut self, i: usize, j: usize) -> ShrinkResult<()> {
        let s = self.current_string(i);
        let t = self.current_string(j);
        let kind_j = match self.current_nodes[j].kind.as_ref() {
            ChoiceKind::String(kj) => kj.clone(),
            _ => unreachable!("kind/value invariant violated: outer match guaranteed this variant"),
        };

        if s.is_empty() {
            return Ok(());
        }

        // Try moving everything from s to t.
        let combined: Vec<u32> = s.iter().copied().chain(t.iter().copied()).collect();
        if self.try_redistribute(i, j, Vec::new(), combined, &kind_j)? {
            return Ok(());
        }

        // Try moving the last codepoint of s to the start of t.
        let (last, s_init) = s.split_last().unwrap();
        let mut t_prepended = Vec::with_capacity(t.len() + 1);
        t_prepended.push(*last);
        t_prepended.extend_from_slice(&t);
        if !self.try_redistribute(i, j, s_init.to_vec(), t_prepended, &kind_j)? {
            return Ok(());
        }

        // Binary search for the longest suffix of s that can be moved.
        let s_len = s.len();
        bin_search_down_r(1, s_len as i128, &mut |n| {
            let n = n as usize;
            let new_s = s[..s_len - n].to_vec();
            let mut new_t = s[s_len - n..].to_vec();
            new_t.extend_from_slice(&t);
            self.try_redistribute(i, j, new_s, new_t, &kind_j)
        })?;
        Ok(())
    }

    fn try_redistribute(
        &mut self,
        i: usize,
        j: usize,
        new_s: Vec<u32>,
        new_t: Vec<u32>,
        kind_j: &StringChoice,
    ) -> ShrinkResult<bool> {
        if !kind_j.validate(&new_t) {
            return Ok(false);
        }
        self.replace(&HashMap::from([
            (i, ChoiceValue::String(new_s)),
            (j, ChoiceValue::String(new_t)),
        ]))
    }

    /// For each pair of string nodes within distance 4, lower every
    /// occurrence of a shared codepoint in *both* strings simultaneously.
    ///
    /// Handles the case where two strings must contain the same
    /// character but the actual character value is free — we want to
    /// drive both occurrences toward the alphabet's smallest member at
    /// once.
    pub(crate) fn lower_duplicated_characters(&mut self) -> ShrinkResult<()> {
        let len = self.current_nodes.len();
        for i in 0..len {
            for j in (i + 1)..(i + 1 + 4).min(len) {
                // Both must be String kinds.
                let (kind_i, val_i) = match (
                    self.current_nodes[i].kind.as_ref(),
                    &self.current_nodes[i].value,
                ) {
                    (ChoiceKind::String(k), ChoiceValue::String(v)) => (k.clone(), v.clone()),
                    _ => continue,
                };
                let (kind_j, val_j) = match (
                    self.current_nodes[j].kind.as_ref(),
                    &self.current_nodes[j].value,
                ) {
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
                    bin_search_down_r(0, original_key as i128, &mut |new_key| {
                        // `key_to_codepoint(new_key)` is `Some` for
                        // every key in `0..alpha_size`, and our search
                        // upper bound is `original_key` which is itself
                        // a valid alphabet position.  Likewise the
                        // resulting `new_cp` differs from `ch` (whose
                        // key was `original_key > new_key`).
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
                        // The two nodes can have different alphabets: the
                        // lowered codepoint comes from node i's alphabet and
                        // may not exist in node j's. Hypothesis's equivalent
                        // attempt is silently rejected by choice_permitted;
                        // do the same.
                        if !kind_i.validate(&new_i) || !kind_j.validate(&new_j) {
                            return Ok(false);
                        }
                        self.replace(&HashMap::from([
                            (i, ChoiceValue::String(new_i)),
                            (j, ChoiceValue::String(new_j)),
                        ]))
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Walk every string node and try replacing each codepoint with one
    /// of its "natural simpler" variants — NFD base + case mappings.
    ///
    /// Complements `shrink_strings`' per-position search by trying the
    /// semantically obvious replacements that lex-index bisection can
    /// skip over.
    pub(crate) fn normalize_unicode_chars(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let (kind, value) = match (
                self.current_nodes[i].kind.as_ref(),
                &self.current_nodes[i].value,
            ) {
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
                    if self.replace(&HashMap::from([(i, ChoiceValue::String(new_value))]))? {
                        break;
                    }
                }
            }
            i += 1;
        }
        Ok(())
    }
}

/// Cross-string codepoint candidates from natural text transformations.
///
/// For codepoint `cp` under alphabet `intervals`, returns the candidates
/// produced by (matching Hypothesis's `_natural_simpler_chars`):
///
/// * NFD and NFKD decomposition — every character of the fully-decomposed
///   form, so `'À'` offers `'A'` and `'①'` offers `'1'`.
/// * `to_lowercase`, `to_uppercase`, and full case folding — every
///   character of the mapped form, so `'ß'` offers `'s'` via casefold
///   even though the folded form `"ss"` is two characters.
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
    candidates.extend(unicodedata::casefold_chars(cp));
    candidates.extend(unicodedata::decomposition_chars(cp));
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
/// pure binary search nor a `find_integer` descent would reach the
/// smaller-key target.
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

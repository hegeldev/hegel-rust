use crate::native::HashMap;
use alloc::vec::Vec;

use crate::native::core::{ChoiceValue, StringChoice};
use crate::unicodedata;

use super::search::{BinSearchDown, FindInteger};
use super::{PassExit, ShrinkResult, Shrinker, absorb_node_gone};
use crate::control::{
    hegel_internal_debug_assert, hegel_internal_debug_assert_ne, hegel_internal_unwrap,
};

impl<'a> Shrinker<'a> {
    pub(super) async fn shrink_strings(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            absorb_node_gone(self.shrink_string_node(i).await)?;
            i += 1;
        }
        Ok(())
    }

    async fn shrink_string_node(&mut self, i: usize) -> Result<(), PassExit> {
        {
            let (kind, current) = self.string_at(i).ok_or(PassExit::NodeGone)?;

            let simplest = kind.simplest()?;
            if simplest != current {
                self.replace(&HashMap::from_iter([(i, ChoiceValue::String(simplest))]))
                    .await?;
            }

            let captured = self.current_string(i).ok_or(PassExit::NodeGone)?;
            let cur_len = captured.len();
            if cur_len > kind.min_size {
                let mut search = BinSearchDown::new(kind.min_size as i128, cur_len as i128);
                while let Some(sz) = search.probe() {
                    let cand: Vec<u32> = captured[..sz as usize].to_vec();
                    let ok = self
                        .replace(&HashMap::from_iter([(i, ChoiceValue::String(cand))]))
                        .await?;
                    search.record(ok);
                }
            }

            let cur = self.current_string(i).ok_or(PassExit::NodeGone)?;
            let scan_end = (kind.min_size + 8).min(cur.len());
            for target_len in kind.min_size..scan_end {
                let cur = self.current_string(i).ok_or(PassExit::NodeGone)?;
                if target_len > cur.len() {
                    break;
                }
                let cand: Vec<u32> = cur[..target_len].to_vec();
                self.replace(&HashMap::from_iter([(i, ChoiceValue::String(cand))]))
                    .await?;
            }

            let cur = self.current_string(i).ok_or(PassExit::NodeGone)?;
            let mut j = cur.len();
            while j > 0 {
                j -= 1;
                let cur = self.current_string(i).ok_or(PassExit::NodeGone)?;
                if cur.len() <= kind.min_size {
                    continue;
                }
                let mut cand = cur.clone();
                cand.remove(j);
                self.replace(&HashMap::from_iter([(i, ChoiceValue::String(cand))]))
                    .await?;
            }

            let cur = self.current_string(i).ok_or(PassExit::NodeGone)?;
            let dup_codepoints: Vec<u32> = {
                let mut counts: HashMap<u32, usize> = HashMap::default();
                for &cp in &cur {
                    *counts.entry(cp).or_default() += 1;
                }
                let mut dups: Vec<u32> = counts
                    .into_iter()
                    .filter(|(_, n)| *n > 1)
                    .map(|(cp, _)| cp)
                    .collect();
                dups.sort_by_key(|&cp| kind.codepoint_key(cp));
                dups
            };
            async fn try_replace_all(
                sh: &mut Shrinker<'_>,
                i: usize,
                val: u32,
                cand_cp: u32,
            ) -> Result<bool, PassExit> {
                let mut new_str = sh.current_string(i).ok_or(PassExit::NodeGone)?;
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
                Ok(sh
                    .replace(&HashMap::from_iter([(i, ChoiceValue::String(new_str))]))
                    .await?)
            }

            for val in dup_codepoints {
                if kind.codepoint_key(val) == 0 {
                    continue;
                }

                for cand_cp in semantic_candidates(val, &kind) {
                    try_replace_all(self, i, val, cand_cp).await?;
                    let cur = self.current_string(i).ok_or(PassExit::NodeGone)?;
                    if !cur.contains(&val) {
                        break;
                    }
                }

                let cur = self.current_string(i).ok_or(PassExit::NodeGone)?;
                if cur.contains(&val) {
                    let cur_key = kind.codepoint_key(val);
                    if cur_key > 0 {
                        let mut search = BinSearchDown::new(0, cur_key as i128);
                        while let Some(k) = search.probe() {
                            let cp = hegel_internal_unwrap!(
                                kind.key_to_codepoint(k as u32),
                                "shrink pass probed a key outside the alphabet"
                            );
                            let ok = try_replace_all(self, i, val, cp).await?;
                            search.record(ok);
                        }
                    }
                }
            }

            let cur = self.current_string(i).ok_or(PassExit::NodeGone)?;
            let mut j = cur.len();
            while j > 0 {
                j -= 1;
                let cur = self.current_string(i).ok_or(PassExit::NodeGone)?;
                if kind.codepoint_key(cur[j]) == 0 {
                    continue;
                }
                let original_cp = cur[j];

                for cand_cp in semantic_candidates(original_cp, &kind) {
                    let mut cand = self.current_string(i).ok_or(PassExit::NodeGone)?;
                    let cur_key = kind.codepoint_key(cand[j]);
                    if kind.codepoint_key(cand_cp) >= cur_key {
                        continue;
                    }
                    cand[j] = cand_cp;
                    self.replace(&HashMap::from_iter([(i, ChoiceValue::String(cand))]))
                        .await?;
                }

                let cur = self.current_string(i).ok_or(PassExit::NodeGone)?;
                let cur_key = kind.codepoint_key(cur[j]);
                if cur_key > 0 {
                    let mut search = BinSearchDown::new(0, cur_key as i128);
                    while let Some(k) = search.probe() {
                        let cp = hegel_internal_unwrap!(
                            kind.key_to_codepoint(k as u32),
                            "shrink pass probed a key outside the alphabet"
                        );
                        let mut cand = self.current_string(i).ok_or(PassExit::NodeGone)?;
                        cand[j] = cp;
                        let ok = self
                            .replace(&HashMap::from_iter([(i, ChoiceValue::String(cand))]))
                            .await?;
                        search.record(ok);
                    }
                }
            }

            let mut pos = 1;
            loop {
                let cur = self.current_string(i).ok_or(PassExit::NodeGone)?;
                if pos >= cur.len() {
                    break;
                }
                let mut j = pos;
                while j > 0 {
                    let cur = self.current_string(i).ok_or(PassExit::NodeGone)?;
                    let prev_key = kind.codepoint_key(cur[j - 1]);
                    let cur_key = kind.codepoint_key(cur[j]);
                    if prev_key <= cur_key {
                        break;
                    }
                    let mut swapped = cur.clone();
                    swapped.swap(j - 1, j);
                    if self
                        .replace(&HashMap::from_iter([(i, ChoiceValue::String(swapped))]))
                        .await?
                    {
                        j -= 1;
                    } else {
                        break;
                    }
                }
                pos += 1;
            }
        }
        Ok(())
    }

    /// The string constraint and value at node `i`, or `None` when the node
    /// is not (or no longer) a string — a concurrent shrink can pun the kind
    /// at any position between probes.
    fn string_at(&self, i: usize) -> Option<(StringChoice, Vec<u32>)> {
        let (sc, v) = self.current_nodes.get(i)?.data.as_string()?;
        Some((sc.clone(), v.to_vec()))
    }

    /// The current string value at node `i`; `None` under the same
    /// conditions as [`Shrinker::string_at`].
    fn current_string(&self, i: usize) -> Option<Vec<u32>> {
        self.string_at(i).map(|(_, v)| v)
    }

    /// Try redistributing length between pairs of string values. For
    /// adjacent and skip-one-adjacent pairs of `StringChoice` nodes, move
    /// codepoints from the earlier node's value to the later one's —
    /// useful for tests with a total-length constraint across two strings,
    /// where the minimal counterexample has the first string as short as
    /// possible.
    pub(super) async fn redistribute_string_pairs(&mut self) -> ShrinkResult<()> {
        for gap in 1..3usize {
            let mut idx = 0;
            loop {
                let indices = self.string_indices();
                if idx + gap >= indices.len() {
                    break;
                }
                let i = indices[idx];
                let j = indices[idx + gap];
                absorb_node_gone(self.redistribute_string_pair(i, j).await)?;
                idx += 1;
            }
        }
        Ok(())
    }

    fn string_indices(&self) -> Vec<usize> {
        self.current_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.data.as_string().map(|_| i))
            .collect()
    }

    async fn redistribute_string_pair(&mut self, i: usize, j: usize) -> Result<(), PassExit> {
        let s = self.current_string(i).ok_or(PassExit::NodeGone)?;
        let (kind_j, t) = self.string_at(j).ok_or(PassExit::NodeGone)?;

        if s.is_empty() {
            return Ok(());
        }

        let combined: Vec<u32> = s.iter().copied().chain(t.iter().copied()).collect();
        if self
            .try_redistribute(i, j, Vec::new(), combined, &kind_j)
            .await?
        {
            return Ok(());
        }

        let (last, s_init) = s.split_last().unwrap();
        let mut t_prepended = Vec::with_capacity(t.len() + 1);
        t_prepended.push(*last);
        t_prepended.extend_from_slice(&t);
        if !self
            .try_redistribute(i, j, s_init.to_vec(), t_prepended, &kind_j)
            .await?
        {
            return Ok(());
        }

        let s_len = s.len();
        let mut search = FindInteger::new();
        while let Some(extra) = search.probe() {
            let n = 1 + extra;
            let ok = if n > s_len {
                false
            } else {
                let new_s = s[..s_len - n].to_vec();
                let mut new_t = s[s_len - n..].to_vec();
                new_t.extend_from_slice(&t);
                self.try_redistribute(i, j, new_s, new_t, &kind_j).await?
            };
            search.record(ok);
        }
        Ok(())
    }

    async fn try_redistribute(
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
        self.replace(&HashMap::from_iter([
            (i, ChoiceValue::String(new_s)),
            (j, ChoiceValue::String(new_t)),
        ]))
        .await
    }

    /// For each pair of string nodes within distance 4, lower every
    /// occurrence of a shared codepoint in *both* strings simultaneously.
    ///
    /// Handles the case where two strings must contain the same
    /// character but the actual character value is free — we want to
    /// drive both occurrences toward the alphabet's smallest member at
    /// once.
    pub(crate) async fn lower_duplicated_characters(&mut self) -> ShrinkResult<()> {
        let len = self.current_nodes.len();
        for i in 0..len {
            for j in (i + 1)..(i + 1 + 4).min(len) {
                let Some((kind_i, val_i)) = self.string_at(i) else {
                    continue;
                };
                let Some((kind_j, val_j)) = self.string_at(j) else {
                    continue;
                };
                let set_i: alloc::collections::BTreeSet<u32> = val_i.iter().copied().collect();
                let set_j: alloc::collections::BTreeSet<u32> = val_j.iter().copied().collect();
                let shared: Vec<u32> = set_i.intersection(&set_j).copied().collect();
                for ch in shared {
                    let original_key = kind_i.codepoint_key(ch);
                    if original_key == 0 {
                        continue;
                    }
                    let mut search = BinSearchDown::new(0, original_key as i128);
                    while let Some(new_key) = search.probe() {
                        let new_cp = hegel_internal_unwrap!(
                            kind_i.key_to_codepoint(new_key as u32),
                            "shrink pass probed a key outside the alphabet"
                        );
                        hegel_internal_debug_assert_ne!(new_cp, ch);
                        let new_i: Vec<u32> = val_i
                            .iter()
                            .map(|&c| if c == ch { new_cp } else { c })
                            .collect();
                        let new_j: Vec<u32> = val_j
                            .iter()
                            .map(|&c| if c == ch { new_cp } else { c })
                            .collect();
                        let ok = if !kind_i.validate(&new_i) || !kind_j.validate(&new_j) {
                            false
                        } else {
                            self.replace(&HashMap::from_iter([
                                (i, ChoiceValue::String(new_i)),
                                (j, ChoiceValue::String(new_j)),
                            ]))
                            .await?
                        };
                        search.record(ok);
                    }
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
    pub(crate) async fn normalize_unicode_chars(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            absorb_node_gone(self.normalize_unicode_chars_at(i).await)?;
            i += 1;
        }
        Ok(())
    }

    async fn normalize_unicode_chars_at(&mut self, i: usize) -> Result<(), PassExit> {
        let (kind, value) = self.string_at(i).ok_or(PassExit::NodeGone)?;
        for pos in 0..value.len() {
            let cp = value[pos];
            let candidates = natural_simpler_chars(cp, &kind);
            let cur = self.current_string(i).ok_or(PassExit::NodeGone)?;
            if pos >= cur.len() || cur[pos] != cp {
                continue;
            }
            for replacement in candidates {
                let mut new_value = cur.clone();
                new_value[pos] = replacement;
                hegel_internal_debug_assert!(kind.validate(&new_value));
                if self
                    .replace(&HashMap::from_iter([(i, ChoiceValue::String(new_value))]))
                    .await?
                {
                    break;
                }
            }
        }
        Ok(())
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
    use alloc::collections::BTreeSet;
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
/// pure binary search nor a [`FindInteger`] descent would reach the
/// smaller-key target.
fn semantic_candidates(cp: u32, kind: &StringChoice) -> Vec<u32> {
    let mut out = Vec::with_capacity(64);
    let cur_key = kind.codepoint_key(cp);

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

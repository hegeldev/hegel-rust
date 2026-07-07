use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceValue, StringChoice};
use crate::unicodedata;

use super::{ShrinkResult, Shrinker, bin_search_down_r, find_integer_r};
use crate::control::{hegel_internal_debug_assert, hegel_internal_debug_assert_ne};

impl<'a> Shrinker<'a> {
    pub(super) fn shrink_strings(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let (kind, current) = match (
                self.current_nodes[i].kind.as_ref(),
                self.current_nodes[i].value.clone(),
            ) {
                (ChoiceKind::String(sc), ChoiceValue::String(s)) => (sc.clone(), s),
                _ => {
                    i += 1;
                    continue;
                }
            };

            let simplest = kind.simplest();
            if simplest != current {
                self.replace(&HashMap::from([(i, ChoiceValue::String(simplest))]))?;
            }

            let cur_len = self.current_string(i).len();
            if cur_len > kind.min_size {
                let captured = self.current_string(i);
                bin_search_down_r(kind.min_size as i128, cur_len as i128, &mut |sz| {
                    let cand: Vec<u32> = captured[..sz as usize].to_vec();
                    self.replace(&HashMap::from([(i, ChoiceValue::String(cand))]))
                })?;
            }

            let cur_len = self.current_string(i).len();
            let scan_end = (kind.min_size + 8).min(cur_len);
            for target_len in kind.min_size..scan_end {
                let cur = self.current_string(i);
                if target_len > cur.len() {
                    break;
                }
                let cand: Vec<u32> = cur[..target_len].to_vec();
                self.replace(&HashMap::from([(i, ChoiceValue::String(cand))]))?;
            }

            let mut j = self.current_string(i).len();
            while j > 0 {
                j -= 1;
                let cur = self.current_string(i);
                if cur.len() <= kind.min_size {
                    continue;
                }
                let mut cand = cur.clone();
                cand.remove(j);
                self.replace(&HashMap::from([(i, ChoiceValue::String(cand))]))?;
            }

            let dup_codepoints: Vec<u32> = {
                let cur = self.current_string(i);
                let mut counts: HashMap<u32, usize> = HashMap::new();
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
                    try_replace_all(self, cand_cp)?;
                    if !self.current_string(i).contains(&val) {
                        break;
                    }
                }

                if self.current_string(i).contains(&val) {
                    let cur_key = kind.codepoint_key(val);
                    if cur_key > 0 {
                        bin_search_down_r(0, cur_key as i128, &mut |k| {
                            let cp = kind
                                .key_to_codepoint(k as u32)
                                .expect("bin_search probe stays within alpha_size");
                            try_replace_all(self, cp)
                        })?;
                    }
                }
            }

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

                let cur_key = kind.codepoint_key(self.current_string(i)[j]);
                if cur_key > 0 {
                    bin_search_down_r(0, cur_key as i128, &mut |k| {
                        let cp = kind
                            .key_to_codepoint(k as u32)
                            .expect("bin_search probe stays within alpha_size");
                        let mut cand = self.current_string(i);
                        cand[j] = cp;
                        self.replace(&HashMap::from([(i, ChoiceValue::String(cand))]))
                    })?;
                }
            }

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
                    if self.replace(&HashMap::from([(i, ChoiceValue::String(swapped))]))? {
                        j -= 1;
                    } else {
                        break;
                    }
                }
                pos += 1;
            }

            i += 1;
        }
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

        let combined: Vec<u32> = s.iter().copied().chain(t.iter().copied()).collect();
        if self.try_redistribute(i, j, Vec::new(), combined, &kind_j)? {
            return Ok(());
        }

        let (last, s_init) = s.split_last().unwrap();
        let mut t_prepended = Vec::with_capacity(t.len() + 1);
        t_prepended.push(*last);
        t_prepended.extend_from_slice(&t);
        if !self.try_redistribute(i, j, s_init.to_vec(), t_prepended, &kind_j)? {
            return Ok(());
        }

        let s_len = s.len();
        find_integer_r(|extra| {
            let n = 1 + extra;
            if n > s_len {
                return Ok(false);
            }
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
                    let original_key = kind_i.codepoint_key(ch);
                    if original_key == 0 {
                        continue;
                    }
                    bin_search_down_r(0, original_key as i128, &mut |new_key| {
                        let new_cp = kind_i
                            .key_to_codepoint(new_key as u32)
                            .expect("key < original_key < alpha_size");
                        hegel_internal_debug_assert_ne!(new_cp, ch);
                        let new_i: Vec<u32> = val_i
                            .iter()
                            .map(|&c| if c == ch { new_cp } else { c })
                            .collect();
                        let new_j: Vec<u32> = val_j
                            .iter()
                            .map(|&c| if c == ch { new_cp } else { c })
                            .collect();
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
                    hegel_internal_debug_assert!(kind.validate(&new_value));
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
/// pure binary search nor a `find_integer` descent would reach the
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

use crate::native::HashMap;

use crate::native::core::{BytesChoice, ChoiceValue};

use super::search::{BinSearchDown, FindInteger};
use super::{PassExit, ShrinkResult, Shrinker, absorb_node_gone};

impl<'a> Shrinker<'a> {
    pub(super) async fn shrink_bytes(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            absorb_node_gone(self.shrink_bytes_node(i).await)?;
            i += 1;
        }
        Ok(())
    }

    async fn shrink_bytes_node(&mut self, i: usize) -> Result<(), PassExit> {
        {
            let (bc, current) = self.bytes_at(i).ok_or(PassExit::NodeGone)?;
            let min_size = bc.min_size;

            let simplest = vec![0u8; min_size];
            if simplest != current {
                self.replace(&HashMap::from_iter([(i, ChoiceValue::Bytes(simplest))]))
                    .await?;
            }

            let captured = self.current_byte_value(i).ok_or(PassExit::NodeGone)?;
            let cur_len = captured.len();
            if cur_len > min_size {
                let mut search = BinSearchDown::new(min_size as i128, cur_len as i128);
                while let Some(sz) = search.probe() {
                    let sz = sz as usize;
                    let cand = captured[..sz].to_vec();
                    let ok = self
                        .replace(&HashMap::from_iter([(i, ChoiceValue::Bytes(cand))]))
                        .await?;
                    search.record(ok);
                }
            }

            let cur = self.current_byte_value(i).ok_or(PassExit::NodeGone)?;
            let scan_end = (min_size + 8).min(cur.len());
            for sz in min_size..scan_end {
                let cur = self.current_byte_value(i).ok_or(PassExit::NodeGone)?;
                if sz > cur.len() {
                    break;
                }
                let cand = cur[..sz].to_vec();
                self.replace(&HashMap::from_iter([(i, ChoiceValue::Bytes(cand))]))
                    .await?;
            }

            let cur = self.current_byte_value(i).ok_or(PassExit::NodeGone)?;
            let mut j = cur.len();
            while j > 0 {
                j -= 1;
                let cur = self.current_byte_value(i).ok_or(PassExit::NodeGone)?;
                if cur.len() <= min_size {
                    continue;
                }
                let mut cand = cur.clone();
                cand.remove(j);
                self.replace(&HashMap::from_iter([(i, ChoiceValue::Bytes(cand))]))
                    .await?;
            }

            let cur = self.current_byte_value(i).ok_or(PassExit::NodeGone)?;
            let mut j = cur.len();
            while j > 0 {
                j -= 1;
                let cur = self.current_byte_value(i).ok_or(PassExit::NodeGone)?;
                if cur[j] == 0 {
                    continue;
                }
                let hi = cur[j] as i128;
                let mut search = BinSearchDown::new(0, hi);
                while let Some(e) = search.probe() {
                    let mut cand = self.current_byte_value(i).ok_or(PassExit::NodeGone)?;
                    cand[j] = e as u8;
                    let ok = self
                        .replace(&HashMap::from_iter([(i, ChoiceValue::Bytes(cand))]))
                        .await?;
                    search.record(ok);
                }
            }

            let mut pos = 1;
            loop {
                let cur = self.current_byte_value(i).ok_or(PassExit::NodeGone)?;
                if pos >= cur.len() {
                    break;
                }
                let mut j = pos;
                while j > 0 {
                    let cur = self.current_byte_value(i).ok_or(PassExit::NodeGone)?;
                    if cur[j - 1] <= cur[j] {
                        break;
                    }
                    let mut swapped = cur.clone();
                    swapped.swap(j - 1, j);
                    if self
                        .replace(&HashMap::from_iter([(i, ChoiceValue::Bytes(swapped))]))
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

    /// The bytes constraint and value at node `i`, or `None` when the node
    /// is not (or no longer) a bytes node — a concurrent shrink can pun the
    /// kind at any position between probes.
    fn bytes_at(&self, i: usize) -> Option<(BytesChoice, Vec<u8>)> {
        let (bc, v) = self.current_nodes.get(i)?.data.as_bytes()?;
        Some((bc.clone(), v.to_vec()))
    }

    /// The current bytes value at node `i`; `None` under the same
    /// conditions as [`Shrinker::bytes_at`].
    fn current_byte_value(&self, i: usize) -> Option<Vec<u8>> {
        self.bytes_at(i).map(|(_, v)| v)
    }

    /// Try redistributing length between pairs of bytes values.
    ///
    /// For adjacent and skip-one-adjacent pairs of `BytesChoice` nodes,
    /// try moving bytes from the earlier node's value to the later one's.
    /// Useful for tests with a total-length constraint across two bytes
    /// values, where the minimal counterexample has the first as short
    /// as possible.
    pub(super) async fn redistribute_bytes_pairs(&mut self) -> ShrinkResult<()> {
        for gap in 1..3usize {
            let mut idx = 0;
            loop {
                let indices = self.bytes_indices();
                if idx + gap >= indices.len() {
                    break;
                }
                let i = indices[idx];
                let j = indices[idx + gap];
                absorb_node_gone(self.redistribute_bytes_pair(i, j).await)?;
                idx += 1;
            }
        }
        Ok(())
    }

    fn bytes_indices(&self) -> Vec<usize> {
        self.current_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.data.as_bytes().map(|_| i))
            .collect()
    }

    async fn redistribute_bytes_pair(&mut self, i: usize, j: usize) -> Result<(), PassExit> {
        let s = self.current_byte_value(i).ok_or(PassExit::NodeGone)?;
        let (kind_j, t) = self.bytes_at(j).ok_or(PassExit::NodeGone)?;

        if s.is_empty() {
            return Ok(());
        }

        let combined: Vec<u8> = s.iter().copied().chain(t.iter().copied()).collect();
        if self
            .try_redistribute_bytes(i, j, Vec::new(), combined, &kind_j)
            .await?
        {
            return Ok(());
        }

        let (last, s_init) = s.split_last().unwrap();
        let mut t_prepended = Vec::with_capacity(t.len() + 1);
        t_prepended.push(*last);
        t_prepended.extend_from_slice(&t);
        if !self
            .try_redistribute_bytes(i, j, s_init.to_vec(), t_prepended, &kind_j)
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
                self.try_redistribute_bytes(i, j, new_s, new_t, &kind_j)
                    .await?
            };
            search.record(ok);
        }
        Ok(())
    }

    async fn try_redistribute_bytes(
        &mut self,
        i: usize,
        j: usize,
        new_s: Vec<u8>,
        new_t: Vec<u8>,
        kind_j: &BytesChoice,
    ) -> ShrinkResult<bool> {
        if !kind_j.validate(&new_t) {
            return Ok(false);
        }
        self.replace(&HashMap::from_iter([
            (i, ChoiceValue::Bytes(new_s)),
            (j, ChoiceValue::Bytes(new_t)),
        ]))
        .await
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_bytes_tests.rs"]
mod tests;

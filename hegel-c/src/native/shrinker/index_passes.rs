//! Index-based shrink passes: `lower_and_bump` and `try_shortening_via_increment`.
//!
//! Both passes use the `to_index`/`from_index` API on `ChoiceData` for
//! type-generic shrinking.

use crate::control::hegel_internal_unwrap;
use crate::native::{HashMap, HashSet};
use alloc::vec::Vec;

use crate::native::bignum::{BigUint, Zero};
use crate::native::core::{ChoiceData, ChoiceNode, ChoiceValue, flattened_len};

use super::search::FindInteger;
use super::{ShrinkResult, ShrinkRun, Shrinker};

/// How many choices after a raised node
/// [`Shrinker::try_shortening_via_increment`] tries deleting to complete a
/// path switch.
const INCREMENT_DELETION_WINDOW: usize = 8;

/// Extra choices a raised sequence's probe may draw past the current
/// target's length, so a raise that leads to a longer path is seen as a
/// path change rather than an overrun.
const PROBE_EXTENSION: usize = 16;

/// Nodes the index passes skip even though they carry a dense index:
/// sequence kinds get their own dedicated passes. Clone nodes are skipped
/// too, structurally — they have no dense index, so `to_index` returns
/// `None` for them.
fn is_sequence(data: &ChoiceData) -> bool {
    matches!(data, ChoiceData::Bytes(..) | ChoiceData::String(..))
}

impl<'a> Shrinker<'a> {
    /// For each indexed node not at simplest, try decrementing it (lowering
    /// the index) and bumping a later node (raising its index).
    ///
    /// Value punning (via `for_choices` with `prefix_nodes`) handles the
    /// case where decrementing changes the kind at position `j` (e.g. a
    /// `one_of` branch switch).
    ///
    /// The bumps tried on `j` are small relative offsets, then small
    /// absolute indices, then the largest index: for a pair coupled through
    /// a product or a threshold (`a × k ≥ 1000`), lowering `a` needs `k`
    /// raised much further than a few units, and the largest value is the
    /// one most likely to keep the predicate — once it does, the
    /// value-lowering passes bring `k` down to the smallest value that
    /// still does.
    pub(super) async fn lower_and_bump(&mut self) -> ShrinkResult<()> {
        let max_gap = core::cmp::min(self.current_nodes.len(), 4);
        for gap in 1..max_gap {
            let mut idx = 0;
            while idx < self.current_nodes.len() {
                let i = idx;
                let node_i = self.current_nodes[i].clone();
                if is_sequence(&node_i.data) {
                    idx += 1;
                    continue;
                }
                let Some(current_idx) = node_i.data.to_index()? else {
                    idx += 1;
                    continue;
                };
                if current_idx.is_zero() {
                    idx += 1;
                    continue;
                }

                let mut decrement_targets: Vec<ChoiceValue> = Vec::new();
                if current_idx > BigUint::from(1u32) {
                    let v0 = hegel_internal_unwrap!(
                        node_i.data.from_index(BigUint::zero())?,
                        "lower_and_bump: from_index(0) has no value for an indexed kind"
                    );
                    decrement_targets.push(v0);
                }
                if let Some(v_prev) = node_i.data.from_index(&current_idx - BigUint::from(1u32))? {
                    if !decrement_targets.contains(&v_prev) {
                        decrement_targets.push(v_prev);
                    }
                }

                let j_opt = i.checked_add(gap).filter(|&j| j < self.current_nodes.len());
                let Some(j) = j_opt else {
                    idx += 1;
                    continue;
                };

                for new_val in &decrement_targets {
                    if gap == 1 {
                        let mut attempt = self.current_nodes.clone();
                        if let Some(lowered) = attempt[i].with_value(new_val) {
                            attempt[i] = lowered;
                            if self.consider(&attempt).await? {
                                self.descend_index(i).await?;
                            }

                            let mut zeroed = attempt;
                            for node in &mut zeroed[i + 1..] {
                                *node = node.with_simplest()?;
                            }
                            self.consider(&zeroed).await?;
                        }
                    }

                    if j < self.current_nodes.len() && !is_sequence(&self.current_nodes[j].data) {
                        let data_j = self.current_nodes[j].data.clone();
                        let Some((target_idx, max_j)) = data_j.to_index()?.zip(data_j.max_index())
                        else {
                            continue;
                        };
                        let mut bumped_any_relative = false;
                        for bump in [1u32, 2, 4] {
                            let candidate_idx = &target_idx + BigUint::from(bump);
                            if let Some(bumped) = data_j.from_index(candidate_idx)? {
                                if try_bump_ij(self, i, new_val, j, &bumped).await? {
                                    bumped_any_relative = true;
                                    break;
                                }
                            }
                        }
                        if !bumped_any_relative {
                            let mut p = BigUint::from(1u32);
                            for _ in 0..8 {
                                if p > max_j {
                                    break;
                                }
                                let p_minus_one = &p - BigUint::from(1u32);
                                if let Some(v) = data_j.from_index(p_minus_one)? {
                                    try_bump_ij(self, i, new_val, j, &v).await?;
                                }
                                if let Some(v) = data_j.from_index(p.clone())? {
                                    try_bump_ij(self, i, new_val, j, &v).await?;
                                }
                                p *= BigUint::from(2u32);
                            }
                            if let Some(v) = data_j.from_index(max_j)? {
                                try_bump_ij(self, i, new_val, j, &v).await?;
                            }
                        }
                    }
                }
                idx += 1;
            }
        }
        Ok(())
    }

    /// Keep lowering node `i`'s index by geometrically growing amounts
    /// after a step of one was accepted on its own. A predicate that
    /// admits one step usually admits many — the value passes that ran
    /// earlier in the iteration saw a target another pass has since
    /// changed — and taking them one per pass step would spend the
    /// improvement cap on unit moves before those passes run again. A
    /// node the accepted run left without an index descends nowhere.
    async fn descend_index(&mut self, i: usize) -> ShrinkResult<()> {
        let base = self.current_nodes[i]
            .data
            .to_index()?
            .unwrap_or_else(BigUint::zero);
        let mut search = FindInteger::new();
        while let Some(n) = search.probe() {
            let step = BigUint::from(n as u64);
            let lowered = if step > base || i >= self.current_nodes.len() {
                None
            } else {
                self.current_nodes[i].data.from_index(&base - &step)?
            };
            let ok = match lowered.and_then(|value| self.current_nodes[i].with_value(&value)) {
                Some(node) => {
                    let mut attempt = self.current_nodes.clone();
                    attempt[i] = node;
                    self.consider(&attempt).await?
                }
                None => false,
            };
            search.record(ok);
        }
        Ok(())
    }

    /// For each indexed node, try *raising* its index — by one, then to its
    /// largest — to see if the test then takes a shorter path.
    ///
    /// A value shrinker can only make values simpler; sometimes making a
    /// value *less* simple (`false → true`, or a `one_of` selector moved to
    /// a later alternative) leads to a shorter and thus overall simpler
    /// choice sequence. Such a candidate is lexicographically above the
    /// current target, so it goes through [`Shrinker::consider_reshaped`]
    /// rather than the prefiltered `consider`. The largest index is tried
    /// as well because a gate that hides draws while it is below a
    /// threshold (`if n(0, 99) < 50 { draw the pick }`) has to jump past
    /// the threshold rather than step: the largest value is the one surest
    /// to be past it, and the value-lowering passes then bring it back down
    /// to the threshold.
    ///
    /// The raised sequence is first replayed as a [`ShrinkRun::Probe`], so
    /// the run may draw past its end: that both accepts a shorter
    /// realisation outright and tells apart a raise that leaves the path
    /// unchanged (the realised nodes equal the candidate) from one that
    /// changes it — including the path that now needs *more* data, which a
    /// fixed-length replay would report as an overrun with the same nodes.
    ///
    /// When the raise changes the path without being an improvement, the
    /// shorter path usually also needs a stale node from the old path
    /// removed — the mirror image of the size-dependency fixup in
    /// [`Shrinker::minimize_individual_choices`]: switching a `one_of` to a
    /// shorter branch whose value must stay non-simplest, or flipping a
    /// boolean that replaces a collection with a single draw. So the raised
    /// sequence is retried with each span, then each single node, starting
    /// within [`INCREMENT_DELETION_WINDOW`] choices after the raised
    /// position deleted. The window keeps the pass from spending the
    /// scheduler's stall budget on far-away deletions; the stale node of a
    /// switched path sits right after the switch.
    pub(super) async fn try_shortening_via_increment(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            self.shorten_via_increment_at(i).await?;
            i += 1;
        }
        Ok(())
    }

    async fn shorten_via_increment_at(&mut self, i: usize) -> ShrinkResult<()> {
        let node = self.current_nodes[i].clone();
        if node.was_forced || is_sequence(&node.data) {
            return Ok(());
        }
        let Some(current_idx) = node.data.to_index()? else {
            return Ok(());
        };
        let plus_one = &current_idx + BigUint::from(1u32);
        let max = node.data.max_index().filter(|m| *m > plus_one);
        let epoch = self.improvements;
        for target in core::iter::once(plus_one).chain(max) {
            let Some(raised_value) = node.data.from_index(target)? else {
                return Ok(());
            };
            self.shorten_via_raise(i, &node, &raised_value).await?;
            if self.improvements > epoch {
                return Ok(());
            }
        }
        Ok(())
    }

    async fn shorten_via_raise(
        &mut self,
        i: usize,
        node: &ChoiceNode,
        raised_value: &ChoiceValue,
    ) -> ShrinkResult<()> {
        let raised_node = hegel_internal_unwrap!(
            node.with_value(raised_value),
            "try_shortening_via_increment: from_index produced a value of another kind"
        );
        let mut raised = self.current_nodes.clone();
        raised[i] = raised_node;

        let epoch = self.improvements;
        let prefix: Vec<ChoiceValue> = raised.iter().map(|n| n.value()).collect();
        let max_size = flattened_len(&self.current_nodes) + PROBE_EXTENSION;
        let Some(realised) = self
            .consider_reshaped(ShrinkRun::Probe {
                prefix: &prefix,
                max_size,
            })
            .await?
        else {
            return Ok(());
        };
        if self.improvements > epoch {
            return Ok(());
        }
        let path_unchanged = realised.len() == raised.len()
            && realised
                .iter()
                .zip(&raised)
                .all(|(r, c)| r.value() == c.value());
        if path_unchanged {
            return Ok(());
        }

        let window_end = raised.len().min(i + 1 + INCREMENT_DELETION_WINDOW);
        let spans: Vec<(usize, usize)> = self
            .current_spans
            .iter()
            .filter(|s| {
                s.start > i && s.start < window_end && s.end > s.start && s.end <= raised.len()
            })
            .map(|s| (s.start, s.end))
            .collect();
        let singles = (i + 1..window_end).map(|j| (j, j + 1));
        let mut attempted: HashSet<(usize, usize)> = HashSet::default();
        for (start, end) in spans.into_iter().chain(singles) {
            if !attempted.insert((start, end)) {
                continue;
            }
            let mut candidate = raised[..start].to_vec();
            candidate.extend_from_slice(&raised[end..]);
            let outcome = self.consider_reshaped(ShrinkRun::Full(&candidate)).await?;
            if outcome.is_none() || self.improvements > epoch {
                return Ok(());
            }
        }
        Ok(())
    }
}

/// Helper for `lower_and_bump`: replace `{i: new_val, j: bump_val}` if the
/// kind at j validates `bump_val`. Returns whether the attempt was
/// interesting.
pub(super) async fn try_bump_ij(
    shrinker: &mut Shrinker<'_>,
    i: usize,
    new_val: &ChoiceValue,
    j: usize,
    bump_val: &ChoiceValue,
) -> ShrinkResult<bool> {
    let replacements: HashMap<usize, ChoiceValue> = [(i, new_val.clone()), (j, bump_val.clone())]
        .into_iter()
        .collect();
    shrinker.replace(&replacements).await
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_lower_and_bump_tests.rs"]
mod lower_and_bump_tests;

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_try_shortening_via_increment_tests.rs"]
mod try_shortening_via_increment_tests;

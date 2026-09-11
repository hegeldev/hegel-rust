//! Replaying a counterexample as one test case (decision 74).
//!
//! A counterexample is an ordered pool of timelines. A replay keeps the
//! *live set*: the timelines that agree with every value drawn so far, in
//! every stream of the family. Each draw is served from the first live
//! timeline whose stored value fits the request; the timelines whose value
//! at that position differs — or that have no value there — leave the live
//! set. Switching between live timelines is not a divergence: the stored
//! branches are all the same counterexample, and which one the run is on is
//! decided only when the test's own choices reveal it. A divergence is the
//! moment no live timeline fits, and it is recorded once, at the stream and
//! position where it happened; from there the run is rescued by
//! [`Rescue`]. The live set is shared by every stream of the family, so a
//! disagreement inside a cloned stream prunes the timeline for its parent
//! too.

use alloc::sync::Arc;
use alloc::vec::Vec;

use super::choices::{ChoiceNode, ChoiceValue};
use crate::sys::sync::Mutex;

/// What a replay does once no stored timeline is live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rescue {
    /// The proposal mode of a shrink candidate's first run and of every
    /// probe: a single timeline stays the positional prefix for the whole
    /// run, a misfitting value puns to the draw's simplest or unit value,
    /// and past its end the run draws randomly. The candidate's misfits are
    /// the shrink's own edits, not the test's nondeterminism.
    Pun,
    /// The measurement mode: the pruned timelines, most recently pruned
    /// first, continue positionally where their values fit, and where none
    /// fits the run draws randomly under its continuation budget.
    Continue,
}

/// Where a replay left its counterexample: the stream (its clone id; empty
/// for the root) and the position in that stream of the first draw no live
/// timeline could serve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Divergence {
    pub stream: Vec<usize>,
    pub position: usize,
}

/// The family-wide replay state: which timelines are live, the order in
/// which the others were pruned (as batches, one per pruning draw), and the
/// first divergence.
#[derive(Debug)]
struct LiveSet {
    live: Vec<bool>,
    pruned: Vec<Vec<usize>>,
    divergence: Option<Divergence>,
}

impl LiveSet {
    fn new(count: usize) -> Self {
        LiveSet {
            live: alloc::vec![true; count],
            pruned: Vec::new(),
            divergence: None,
        }
    }

    fn prune(&mut self, batch: Vec<usize>) {
        if batch.is_empty() {
            return;
        }
        for &k in &batch {
            self.live[k] = false;
        }
        self.pruned.push(batch);
    }

    fn diverge(&mut self, stream: &[usize], position: usize) {
        if self.divergence.is_none() {
            self.divergence = Some(Divergence {
                stream: stream.to_vec(),
                position,
            });
        }
    }

    fn any_live(&self) -> bool {
        self.live.iter().any(|l| *l)
    }
}

/// What a replay resolved a draw to.
pub(crate) enum Resolved<'a, V> {
    /// A stored value fit the request.
    Served(V),
    /// [`Rescue::Pun`] only: the positional value misfits; the caller puns.
    Misfit(&'a ChoiceValue),
    /// Nothing stored applies at this position; the caller draws.
    Exhausted,
}

/// One stream's view of a counterexample replay: this stream's value
/// sequence under each timeline (`None` where the timeline had no cloned
/// stream at this position), the realized nodes of the proposal under
/// [`Rescue::Pun`] (for the simplest-value pun), and the family's shared
/// live set.
pub(crate) struct Replay {
    timelines: Vec<Option<Vec<ChoiceValue>>>,
    nodes: Option<Vec<ChoiceNode>>,
    rescue: Rescue,
    shared: Arc<Mutex<LiveSet>>,
}

impl Replay {
    /// A [`Rescue::Pun`] replay of one proposed sequence.
    pub(crate) fn pun(prefix: Vec<ChoiceValue>, nodes: Option<Vec<ChoiceNode>>) -> Self {
        Replay {
            timelines: alloc::vec![Some(prefix)],
            nodes,
            rescue: Rescue::Pun,
            shared: Arc::new(Mutex::new(LiveSet::new(1))),
        }
    }

    /// A [`Rescue::Continue`] replay of a whole counterexample, in its order.
    pub(crate) fn counterexample(timelines: Vec<Vec<ChoiceValue>>) -> Self {
        let count = timelines.len();
        Replay {
            timelines: timelines.into_iter().map(Some).collect(),
            nodes: None,
            rescue: Rescue::Continue,
            shared: Arc::new(Mutex::new(LiveSet::new(count))),
        }
    }

    /// The proposal's stored value at `position`, for [`Rescue::Pun`].
    fn proposal(&self) -> &[ChoiceValue] {
        self.timelines[0].as_deref().unwrap_or(&[])
    }

    /// The realized node of the proposal at `position`, when known.
    pub(crate) fn proposal_node(&self, position: usize) -> Option<&ChoiceNode> {
        self.nodes.as_ref().and_then(|n| n.get(position))
    }

    fn value_at(&self, timeline: usize, position: usize) -> Option<&ChoiceValue> {
        self.timelines[timeline]
            .as_ref()
            .and_then(|t| t.get(position))
    }

    /// Resolve the draw at `position` of stream `stream`, given the draw's
    /// acceptance test over stored values. A proposal's misfit under
    /// [`Rescue::Pun`] is the shrink's own edit, never a divergence.
    pub(crate) fn resolve<V>(
        &self,
        stream: &[usize],
        position: usize,
        fits: impl Fn(&ChoiceValue) -> Option<V>,
    ) -> Resolved<'_, V> {
        match self.rescue {
            Rescue::Pun => match self.proposal().get(position) {
                Some(stored) => match fits(stored) {
                    Some(v) => Resolved::Served(v),
                    None => Resolved::Misfit(stored),
                },
                None => Resolved::Exhausted,
            },
            Rescue::Continue => self.resolve_live(stream, position, fits),
        }
    }

    fn resolve_live<V>(
        &self,
        stream: &[usize],
        position: usize,
        fits: impl Fn(&ChoiceValue) -> Option<V>,
    ) -> Resolved<'_, V> {
        let mut set = self.shared.lock();
        if set.any_live() {
            let hit = (0..self.timelines.len())
                .filter(|&k| set.live[k])
                .find_map(|k| {
                    let stored = self.value_at(k, position)?;
                    fits(stored).map(|v| (stored, v))
                });
            match hit {
                Some((stored, v)) => {
                    let batch: Vec<usize> = (0..self.timelines.len())
                        .filter(|&j| set.live[j] && self.value_at(j, position) != Some(stored))
                        .collect();
                    set.prune(batch);
                    return Resolved::Served(v);
                }
                None => {
                    let batch: Vec<usize> =
                        (0..self.timelines.len()).filter(|&j| set.live[j]).collect();
                    set.prune(batch);
                    set.diverge(stream, position);
                }
            }
        }
        for &k in set.pruned.iter().rev().flatten() {
            if let Some(v) = self.value_at(k, position).and_then(&fits) {
                return Resolved::Served(v);
            }
        }
        Resolved::Exhausted
    }

    /// The replay of the stream cloned at `position` of stream `stream`:
    /// each timeline's cloned sequence there, with the timelines that have
    /// no clone at that position leaving the live set.
    pub(crate) fn clone_child(&self, stream: &[usize], position: usize) -> Self {
        let mut set = self.shared.lock();
        let mut timelines = Vec::with_capacity(self.timelines.len());
        let mut nodes = None;
        let mut batch = Vec::new();
        for k in 0..self.timelines.len() {
            match self.value_at(k, position) {
                Some(ChoiceValue::Clone(record)) => {
                    if k == 0 {
                        nodes = record.realized_nodes().map(<[ChoiceNode]>::to_vec);
                    }
                    timelines.push(Some(record.owned_values()));
                }
                _ => {
                    timelines.push(None);
                    if set.live[k] {
                        batch.push(k);
                    }
                }
            }
        }
        match self.rescue {
            Rescue::Pun => {
                if timelines[0].is_none() {
                    timelines[0] = Some(Vec::new());
                }
            }
            Rescue::Continue => {
                set.prune(batch);
                if !set.any_live() {
                    set.diverge(stream, position);
                }
            }
        }
        Replay {
            timelines,
            nodes,
            rescue: self.rescue,
            shared: Arc::clone(&self.shared),
        }
    }

    /// The family's first divergence, if any.
    pub(crate) fn divergence(&self) -> Option<Divergence> {
        self.shared.lock().divergence.clone()
    }

    /// Which timelines are live, in counterexample order.
    #[cfg(test)]
    pub(crate) fn live(&self) -> Vec<bool> {
        self.shared.lock().live.clone()
    }

    /// The longest timeline's top-level length: the floor of a replay's
    /// size budget.
    pub(crate) fn longest(&self) -> usize {
        self.timelines
            .iter()
            .map(|t| t.as_ref().map_or(0, Vec::len))
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/replay_tests.rs"]
mod tests;

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
//!
//! A counterexample stored as a graph (decision 78, [`Graph`]) is replayed
//! by [`Replay::graph`]: a walk that stands at the states the last draw may
//! have led to, arrives at each draw by the identity its address reports,
//! and serves the first fitting edge there. See [`GraphWalk`].

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;

use super::choices::{ChoiceNode, ChoiceValue};
use crate::native::graph::{END, Frame, Graph, Ident, START, ident_before};
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

/// A resolver outside the engine deciding every draw of a replay
/// (experiment 017: the counterexample as a graph, walked by the
/// harness). `resolve` returns the stored value to serve at `position` of
/// `stream`, or `None` to draw randomly; `fits` is the draw's acceptance
/// test over stored values; `frames` is the draw's structural address
/// (experiment 019): the spans open at the draw, outermost first, each as
/// its label and the number of earlier siblings with that label.
pub trait ExternalReplay: Send {
    fn resolve(
        &mut self,
        stream: &[usize],
        position: usize,
        frames: &[(u64, usize)],
        fits: &dyn Fn(&ChoiceValue) -> bool,
    ) -> Option<ChoiceValue>;
    fn divergence(&self) -> Option<Divergence>;
    fn longest(&self) -> usize;
}

type External = Arc<Mutex<Box<dyn ExternalReplay>>>;

/// The family-wide replay state: which timelines are live, the order in
/// which the others were pruned (as batches, one per pruning draw), and the
/// first divergence.
#[derive(Debug)]
struct LiveSet {
    live: Vec<bool>,
    pruned: Vec<Vec<usize>>,
    divergence: Option<Divergence>,
    realized: Vec<bool>,
    /// A live proposal ran out of values: the rest of the run is drawn at
    /// random, as under [`Rescue::Pun`], rather than continued from the
    /// timelines it was a prefix of.
    random_tail: bool,
}

impl LiveSet {
    fn new(count: usize) -> Self {
        LiveSet {
            live: alloc::vec![true; count],
            pruned: Vec::new(),
            divergence: None,
            realized: alloc::vec![false; count],
            random_tail: false,
        }
    }

    /// The first live timeline in counterexample order: the one that
    /// serves a fitting value, and under [`Replay::shrink_set`] the leading
    /// proposal while it is live.
    fn first_live(&self) -> Option<usize> {
        self.live.iter().position(|live| *live)
    }

    /// The leading proposal `first` leaves the set at `position` by its
    /// own doing — its end or its insisted misfit — taking every other live
    /// timeline with it; it alone is realized by the run, the others
    /// merely agreed with it so far.
    fn leave_leading(&mut self, first: usize, stream: &[usize], position: usize) {
        let batch: Vec<usize> = (0..self.live.len()).filter(|&k| self.live[k]).collect();
        self.prune(batch);
        if self.divergence.is_none() {
            self.divergence = Some(Divergence {
                stream: stream.to_vec(),
                position,
            });
        }
        self.realized[first] = true;
    }

    /// The leading proposal ran out at `position`: the run's tail is random.
    fn run_out(&mut self, first: usize, stream: &[usize], position: usize) {
        self.leave_leading(first, stream, position);
        self.random_tail = true;
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
            if let Some(batch) = self.pruned.last() {
                for &k in batch {
                    self.realized[k] = true;
                }
            }
        }
    }

    fn any_live(&self) -> bool {
        self.live.iter().any(|l| *l)
    }
}

/// Where a graph walk stands between draws: the states the last served
/// draw may have led to — its tie — or, after a clone, the clone edges of
/// the tie together with the child stream's live set over their records,
/// so that the states still possible are those of the records the child
/// has agreed with so far.
enum Pending {
    Nodes(Vec<usize>),
    Tie {
        node: usize,
        edges: Vec<usize>,
    },
    Clone {
        node: usize,
        edges: Vec<usize>,
        shared: Arc<Mutex<LiveSet>>,
    },
}

/// The root stream's walk of a [`Graph`] (decision 78). At each draw the
/// walk computes the identity of the state the run is in from the draw's
/// address and the previous draw's, and **arrives** at the pending state of
/// that identity — settling the edge that led there. No pending state of
/// that identity is a **misjoin**: a divergence, after which the walk is
/// rescued by the graph's node of that identity if there is one. The draw
/// is then served by the first edge at its address whose value fits, and
/// the edges at that address with that value are the new tie. No fitting
/// edge is a **misfit**: a divergence, and the run draws at random until
/// an identity rescues it. A clone is served by the clone edges at its
/// address, whose records the child stream replays as a live set; the
/// child's divergence is the walk's.
pub(crate) struct GraphWalk {
    graph: Arc<Graph>,
    prev: Option<Vec<Frame>>,
    pending: Pending,
    settled: Vec<(usize, usize)>,
    divergence: Option<Divergence>,
    children: Vec<Arc<Mutex<LiveSet>>>,
}

impl GraphWalk {
    fn new(graph: Arc<Graph>) -> Self {
        GraphWalk {
            graph,
            prev: None,
            pending: Pending::Nodes(alloc::vec![START]),
            settled: Vec::new(),
            divergence: None,
            children: Vec::new(),
        }
    }

    /// The states the run may be in, each with the edge that would have led
    /// there.
    fn candidates(&self) -> Vec<(Option<(usize, usize)>, usize)> {
        let target = |node: usize, i: usize| self.graph.nodes()[node].edges[i].target;
        match &self.pending {
            Pending::Nodes(nodes) => nodes.iter().map(|&n| (None, n)).collect(),
            Pending::Tie { node, edges } => edges
                .iter()
                .map(|&i| (Some((*node, i)), target(*node, i)))
                .collect(),
            Pending::Clone {
                node,
                edges,
                shared,
            } => {
                let live = shared.lock().live.clone();
                edges
                    .iter()
                    .zip(live)
                    .filter(|(_, live)| *live)
                    .map(|(&i, _)| (Some((*node, i)), target(*node, i)))
                    .collect()
            }
        }
    }

    /// Arrive at the pending state of `ident`, settling the edge that led
    /// there; a misjoin diverges and is rescued by the graph's node of that
    /// identity, `None` when it has none.
    fn arrive(&mut self, ident: &Ident, stream: &[usize], position: usize) -> Option<usize> {
        let found = self
            .candidates()
            .into_iter()
            .find(|&(_, n)| self.graph.nodes()[n].ident == *ident);
        match found {
            Some((edge, n)) => {
                if let Some(edge) = edge {
                    self.settled.push(edge);
                }
                Some(n)
            }
            None => {
                self.diverge(stream, position);
                self.graph.node(ident)
            }
        }
    }

    /// The identity the draw at `addr` reports, given the previous draw.
    fn ident_at(&mut self, addr: Vec<Frame>) -> Ident {
        let ident = ident_before(self.prev.as_deref(), &addr);
        self.prev = Some(addr);
        ident
    }

    fn diverge(&mut self, stream: &[usize], position: usize) {
        if self.divergence().is_none() {
            self.divergence = Some(Divergence {
                stream: stream.to_vec(),
                position,
            });
        }
    }

    /// The first divergence of the walk or of any child stream.
    fn divergence(&self) -> Option<Divergence> {
        self.divergence.clone().or_else(|| {
            self.children
                .iter()
                .find_map(|c| c.lock().divergence.clone())
        })
    }

    /// Whether the run, ending now, ends on [`Ident::End`].
    fn ended_on_end(&self) -> bool {
        self.candidates().iter().any(|&(_, n)| n == END)
    }

    /// The edges the run settled on, as `(node, edge index)`: those whose
    /// target the next draw's identity picked, and the edge to `End` if the
    /// run ends on it.
    fn settled(&self) -> Vec<(usize, usize)> {
        let mut out = self.settled.clone();
        out.extend(
            self.candidates()
                .into_iter()
                .filter_map(|(edge, n)| (n == END).then_some(edge).flatten()),
        );
        out
    }
}

/// What a replay resolved a draw to.
pub(crate) enum Resolved<'a, V> {
    /// A stored value fit the request.
    Served(V),
    /// The positional value of timeline `.1` misfits; the caller puns. Under
    /// [`Rescue::Pun`] that is the proposal; under [`Rescue::Continue`] it
    /// is a pruned proposal serving as the donor (see [`Replay::shrink_set`]).
    Misfit(&'a ChoiceValue, usize),
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
    nodes: Vec<Option<Vec<ChoiceNode>>>,
    puns: Vec<bool>,
    /// Per timeline, whether a misfit of this proposal is the shrink's own
    /// edit to pun rather than a branch the test took (decision 77: set
    /// once the driver has put enough of its misfits down to the test).
    insist: Vec<bool>,
    rescue: Rescue,
    shared: Arc<Mutex<LiveSet>>,
    external: Option<External>,
    graph: Option<Mutex<GraphWalk>>,
}

impl Replay {
    /// A replay every draw of which is decided by `resolver`.
    #[cfg(any(test, feature = "__bench"))]
    pub(crate) fn external(resolver: Box<dyn ExternalReplay>) -> Self {
        Self::external_view(
            Arc::new(Mutex::new(resolver)),
            Arc::new(Mutex::new(LiveSet::new(0))),
        )
    }

    fn external_view(external: External, shared: Arc<Mutex<LiveSet>>) -> Self {
        Replay {
            timelines: Vec::new(),
            nodes: Vec::new(),
            puns: Vec::new(),
            insist: Vec::new(),
            rescue: Rescue::Continue,
            shared,
            external: Some(external),
            graph: None,
        }
    }

    /// A [`Rescue::Pun`] replay of one proposed sequence.
    pub(crate) fn pun(prefix: Vec<ChoiceValue>, nodes: Option<Vec<ChoiceNode>>) -> Self {
        Replay {
            timelines: alloc::vec![Some(prefix)],
            nodes: alloc::vec![nodes],
            puns: alloc::vec![true],
            insist: alloc::vec![false],
            rescue: Rescue::Pun,
            shared: Arc::new(Mutex::new(LiveSet::new(1))),
            external: None,
            graph: None,
        }
    }

    /// A [`Rescue::Continue`] replay of a whole counterexample, in its order.
    pub(crate) fn counterexample(timelines: Vec<Vec<ChoiceValue>>) -> Self {
        let count = timelines.len();
        Self::live_set(timelines, Arc::new(Mutex::new(LiveSet::new(count))))
    }

    fn live_set(timelines: Vec<Vec<ChoiceValue>>, shared: Arc<Mutex<LiveSet>>) -> Self {
        let count = timelines.len();
        Replay {
            timelines: timelines.into_iter().map(Some).collect(),
            nodes: (0..count).map(|_| None).collect(),
            puns: alloc::vec![false; count],
            insist: alloc::vec![false; count],
            rescue: Rescue::Continue,
            shared,
            external: None,
            graph: None,
        }
    }

    /// The walk of a counterexample stored as a graph (decision 78): see
    /// [`GraphWalk`].
    pub(crate) fn graph(graph: Arc<Graph>) -> Self {
        Replay {
            timelines: Vec::new(),
            nodes: Vec::new(),
            puns: Vec::new(),
            insist: Vec::new(),
            rescue: Rescue::Continue,
            shared: Arc::new(Mutex::new(LiveSet::new(0))),
            external: None,
            graph: Some(Mutex::new(GraphWalk::new(graph))),
        }
    }

    /// A [`Rescue::Continue`] replay of a shrink's candidate set (decision
    /// 77): the timelines flagged in `puns` are unrealized proposals, and
    /// once such a proposal has left the live set by its own misfit it
    /// keeps serving as a donor the way a [`Rescue::Pun`] replay would —
    /// its misfits pun to the draw's simplest or unit value, guided by its
    /// realized `nodes` where known — because those misfits are the
    /// shrink's own edits. The other timelines are realized executions and
    /// donate leniently, as in [`Self::counterexample`]. While the leading
    /// proposal is live it alone decides its own end and, when it
    /// `insist`s, its own misfits: past its last value the run's tail is
    /// random (never the tail of the timeline it shortened), and an
    /// insisting proposal's misfit leaves the set to be punned rather than
    /// letting another timeline serve.
    pub(crate) fn shrink_set(
        timelines: Vec<Vec<ChoiceValue>>,
        nodes: Vec<Option<Vec<ChoiceNode>>>,
        puns: Vec<bool>,
        insist: Vec<bool>,
    ) -> Self {
        let count = timelines.len();
        Replay {
            timelines: timelines.into_iter().map(Some).collect(),
            nodes,
            puns,
            insist,
            rescue: Rescue::Continue,
            shared: Arc::new(Mutex::new(LiveSet::new(count))),
            external: None,
            graph: None,
        }
    }

    /// The proposal's stored value at `position`, for [`Rescue::Pun`].
    fn proposal(&self) -> &[ChoiceValue] {
        self.timelines[0].as_deref().unwrap_or(&[])
    }

    /// The realized node of timeline `timeline`'s proposal at `position`,
    /// when known.
    pub(crate) fn proposal_node(&self, timeline: usize, position: usize) -> Option<&ChoiceNode> {
        self.nodes[timeline].as_ref().and_then(|n| n.get(position))
    }

    fn value_at(&self, timeline: usize, position: usize) -> Option<&ChoiceValue> {
        self.timelines[timeline]
            .as_ref()
            .and_then(|t| t.get(position))
    }

    /// Resolve the draw at `position` of stream `stream`, given the draw's
    /// acceptance test over stored values and, for a graph walk or an
    /// external resolver only, its structural address. A proposal's misfit
    /// under [`Rescue::Pun`] is the shrink's own edit, never a divergence.
    pub(crate) fn resolve<V>(
        &self,
        stream: &[usize],
        position: usize,
        frames: impl FnOnce() -> Vec<Frame>,
        fits: impl Fn(&ChoiceValue) -> Option<V>,
    ) -> Resolved<'_, V> {
        if let Some(walk) = &self.graph {
            return Self::resolve_graph(&mut walk.lock(), stream, position, frames(), fits);
        }
        if let Some(external) = &self.external {
            return match external
                .lock()
                .resolve(stream, position, &frames(), &|v| fits(v).is_some())
            {
                Some(stored) => match fits(&stored) {
                    Some(v) => Resolved::Served(v),
                    None => Resolved::Exhausted,
                },
                None => Resolved::Exhausted,
            };
        }
        match self.rescue {
            Rescue::Pun => match self.proposal().get(position) {
                Some(stored) => match fits(stored) {
                    Some(v) => Resolved::Served(v),
                    None => Resolved::Misfit(stored, 0),
                },
                None => Resolved::Exhausted,
            },
            Rescue::Continue => self.resolve_live(stream, position, fits),
        }
    }

    /// A graph walk's draw: arrive by identity, serve the first fitting
    /// edge at the address, and stand at its tie (see [`GraphWalk`]).
    fn resolve_graph<'a, V>(
        walk: &mut GraphWalk,
        stream: &[usize],
        position: usize,
        addr: Vec<Frame>,
        fits: impl Fn(&ChoiceValue) -> Option<V>,
    ) -> Resolved<'a, V> {
        let ident = walk.ident_at(addr.clone());
        let graph = Arc::clone(&walk.graph);
        let Some(node) = walk.arrive(&ident, stream, position) else {
            walk.pending = Pending::Nodes(Vec::new());
            return Resolved::Exhausted;
        };
        let edges = &graph.nodes()[node].edges;
        let hit = edges
            .iter()
            .enumerate()
            .filter(|(_, e)| e.addr == addr)
            .find_map(|(i, e)| fits(&e.value).map(|v| (i, v)));
        match hit {
            Some((i, v)) => {
                let value = &edges[i].value;
                walk.pending = Pending::Tie {
                    node,
                    edges: (0..edges.len())
                        .filter(|&j| edges[j].addr == addr && edges[j].value == *value)
                        .collect(),
                };
                Resolved::Served(v)
            }
            None => {
                walk.diverge(stream, position);
                walk.pending = Pending::Nodes(Vec::new());
                Resolved::Exhausted
            }
        }
    }

    fn resolve_live<V>(
        &self,
        stream: &[usize],
        position: usize,
        fits: impl Fn(&ChoiceValue) -> Option<V>,
    ) -> Resolved<'_, V> {
        let mut set = self.shared.lock();
        if set.random_tail {
            return Resolved::Exhausted;
        }
        if let Some(first) = set.first_live() {
            let leading_proposal = self.puns[first];
            match self.value_at(first, position) {
                None if leading_proposal => {
                    set.run_out(first, stream, position);
                    return Resolved::Exhausted;
                }
                Some(stored)
                    if leading_proposal && self.insist[first] && fits(stored).is_none() =>
                {
                    set.leave_leading(first, stream, position);
                }
                _ => {
                    let hit = (0..self.timelines.len())
                        .filter(|&k| set.live[k])
                        .find_map(|k| {
                            let stored = self.value_at(k, position)?;
                            fits(stored).map(|v| (stored, v))
                        });
                    match hit {
                        Some((stored, v)) => {
                            let batch: Vec<usize> = (0..self.timelines.len())
                                .filter(|&j| {
                                    set.live[j] && self.value_at(j, position) != Some(stored)
                                })
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
            }
        }
        if (0..self.timelines.len())
            .any(|k| set.realized[k] && self.puns[k] && self.value_at(k, position).is_none())
        {
            set.random_tail = true;
            return Resolved::Exhausted;
        }
        for &k in set.pruned.iter().rev().flatten() {
            let Some(stored) = self.value_at(k, position) else {
                continue;
            };
            match fits(stored) {
                Some(v) => return Resolved::Served(v),
                None if self.puns[k] && set.realized[k] => return Resolved::Misfit(stored, k),
                None => {}
            }
        }
        Resolved::Exhausted
    }

    /// The replay of the stream cloned at `position` of stream `stream`:
    /// each timeline's cloned sequence there, with the timelines that have
    /// no clone at that position leaving the live set. Under a graph walk,
    /// the live-set replay of the records of the clone edges at the clone's
    /// address (`frames`), whose targets the child's agreement decides.
    pub(crate) fn clone_child(
        &self,
        stream: &[usize],
        position: usize,
        frames: impl FnOnce() -> Vec<Frame>,
    ) -> Self {
        if let Some(walk) = &self.graph {
            return Self::clone_from_graph(&mut walk.lock(), stream, position, frames());
        }
        if let Some(external) = &self.external {
            return Self::external_view(Arc::clone(external), Arc::clone(&self.shared));
        }
        let mut set = self.shared.lock();
        let mut timelines = Vec::with_capacity(self.timelines.len());
        let mut nodes = Vec::with_capacity(self.timelines.len());
        let mut batch = Vec::new();
        for k in 0..self.timelines.len() {
            match self.value_at(k, position) {
                Some(ChoiceValue::Clone(record)) => {
                    nodes.push(if self.puns[k] {
                        record.realized_nodes().map(<[ChoiceNode]>::to_vec)
                    } else {
                        None
                    });
                    timelines.push(Some(record.owned_values()));
                }
                _ => {
                    timelines.push(None);
                    nodes.push(None);
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
                let leading_proposal_misfit = set.first_live().filter(|&first| {
                    self.puns[first]
                        && match self.value_at(first, position) {
                            None => true,
                            Some(ChoiceValue::Clone(_)) => false,
                            Some(_) => self.insist[first],
                        }
                });
                if let Some(first) = leading_proposal_misfit {
                    set.run_out(first, stream, position);
                } else {
                    set.prune(batch);
                    if !set.any_live() {
                        set.diverge(stream, position);
                    }
                }
            }
        }
        Replay {
            timelines,
            nodes,
            puns: self.puns.clone(),
            insist: self.insist.clone(),
            rescue: self.rescue,
            shared: Arc::clone(&self.shared),
            external: None,
            graph: None,
        }
    }

    /// A graph walk's clone: arrive by identity, take the clone edges at
    /// the address as the tie, and hand the child their records as a live
    /// set; none is a divergence and an empty child that draws at random.
    fn clone_from_graph(
        walk: &mut GraphWalk,
        stream: &[usize],
        position: usize,
        addr: Vec<Frame>,
    ) -> Self {
        let ident = walk.ident_at(addr.clone());
        let graph = Arc::clone(&walk.graph);
        let node = walk.arrive(&ident, stream, position);
        let (edges, records): (Vec<usize>, Vec<Vec<ChoiceValue>>) = node
            .map(|n| {
                graph.nodes()[n]
                    .edges
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| match &e.value {
                        ChoiceValue::Clone(record) if e.addr == addr => {
                            Some((i, record.owned_values()))
                        }
                        _ => None,
                    })
                    .unzip()
            })
            .unwrap_or_default();
        let shared = Arc::new(Mutex::new(LiveSet::new(records.len())));
        walk.children.push(Arc::clone(&shared));
        walk.pending = match node {
            Some(node) if !edges.is_empty() => Pending::Clone {
                node,
                edges,
                shared: Arc::clone(&shared),
            },
            _ => {
                walk.diverge(stream, position);
                Pending::Nodes(Vec::new())
            }
        };
        Self::live_set(records, shared)
    }

    /// The family's first divergence, if any.
    pub(crate) fn divergence(&self) -> Option<Divergence> {
        if let Some(walk) = &self.graph {
            return walk.lock().divergence();
        }
        if let Some(external) = &self.external {
            return external.lock().divergence();
        }
        self.shared.lock().divergence.clone()
    }

    /// Under a graph walk, the edges the run settled on (see
    /// [`GraphWalk::settled`]); empty otherwise.
    pub(crate) fn settled(&self) -> Vec<(usize, usize)> {
        self.graph
            .as_ref()
            .map_or_else(Vec::new, |walk| walk.lock().settled())
    }

    /// Under a graph walk, whether the run ending now ends on
    /// [`Ident::End`]; false otherwise.
    pub(crate) fn ended_on_end(&self) -> bool {
        self.graph
            .as_ref()
            .is_some_and(|walk| walk.lock().ended_on_end())
    }

    /// Which timelines are live, in counterexample order.
    pub(crate) fn live(&self) -> Vec<bool> {
        self.shared.lock().live.clone()
    }

    /// Which timelines the run realized, in counterexample order: those
    /// live at its end, and those that left the live set at the divergence
    /// — by their own misfit, with nothing else to serve — never by
    /// disagreeing with, or running out while, another timeline served. A
    /// proposal so flagged has been executed as itself, its edits included.
    pub(crate) fn realized(&self) -> Vec<bool> {
        let set = self.shared.lock();
        set.live
            .iter()
            .zip(&set.realized)
            .map(|(live, realized)| *live || *realized)
            .collect()
    }

    /// Whether the leading proposal ran out of values and the run drew its
    /// tail at random (decision 77): the proposal was too short for the
    /// test, not wrong.
    pub(crate) fn ran_out(&self) -> bool {
        self.shared.lock().random_tail
    }

    /// The longest timeline's top-level length: the floor of a replay's
    /// size budget.
    pub(crate) fn longest(&self) -> usize {
        if let Some(external) = &self.external {
            return external.lock().longest();
        }
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

#[cfg(test)]
#[path = "../../../tests/embedded/native/replay_graph_tests.rs"]
mod graph_tests;

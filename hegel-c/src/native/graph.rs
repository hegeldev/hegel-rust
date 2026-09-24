//! The counterexample as a graph of draws (decision 78; experiments
//! 017–019).
//!
//! A nondeterministic failure is not one choice sequence: the test's
//! structure after a draw may depend on a hidden coin, so its failing
//! executions are many sequences with shared parts. The graph stores them
//! together. Its **nodes are states** — where a run is between two draws —
//! and its **edges are draws**: a node's edge carries the draw's address,
//! its value, and the state the run reached next.
//!
//! A draw's **address** is the spans open at it, outermost first, each as
//! `(label, ordinal)`, the ordinal counting the earlier same-label siblings
//! under the same parent. A state's **identity** is the prefix of the next
//! draw's address through its first frame that was not open at the previous
//! draw — the first span the run enters after the last one it left — with
//! [`Ident::Start`] before the first draw and [`Ident::End`] after the last.
//! There is one node per identity, so inserting a run merges it into the
//! graph by identity: two runs that reach the same state share the node and
//! everything after it recombines. When the same address and value lead to
//! different states in different runs the node holds one edge per target —
//! a **tie**, settled at replay by the identity the next draw reports.
//!
//! The graph is walked by `Replay::graph` (`core/replay.rs`) and shrunk by
//! [`crate::native::graph_shrink`]. Nothing here assumes the
//! graph is acyclic: two runs may order sibling spans differently.

use alloc::vec::Vec;

use crate::native::HashMap;
use crate::native::bignum::ToPrimitive;
use crate::native::core::{ChoiceNode, ChoiceValue, Span, float_to_index};
use crate::native::database::{deserialize_choices_exact, serialize_choices};

/// One open span at a draw: its label and the number of earlier siblings
/// under the same parent with that label.
pub(crate) type Frame = (u64, usize);

/// A draw's address: the frames of the spans open at it, outermost first.
pub(crate) type Addr = Vec<Frame>;

/// The identity of a state between two draws (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Ident {
    Start,
    At(Vec<Frame>),
    End,
}

/// One draw of a realized run: where it was and what it drew.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Step {
    pub(crate) addr: Addr,
    pub(crate) value: ChoiceValue,
}

/// A realized execution with the address of every draw.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct Run {
    pub(crate) steps: Vec<Step>,
}

/// The frame of every span: its label and sibling ordinal.
fn span_frames(spans: &[Span]) -> Vec<Frame> {
    let mut counts: HashMap<(Option<usize>, u64), usize> = HashMap::default();
    spans
        .iter()
        .map(|span| {
            let label = span.label;
            let count = counts.entry((span.parent, label)).or_insert(0);
            let ordinal = *count;
            *count += 1;
            (label, ordinal)
        })
        .collect()
}

/// The address of each of `count` draws under `spans`: the frames of the
/// spans containing it, outermost first. Agrees with what
/// `NativeTestCase::open_span_frames` reported at the draw, since a span
/// open at a draw is exactly one that contains it and sibling ordinals only
/// grow.
pub(crate) fn draw_addresses(spans: &[Span], count: usize) -> Vec<Addr> {
    let frames = span_frames(spans);
    let mut open: Vec<usize> = Vec::new();
    let mut next = 0;
    (0..count)
        .map(|i| {
            while next < spans.len() && spans[next].start <= i {
                if spans[next].end > i {
                    open.push(next);
                }
                next += 1;
            }
            open.retain(|&s| spans[s].end > i);
            open.iter().map(|&s| frames[s]).collect()
        })
        .collect()
}

impl Run {
    /// The run `nodes` realized under `spans` (a stream's own spans; a clone
    /// stream's draws are inside its clone value). A forced draw is not a
    /// step: the test decides it without consulting the walk, which never
    /// serves or settles it, so an edge for it would be one no replay can
    /// exercise.
    pub(crate) fn from_nodes(nodes: &[ChoiceNode], spans: &[Span]) -> Run {
        let addrs = draw_addresses(spans, nodes.len());
        Run {
            steps: nodes
                .iter()
                .zip(addrs)
                .filter(|(node, _)| !node.was_forced)
                .map(|(node, addr)| Step {
                    addr,
                    value: node.value(),
                })
                .collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn values(&self) -> Vec<ChoiceValue> {
        self.steps.iter().map(|s| s.value.clone()).collect()
    }

    /// The identity of the state after each draw; the last is [`Ident::End`].
    pub(crate) fn idents(&self) -> Vec<Ident> {
        let mut out = Vec::with_capacity(self.steps.len());
        for pair in self.steps.windows(2) {
            out.push(ident_after(&pair[0].addr, &pair[1].addr));
        }
        if !self.steps.is_empty() {
            out.push(Ident::End);
        }
        out
    }
}

fn common_prefix(a: &[Frame], b: &[Frame]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// The identity of the state between a draw at `prev` and one at `next`:
/// the prefix of `next` through its first frame not open at `prev`. Two
/// draws never share an address (the engine wraps every draw in its own
/// kind span), so the prefix is proper; a bare address is its own identity.
pub(crate) fn ident_after(prev: &[Frame], next: &[Frame]) -> Ident {
    let shared = common_prefix(prev, next);
    Ident::At(next[..(shared + 1).min(next.len())].to_vec())
}

/// The identity of the state before a draw at `next`, given the previous
/// draw's address if any.
pub(crate) fn ident_before(prev: Option<&[Frame]>, next: &[Frame]) -> Ident {
    match prev {
        None => Ident::Start,
        Some(prev) => ident_after(prev, next),
    }
}

/// A draw the graph knows from a state: its address and value, and the
/// state it led to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Edge {
    pub(crate) addr: Addr,
    pub(crate) value: ChoiceValue,
    pub(crate) target: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub(crate) ident: Ident,
    pub(crate) edges: Vec<Edge>,
}

/// How a run relates to a graph's walk (see [`Graph::walk_verdict`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Walked {
    /// The walk serves the whole run and ends on [`Ident::End`].
    Whole,
    /// At some draw the walk serves a different value than the run drew: no
    /// replay of this graph produces the run.
    Foreign,
    /// The walk has no edge for a draw, no tie target of the identity the
    /// run reached, or does not end on [`Ident::End`]: a gap the run fills.
    Gap,
}

/// The counterexample as a graph: see the module docs. Node 0 is
/// [`Ident::Start`], node 1 is [`Ident::End`].
#[derive(Clone, Debug)]
pub(crate) struct Graph {
    nodes: Vec<Node>,
    index: HashMap<Ident, usize>,
}

pub(crate) const START: usize = 0;
pub(crate) const END: usize = 1;

impl Default for Graph {
    fn default() -> Self {
        Graph::new()
    }
}

impl Graph {
    /// The empty graph: `Start` and `End`, no edges.
    pub(crate) fn new() -> Graph {
        let mut g = Graph {
            nodes: Vec::new(),
            index: HashMap::default(),
        };
        g.node_for(&Ident::Start);
        g.node_for(&Ident::End);
        g
    }

    pub(crate) fn from_run(run: &Run) -> Graph {
        let mut g = Graph::new();
        g.insert(run);
        g
    }

    pub(crate) fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub(crate) fn node(&self, ident: &Ident) -> Option<usize> {
        self.index.get(ident).copied()
    }

    fn node_for(&mut self, ident: &Ident) -> usize {
        if let Some(&n) = self.index.get(ident) {
            return n;
        }
        let n = self.nodes.len();
        self.nodes.push(Node {
            ident: ident.clone(),
            edges: Vec::new(),
        });
        self.index.insert(ident.clone(), n);
        n
    }

    fn edge_to(&self, n: usize, step: &Step, want: usize) -> Option<usize> {
        self.nodes[n]
            .edges
            .iter()
            .position(|e| e.target == want && e.addr == step.addr && e.value == step.value)
    }

    /// Insert a realized run: follow it where the graph already has its
    /// draws leading to the states it reached, and add the missing edges
    /// (and the missing states) where it does not. Returns whether anything
    /// was added. An empty run adds nothing: a test that drew nothing has no
    /// structure to store.
    pub(crate) fn insert(&mut self, run: &Run) -> bool {
        let mut n = START;
        let mut added = false;
        for (step, ident) in run.steps.iter().zip(run.idents()) {
            let want = self.node_for(&ident);
            if self.edge_to(n, step, want).is_none() {
                self.nodes[n].edges.push(Edge {
                    addr: step.addr.clone(),
                    value: step.value.clone(),
                    target: want,
                });
                added = true;
            }
            n = want;
        }
        added
    }

    /// Graft a failing run the graph could not produce into it: insert it
    /// when the walk has a gap for it, and leave the graph alone when the
    /// run is foreign — the walk would serve another value somewhere, so
    /// no replay of the graph produces it and its edges would be dead.
    /// Returns whether anything was added.
    pub(crate) fn graft(&mut self, run: &Run) -> bool {
        self.walk_verdict(run) == Walked::Gap && self.insert(run)
    }

    /// The graph with `run` grafted, or — when `run` is foreign to it — the
    /// run's own graph: the counterexample a failing run the graph cannot
    /// produce belongs to.
    pub(crate) fn with_run(&self, run: &Run) -> Graph {
        if self.walk_verdict(run) == Walked::Foreign {
            return Graph::from_run(run);
        }
        let mut g = self.clone();
        g.insert(run);
        g
    }

    /// Whether the first-fit walk — the value served is the first edge at
    /// the draw's address whose kind fits, ties settled by the identity the
    /// run reached — produces the whole run, would serve another value
    /// somewhere, or has a gap the run fills.
    pub(crate) fn walk_verdict(&self, run: &Run) -> Walked {
        let mut n = START;
        for (step, ident) in run.steps.iter().zip(run.idents()) {
            let Some(first) = self.nodes[n]
                .edges
                .iter()
                .find(|e| e.addr == step.addr && value_kind(&e.value) == value_kind(&step.value))
            else {
                return Walked::Gap;
            };
            if first.value != step.value {
                return Walked::Foreign;
            }
            let Some(want) = self.node(&ident) else {
                return Walked::Gap;
            };
            if self.edge_to(n, step, want).is_none() {
                return Walked::Gap;
            }
            n = want;
        }
        if n == END { Walked::Whole } else { Walked::Gap }
    }

    /// The nodes reachable from `Start`, in breadth-first order.
    pub(crate) fn reachable(&self) -> Vec<usize> {
        let mut order = Vec::from([START]);
        let mut seen = alloc::vec![false; self.nodes.len()];
        seen[START] = true;
        let mut i = 0;
        while i < order.len() {
            let n = order[i];
            i += 1;
            for e in &self.nodes[n].edges {
                if !seen[e.target] {
                    seen[e.target] = true;
                    order.push(e.target);
                }
            }
        }
        order
    }

    pub(crate) fn edge_count(&self) -> usize {
        self.reachable()
            .iter()
            .map(|&n| self.nodes[n].edges.len())
            .sum()
    }

    /// The shrink order on graphs: fewer edges, then fewer reachable nodes,
    /// then the edge values in breadth-first order, each by
    /// [`shrink_rank`]. Strictly smaller is better.
    pub(crate) fn key(&self) -> GraphKey {
        let order = self.reachable();
        let mut values = Vec::new();
        for &n in &order {
            for e in &self.nodes[n].edges {
                values.push(shrink_rank(&e.value));
            }
        }
        GraphKey {
            edges: values.len(),
            nodes: order.len(),
            values,
        }
    }

    /// The graph with its unreachable nodes and the edges into them
    /// removed, nodes renumbered in breadth-first order (`Start` and `End`
    /// keep their places).
    pub(crate) fn pruned(&self) -> Graph {
        let mut order = self.reachable();
        if !order.contains(&END) {
            order.push(END);
        }
        order.retain(|&n| n != START && n != END);
        order.insert(0, START);
        order.insert(1, END);
        let mut map = alloc::vec![usize::MAX; self.nodes.len()];
        for (i, &n) in order.iter().enumerate() {
            map[n] = i;
        }
        let mut g = Graph {
            nodes: Vec::with_capacity(order.len()),
            index: HashMap::default(),
        };
        for &n in &order {
            let node = &self.nodes[n];
            g.index.insert(node.ident.clone(), g.nodes.len());
            g.nodes.push(Node {
                ident: node.ident.clone(),
                edges: node
                    .edges
                    .iter()
                    .map(|e| Edge {
                        addr: e.addr.clone(),
                        value: e.value.clone(),
                        target: map[e.target],
                    })
                    .collect(),
            });
        }
        g
    }

    /// The graph without edge `i` of node `n`. A node left without edges
    /// is where a run ends: the edges into it lead to `End` instead (one
    /// of any two made identical), and it goes with the next pruning.
    pub(crate) fn delete_edge(&self, n: usize, i: usize) -> Graph {
        let mut g = self.clone();
        g.nodes[n].edges.remove(i);
        if g.nodes[n].edges.is_empty() {
            for node in &mut g.nodes {
                let mut kept: Vec<Edge> = Vec::with_capacity(node.edges.len());
                for mut e in node.edges.drain(..) {
                    if e.target == n {
                        e.target = END;
                    }
                    if !kept.contains(&e) {
                        kept.push(e);
                    }
                }
                node.edges = kept;
            }
        }
        g
    }

    /// The graph with `value` in place of `old` on every edge of `n` at
    /// `addr`: a tie's edges are one draw, so its value changes together.
    pub(crate) fn set_value(
        &self,
        n: usize,
        addr: &[Frame],
        old: &ChoiceValue,
        value: ChoiceValue,
    ) -> Graph {
        let mut g = self.clone();
        for e in &mut g.nodes[n].edges {
            if e.addr == addr && e.value == *old {
                e.value = value.clone();
            }
        }
        g
    }

    /// Remove every tie alternative no replay settled on: an edge whose
    /// node has another edge with the same address and value in `settled`
    /// (as `(node, edge index)`) while it is not there itself. Ties none of
    /// whose edges were settled are kept whole.
    pub(crate) fn retain_settled(&mut self, settled: &[(usize, usize)]) {
        for (n, node) in self.nodes.iter_mut().enumerate() {
            let edges = &node.edges;
            let keep: Vec<bool> = (0..edges.len())
                .map(|i| {
                    settled.contains(&(n, i))
                        || !(0..edges.len()).any(|j| {
                            j != i
                                && edges[j].addr == edges[i].addr
                                && edges[j].value == edges[i].value
                                && settled.contains(&(n, j))
                        })
                })
                .collect();
            let mut i = 0;
            node.edges.retain(|_| {
                i += 1;
                keep[i - 1]
            });
        }
    }

    /// The wire form: a node count, then per node its identity and edges,
    /// every value as a one-choice [`serialize_choices`] body. `None` if a
    /// value cannot be serialized (a clone nested past the depth limit).
    pub(crate) fn encode(&self) -> Option<Vec<u8>> {
        let mut buf = Vec::new();
        put_u32(&mut buf, self.nodes.len());
        for node in &self.nodes {
            match &node.ident {
                Ident::Start => buf.push(0),
                Ident::End => buf.push(1),
                Ident::At(frames) => {
                    buf.push(2);
                    put_frames(&mut buf, frames);
                }
            }
            put_u32(&mut buf, node.edges.len());
            for e in &node.edges {
                put_frames(&mut buf, &e.addr);
                let value = serialize_choices(core::slice::from_ref(&e.value))?;
                put_u32(&mut buf, value.len());
                buf.extend_from_slice(&value);
                put_u32(&mut buf, e.target);
            }
        }
        Some(buf)
    }

    /// Decode [`Self::encode`] output; `None` on any malformation, an edge
    /// target out of range, a duplicated identity, or nodes 0 and 1 not
    /// being `Start` and `End`.
    pub(crate) fn decode(bytes: &[u8]) -> Option<Graph> {
        let mut rest = bytes;
        let count = take_u32(&mut rest)?;
        if !(2..=MAX_NODES).contains(&count) {
            return None;
        }
        let mut g = Graph {
            nodes: Vec::with_capacity(count),
            index: HashMap::default(),
        };
        for n in 0..count {
            let (&tag, tail) = rest.split_first()?;
            rest = tail;
            let ident = match tag {
                0 => Ident::Start,
                1 => Ident::End,
                2 => Ident::At(take_frames(&mut rest)?),
                _ => return None,
            };
            if (n == START) != (ident == Ident::Start) || (n == END) != (ident == Ident::End) {
                return None;
            }
            if g.index.insert(ident.clone(), n).is_some() {
                return None;
            }
            let edges = take_u32(&mut rest)?;
            if edges > MAX_EDGES {
                return None;
            }
            let mut node = Node {
                ident,
                edges: Vec::with_capacity(edges),
            };
            for _ in 0..edges {
                let addr = take_frames(&mut rest)?;
                let len = take_u32(&mut rest)?;
                if rest.len() < len {
                    return None;
                }
                let (body, tail) = rest.split_at(len);
                rest = tail;
                let mut values = deserialize_choices_exact(body)?;
                if values.len() != 1 {
                    return None;
                }
                let target = take_u32(&mut rest)?;
                if target >= count {
                    return None;
                }
                node.edges.push(Edge {
                    addr,
                    value: values.pop()?,
                    target,
                });
            }
            g.nodes.push(node);
        }
        if !rest.is_empty() {
            return None;
        }
        Some(g)
    }
}

/// Sanity caps on a decoded graph, well above anything the engine writes
/// (a run is at most `BUFFER_SIZE` = 8192 draws; a stored graph is a
/// shrunk failure's).
const MAX_NODES: usize = 1 << 20;
const MAX_EDGES: usize = 1 << 20;
const MAX_FRAMES: usize = 1 << 12;

fn put_u32(buf: &mut Vec<u8>, n: usize) {
    buf.extend_from_slice(&(n as u32).to_le_bytes());
}

fn take_u32(rest: &mut &[u8]) -> Option<usize> {
    let (bytes, tail) = rest.split_first_chunk::<4>()?;
    *rest = tail;
    Some(u32::from_le_bytes(*bytes) as usize)
}

fn put_frames(buf: &mut Vec<u8>, frames: &[Frame]) {
    put_u32(buf, frames.len());
    for &(label, ordinal) in frames {
        buf.extend_from_slice(&label.to_le_bytes());
        put_u32(buf, ordinal);
    }
}

fn take_frames(rest: &mut &[u8]) -> Option<Vec<Frame>> {
    let count = take_u32(rest)?;
    if count > MAX_FRAMES {
        return None;
    }
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        let (label, tail) = rest.split_first_chunk::<8>()?;
        *rest = tail;
        let ordinal = take_u32(rest)?;
        frames.push((u64::from_le_bytes(*label), ordinal));
    }
    Some(frames)
}

/// The shrink order of a graph (see [`Graph::key`]).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct GraphKey {
    edges: usize,
    nodes: usize,
    values: Vec<ValueRank>,
}

/// The kind of a value, as a tag: what a draw's acceptance test is about
/// before it is about the value.
pub(crate) fn value_kind(v: &ChoiceValue) -> u8 {
    match v {
        ChoiceValue::Boolean(_) => 0,
        ChoiceValue::Integer(_) => 1,
        ChoiceValue::Float(_) => 2,
        ChoiceValue::Bytes(_) => 3,
        ChoiceValue::String(_) => 4,
        ChoiceValue::Clone(_) => 5,
    }
}

/// A coarse shrink order on single values for the graph's key: booleans
/// before integers before everything else, `false` before `true`, integers
/// by magnitude, floats by the engine's float shrink index, sequences by
/// length then element by element, clones by flattened length.
pub(crate) fn shrink_rank(v: &ChoiceValue) -> ValueRank {
    match v {
        ChoiceValue::Boolean(b) => (0, *b as u64, Vec::new()),
        ChoiceValue::Integer(n) => (1, n.magnitude().to_u64().unwrap_or(u64::MAX), Vec::new()),
        ChoiceValue::Float(f) => (2, float_to_index(libm::fabs(*f)), Vec::new()),
        ChoiceValue::Bytes(b) => (3, b.len() as u64, b.iter().map(|&x| x as u64).collect()),
        ChoiceValue::String(s) => (3, s.len() as u64, s.iter().map(|&x| x as u64).collect()),
        ChoiceValue::Clone(c) => (4, c.flat_len() as u64, Vec::new()),
    }
}

/// The rank of one value under [`shrink_rank`]: kind, size, elements.
pub(crate) type ValueRank = (u8, u64, Vec<u64>);

#[cfg(test)]
#[path = "../../tests/embedded/native/graph_tests.rs"]
mod tests;

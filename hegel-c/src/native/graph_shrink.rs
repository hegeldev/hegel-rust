//! Shrinking a counterexample graph.
//!
//! The shrinker holds an incumbent [`Graph`] and its **witness** — the
//! smallest clean failing run replayed from it, the example the failure
//! is reported with — and proposes candidates: the graph with one edge
//! deleted; the witness with one span deleted (a draw, or the draws of a
//! span the test opened), its later siblings renumbered and, when the test
//! does not follow that alone, an earlier integer draw lowered by one —
//! the size a test drew before the draws it governs; or the graph with
//! one draw's value replaced by a simpler one. A candidate is judged by
//! replaying it through the gauntlet ([`nd::gauntlet`]): a **clean**
//! replay fails with the target origin, never diverged, and ended where
//! the graph ends. In the fast sweep one unclean replay rejects; the
//! confirmation sweep drives every candidate to a bound. The passes
//! repeat until a confirmation sweep changes nothing.
//!
//! A span deletion is proposed as the graph of the shortened witness
//! alone: the identities of the states after the deleted span are those
//! of a run without it, which the graph's edges cannot be edited into.
//! The other paths of the incumbent are given up by the move; the
//! gauntlet decides whether the shortened run alone is still the failure
//! at the anchor rate, and later grafts restore the paths its replays
//! take.
//!
//! A failing unclean replay is a failing run the graph could not fully
//! produce: it is grafted into the incumbent and the candidate unless it is
//! **foreign** — the graph's walk would have served a different value
//! somewhere, so no replay of the graph produces it. A
//! value edit is accepted only when a clean replay **settled** on the
//! edited draw (exercise by settlement: an edit no failing run drew is not
//! tested), and its tie alternatives no judging replay settled on are
//! pruned with it — the structure the old value led to. A deletion needs
//! no exercise: a smaller graph that reproduces is a smaller
//! counterexample, whether or not the failure still visits the state.
//!
//! Values are shrunk under the constraint of the draw that realized them,
//! learned from every replay; a clone's record is not shrunk (only
//! deleted) in this cut.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;

use crate::backend::RunError;
use crate::control::hegel_internal_unwrap;
use crate::native::HashMap;
use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::core::{ChoiceData, ChoiceNode, ChoiceValue, Span, flattened_len, sort_key};
use crate::native::graph::{Addr, Frame, Graph, Ident, Run, Step, Walked};
use crate::native::nd::{self, Evidence, GauntletVerdict};
use crate::native::shrinker::SweepMode;

/// What one replay of a candidate graph came to.
#[derive(Clone, Debug)]
pub(crate) struct Outcome {
    /// The run failed with the origin being shrunk.
    pub(crate) failed: bool,
    pub(crate) divergence: bool,
    /// The run ended on the graph's `End`.
    pub(crate) ended: bool,
    /// The edges the walk settled on, as `(node, edge index)`.
    pub(crate) settled: Vec<(usize, usize)>,
    pub(crate) nodes: Vec<ChoiceNode>,
    pub(crate) spans: Vec<Span>,
}

impl Outcome {
    /// A clean failure: the candidate produced the whole run and it failed.
    pub(crate) fn clean(&self) -> bool {
        self.failed && !self.divergence && self.ended
    }

    fn run(&self) -> Run {
        Run::from_nodes(&self.nodes, &self.spans)
    }
}

pub(crate) type ProbeFuture<'s> =
    Pin<Box<dyn Future<Output = Result<Outcome, RunError>> + Send + 's>>;

/// The engine's side of a graph shrink. `Send` because the engine's run
/// future, which the shrink suspends inside, is.
pub(crate) trait GraphProbe: Send {
    /// Replay `graph` once as one test case, drawing at random past it up
    /// to `max_size` choices in total.
    fn replay<'s>(&'s mut self, graph: Arc<Graph>, max_size: usize) -> ProbeFuture<'s>;

    /// Charge one gauntlet proposal priced against `anchor` to the origin's
    /// alpha budget and return the failure minimum its
    /// verdicts use; `drive` in the confirmation sweep.
    fn charge(&mut self, anchor: f64, drive: bool) -> u64;

    /// A candidate was accepted: `graph` is the incumbent, `witness` (its
    /// nodes and spans) its reported example, `anchor` the raised anchor,
    /// and `longest` the longest clean failing run seen.
    fn adopted(
        &mut self,
        graph: &Graph,
        witness: (&[ChoiceNode], &[Span]),
        anchor: f64,
        longest: usize,
    ) -> Result<(), RunError>;
}

/// Grafting replays a value edit's judgement absorbs before counting
/// evidence: each adds structure the edit opened, and a graph that keeps
/// opening structure is not converging on a counterexample.
const WARM_UP_GRAFTS: u32 = 8;

/// How many earlier integer draws a span deletion is retried with, nearest
/// first, each lowered by one: the size a test drew before the draws it
/// governs is usually the nearest.
const COUNT_LOWERINGS: u32 = 3;

/// The spans of a run — every proper frame prefix of a draw's address,
/// the draw frame itself being no span — each with the index of its first
/// draw, last-starting first and, at one start, outermost first: the
/// order deletions are proposed in, so that a deletion never renumbers a
/// span still to be proposed.
fn spans_of(run: &Run) -> Vec<(Addr, usize)> {
    let mut seen: HashMap<Addr, usize> = HashMap::default();
    for (i, step) in run.steps.iter().enumerate() {
        for d in 1..step.addr.len() {
            seen.entry(step.addr[..d].to_vec()).or_insert(i);
        }
    }
    let mut spans: Vec<(Addr, usize)> = seen.into_iter().collect();
    spans.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.len().cmp(&b.0.len())));
    spans
}

/// `run` without the draws inside `span`, the later siblings of `span`
/// renumbered as the engine numbers them without it.
fn without_span(run: &Run, span: &[Frame]) -> Run {
    let depth = span.len() - 1;
    let (label, ordinal) = span[depth];
    Run {
        steps: run
            .steps
            .iter()
            .filter(|s| !s.addr.starts_with(span))
            .map(|s| {
                let mut addr = s.addr.clone();
                if addr.len() > depth
                    && addr[..depth] == span[..depth]
                    && addr[depth].0 == label
                    && addr[depth].1 > ordinal
                {
                    addr[depth].1 -= 1;
                }
                Step {
                    addr,
                    value: s.value.clone(),
                }
            })
            .collect(),
    }
}

/// One edge of the incumbent named by identities, so that it can be found
/// again after an accept renumbers the graph.
struct EdgeKey {
    from: Ident,
    addr: Addr,
    value: ChoiceValue,
    to: Ident,
}

fn edge_keys(graph: &Graph) -> Vec<EdgeKey> {
    let nodes = graph.nodes();
    graph
        .reachable()
        .into_iter()
        .flat_map(|n| {
            nodes[n].edges.iter().map(move |e| EdgeKey {
                from: nodes[n].ident.clone(),
                addr: e.addr.clone(),
                value: e.value.clone(),
                to: nodes[e.target].ident.clone(),
            })
        })
        .collect()
}

fn locate(graph: &Graph, key: &EdgeKey) -> Option<(usize, usize)> {
    let n = graph.node(&key.from)?;
    let nodes = graph.nodes();
    let i = nodes[n].edges.iter().position(|e| {
        e.addr == key.addr && e.value == key.value && nodes[e.target].ident == key.to
    })?;
    Some((n, i))
}

/// The edit a candidate makes, for the exercise rule.
enum Edit {
    Delete,
    Value {
        node: usize,
        addr: Addr,
        value: ChoiceValue,
    },
}

enum Verdict {
    Accepted {
        graph: Graph,
        witness: Outcome,
        lower_bound: f64,
    },
    Rejected,
}

/// A shrink of one origin's counterexample graph: see the module docs.
pub(crate) struct GraphShrinker {
    graph: Arc<Graph>,
    witness: Vec<ChoiceNode>,
    witness_spans: Vec<Span>,
    anchor: f64,
    /// The constraint of the draw at each `(state, address)` seen in a
    /// replay, for proposing values that fit.
    constraints: HashMap<(Ident, Addr), ChoiceData>,
    /// The longest clean failing run's flattened length: the floor of the
    /// replay budget.
    longest: usize,
    sweep: SweepMode,
    pub(crate) deadline: Option<crate::sys::Instant>,
    pub(crate) timed_out: bool,
    replays: u64,
}

impl GraphShrinker {
    /// A shrink starting from `graph` with `witness` (its nodes and spans)
    /// as the reported example, pricing candidates against `anchor`.
    pub(crate) fn new(
        graph: Graph,
        witness: Vec<ChoiceNode>,
        spans: Vec<Span>,
        anchor: f64,
    ) -> Self {
        let mut shrinker = GraphShrinker {
            graph: Arc::new(graph),
            witness: Vec::new(),
            witness_spans: Vec::new(),
            anchor,
            constraints: HashMap::default(),
            longest: 0,
            sweep: SweepMode::Fast,
            deadline: None,
            timed_out: false,
            replays: 0,
        };
        shrinker.learn_constraints(&witness, &spans);
        shrinker.longest = flattened_len(&witness);
        shrinker.witness = witness;
        shrinker.witness_spans = spans;
        shrinker
    }

    #[cfg(test)]
    pub(crate) fn graph(&self) -> &Arc<Graph> {
        &self.graph
    }

    #[cfg(test)]
    pub(crate) fn witness(&self) -> (&[ChoiceNode], &[Span]) {
        (&self.witness, &self.witness_spans)
    }

    #[cfg(test)]
    pub(crate) fn anchor(&self) -> f64 {
        self.anchor
    }

    #[cfg(test)]
    pub(crate) fn replays(&self) -> u64 {
        self.replays
    }

    /// Floor the replay budget at `longest`: the longest failing run the
    /// counterexample already holds, when the shrink starts from stored
    /// state whose other paths are longer than the witness.
    pub(crate) fn set_longest(&mut self, longest: usize) {
        self.longest = self.longest.max(longest);
    }

    fn expired(&self) -> bool {
        self.deadline
            .is_some_and(|d| crate::sys::Instant::now().is_some_and(|now| now >= d))
    }

    /// Remember the constraint of every draw of a run, by the state it was
    /// drawn in and its address.
    fn learn_constraints(&mut self, nodes: &[ChoiceNode], spans: &[Span]) {
        let run = Run::from_nodes(nodes, spans);
        let idents = run.idents();
        for (p, (node, step)) in nodes
            .iter()
            .filter(|n| !n.was_forced)
            .zip(&run.steps)
            .enumerate()
        {
            if matches!(node.data, ChoiceData::Clone(_)) {
                continue;
            }
            let before = if p == 0 {
                Ident::Start
            } else {
                idents[p - 1].clone()
            };
            self.constraints
                .insert((before, step.addr.clone()), node.data.clone());
        }
    }

    /// Graft a failing run the incumbent could not produce into it, unless
    /// foreign.
    fn graft(&mut self, run: &Run) {
        if self.graph.walk_verdict(run) == Walked::Gap {
            let mut graph = (*self.graph).clone();
            if graph.insert(run) {
                self.graph = Arc::new(graph);
            }
        }
    }

    /// Run the delete, span, and value passes until a confirmation sweep
    /// changes nothing, or the deadline passes.
    pub(crate) async fn shrink(&mut self, probe: &mut dyn GraphProbe) -> Result<(), RunError> {
        loop {
            let deleted = self.delete_pass(probe).await?;
            let trimmed = self.span_pass(probe).await?;
            let shrunk = self.value_pass(probe).await?;
            if self.timed_out {
                return Ok(());
            }
            if deleted || trimmed || shrunk {
                self.sweep = SweepMode::Fast;
            } else if self.sweep == SweepMode::Fast {
                self.sweep = SweepMode::Confirm;
            } else {
                return Ok(());
            }
        }
    }

    /// Propose deleting each edge of the incumbent in turn.
    async fn delete_pass(&mut self, probe: &mut dyn GraphProbe) -> Result<bool, RunError> {
        let mut changed = false;
        for key in edge_keys(&self.graph) {
            if self.timed_out {
                break;
            }
            let Some((n, i)) = locate(&self.graph, &key) else {
                continue;
            };
            let candidate = self.graph.delete_edge(n, i);
            if let Verdict::Accepted {
                graph,
                witness,
                lower_bound,
            } = self.judge(probe, candidate, Edit::Delete).await?
            {
                self.adopt(probe, graph, witness, lower_bound)?;
                changed = true;
            }
        }
        Ok(changed)
    }

    /// Propose deleting each span of the witness in turn (see the module
    /// docs): the shortened witness, then — when the test does not follow
    /// it — the shortened witness with one of the nearest earlier integer
    /// draws lowered by one, up to [`COUNT_LOWERINGS`] of them.
    async fn span_pass(&mut self, probe: &mut dyn GraphProbe) -> Result<bool, RunError> {
        let mut changed = false;
        for (span, start) in spans_of(&Run::from_nodes(&self.witness, &self.witness_spans)) {
            if self.timed_out {
                break;
            }
            let current = Run::from_nodes(&self.witness, &self.witness_spans);
            if !current.steps.iter().any(|s| s.addr.starts_with(&span)) {
                continue;
            }
            let shorter = without_span(&current, &span);
            if shorter.steps.is_empty() {
                continue;
            }
            if self.try_run(probe, &shorter).await? {
                changed = true;
                continue;
            }
            let idents = current.idents();
            let mut tried = 0;
            for j in (0..start).rev() {
                let before = if j == 0 {
                    Ident::Start
                } else {
                    idents[j - 1].clone()
                };
                let Some(lowered) = self.lowered(&shorter, j, before) else {
                    continue;
                };
                tried += 1;
                if self.try_run(probe, &lowered).await? {
                    changed = true;
                    break;
                }
                if tried == COUNT_LOWERINGS {
                    break;
                }
            }
        }
        Ok(changed)
    }

    /// `run` with step `j` — an integer draw made in state `before`, above
    /// its constraint's simplest value — lowered by one; `None` for any
    /// other step.
    fn lowered(&self, run: &Run, j: usize, before: Ident) -> Option<Run> {
        let step = &run.steps[j];
        let (ChoiceValue::Integer(v), Some(ChoiceData::Integer(ic, _))) = (
            &step.value,
            self.constraints.get(&(before, step.addr.clone())),
        ) else {
            return None;
        };
        if *v <= ic.simplest() {
            return None;
        }
        let mut lowered = run.clone();
        lowered.steps[j].value = ChoiceValue::Integer(v - 1);
        Some(lowered)
    }

    /// Judge the graph of `run` alone as a candidate, adopting it on accept.
    async fn try_run(&mut self, probe: &mut dyn GraphProbe, run: &Run) -> Result<bool, RunError> {
        match self
            .judge(probe, Graph::from_run(run), Edit::Delete)
            .await?
        {
            Verdict::Accepted {
                graph,
                witness,
                lower_bound,
            } => {
                self.adopt(probe, graph, witness, lower_bound)?;
                Ok(true)
            }
            Verdict::Rejected => Ok(false),
        }
    }

    /// Propose simpler values for each draw of the incumbent in turn.
    async fn value_pass(&mut self, probe: &mut dyn GraphProbe) -> Result<bool, RunError> {
        let mut changed = false;
        for key in edge_keys(&self.graph) {
            if self.timed_out {
                break;
            }
            let Some(data) = self.constraints.get(&(key.from.clone(), key.addr.clone())) else {
                continue;
            };
            let data = data.clone();
            changed |= self.shrink_value(probe, &key, &data).await?;
        }
        Ok(changed)
    }

    /// Try `value` for the draw `key` names, when it fits the draw's
    /// constraint; on accept the incumbent moves and `key` is updated to
    /// the new value.
    async fn try_value(
        &mut self,
        probe: &mut dyn GraphProbe,
        key: &mut EdgeKey,
        data: &ChoiceData,
        value: ChoiceValue,
    ) -> Result<bool, RunError> {
        if self.timed_out || value == key.value || data.with_value(&value).is_none() {
            return Ok(false);
        }
        let Some(node) = self.graph.node(&key.from).filter(|&n| {
            self.graph.nodes()[n]
                .edges
                .iter()
                .any(|e| e.addr == key.addr && e.value == key.value)
        }) else {
            return Ok(false);
        };
        let candidate = self
            .graph
            .set_value(node, &key.addr, &key.value, value.clone());
        let edit = Edit::Value {
            node,
            addr: key.addr.clone(),
            value: value.clone(),
        };
        match self.judge(probe, candidate, edit).await? {
            Verdict::Accepted {
                graph,
                witness,
                lower_bound,
            } => {
                self.adopt(probe, graph, witness, lower_bound)?;
                key.value = value;
                Ok(true)
            }
            Verdict::Rejected => Ok(false),
        }
    }

    /// The value moves for one draw: its constraint's simplest value, then
    /// a kind-specific ladder toward it — a binary search for integers,
    /// the integer part for floats, shorter then simpler elements for
    /// sequences.
    async fn shrink_value(
        &mut self,
        probe: &mut dyn GraphProbe,
        key: &EdgeKey,
        data: &ChoiceData,
    ) -> Result<bool, RunError> {
        let mut key = EdgeKey {
            from: key.from.clone(),
            addr: key.addr.clone(),
            value: key.value.clone(),
            to: key.to.clone(),
        };
        let simplest = data.simplest_value()?;
        let mut changed = self.try_value(probe, &mut key, data, simplest).await?;
        if changed {
            return Ok(true);
        }
        match (data, key.value.clone()) {
            (ChoiceData::Integer(ic, _), ChoiceValue::Integer(v)) => {
                let (Some(mut lo), Some(mut hi)) = (ic.simplest().to_i128(), v.to_i128()) else {
                    return Ok(false);
                };
                while !self.timed_out && hi.abs_diff(lo) > 1 {
                    let mid = lo + (hi - lo) / 2;
                    if self
                        .try_value(
                            probe,
                            &mut key,
                            data,
                            ChoiceValue::Integer(BigInt::from(mid)),
                        )
                        .await?
                    {
                        hi = mid;
                        changed = true;
                    } else {
                        lo = mid;
                    }
                }
            }
            (ChoiceData::Float(_, _), ChoiceValue::Float(v)) => {
                let whole = libm::trunc(v);
                if whole != v {
                    changed = self
                        .try_value(probe, &mut key, data, ChoiceValue::Float(whole))
                        .await?;
                }
            }
            (ChoiceData::Bytes(bc, _), ChoiceValue::Bytes(v)) => {
                let mut v = v;
                changed |= self
                    .shrink_sequence(probe, &mut key, data, &mut v, bc.min_size, 0, |b| {
                        ChoiceValue::Bytes(b.to_vec())
                    })
                    .await?;
            }
            (ChoiceData::String(sc, _), ChoiceValue::String(v)) => {
                let mut v = v;
                let simplest = sc.simplest_codepoint()?;
                changed |= self
                    .shrink_sequence(probe, &mut key, data, &mut v, sc.min_size, simplest, |s| {
                        ChoiceValue::String(s.to_vec())
                    })
                    .await?;
            }
            _ => {}
        }
        Ok(changed)
    }

    /// The ladder for a sequence value: halve its length toward `min_len`,
    /// drop its last element, then set each element that is not `zero` to
    /// `zero`.
    async fn shrink_sequence<T: Copy + PartialEq>(
        &mut self,
        probe: &mut dyn GraphProbe,
        key: &mut EdgeKey,
        data: &ChoiceData,
        v: &mut Vec<T>,
        min_len: usize,
        zero: T,
        wrap: impl Fn(&[T]) -> ChoiceValue,
    ) -> Result<bool, RunError> {
        let mut changed = false;
        loop {
            let target = (v.len() / 2).max(min_len);
            if target == v.len() || !self.try_value(probe, key, data, wrap(&v[..target])).await? {
                break;
            }
            v.truncate(target);
            changed = true;
        }
        if v.len() > min_len
            && self
                .try_value(probe, key, data, wrap(&v[..v.len() - 1]))
                .await?
        {
            v.pop();
            changed = true;
        }
        for i in 0..v.len() {
            if v[i] == zero {
                continue;
            }
            let mut zeroed = v.clone();
            zeroed[i] = zero;
            if self.try_value(probe, key, data, wrap(&zeroed)).await? {
                *v = zeroed;
                changed = true;
            }
        }
        Ok(changed)
    }

    /// Judge `candidate` through the gauntlet: see the module docs. A
    /// value edit is first warmed up: while its replays fail with runs it
    /// could not fully produce, they are grafted into it and not counted
    /// (up to [`WARM_UP_GRAFTS`]) — the edited value may lead to structure
    /// the incumbent never had — and on accept the tie alternatives no
    /// clean replay settled on are pruned. An accept stands, and its
    /// ledger is topped up to [`nd::ANCHOR_SEED_RUNS`] runs, unchecked by
    /// the deadline, before its bound seeds the anchor:
    /// stopped at the accept itself, four straight clean replays would
    /// seed 0.51 whatever the candidate's true rate, and an anchor that
    /// never climbs lets a deletion that halves the reproduction rate pass.
    async fn judge(
        &mut self,
        probe: &mut dyn GraphProbe,
        mut candidate: Graph,
        edit: Edit,
    ) -> Result<Verdict, RunError> {
        let min_fails = probe.charge(self.anchor, self.sweep == SweepMode::Confirm);
        let mut evidence = Evidence::default();
        let mut witness: Option<Outcome> = None;
        let mut exercised = matches!(edit, Edit::Delete);
        let mut settled: Vec<(usize, usize)> = Vec::new();
        let mut grafts = 0;
        let mut shared = Arc::new(candidate.clone());
        let mut accepted = false;
        loop {
            if accepted && evidence.runs() >= nd::ANCHOR_SEED_RUNS {
                break;
            }
            if !accepted && self.expired() {
                self.timed_out = true;
                return Ok(Verdict::Rejected);
            }
            let out = probe
                .replay(Arc::clone(&shared), nd::continuation_budget(self.longest))
                .await?;
            self.replays += 1;
            self.learn_constraints(&out.nodes, &out.spans);
            let clean = out.clean();
            if clean {
                self.longest = self.longest.max(flattened_len(&out.nodes));
                if let Edit::Value { node, addr, value } = &edit {
                    exercised |= out.settled.iter().any(|&(m, j)| {
                        let e = &shared.nodes()[m].edges[j];
                        m == *node && e.addr == *addr && e.value == *value
                    });
                }
                settled.extend_from_slice(&out.settled);
                if witness
                    .as_ref()
                    .is_none_or(|w| sort_key(&out.nodes) < sort_key(&w.nodes))
                {
                    witness = Some(out);
                }
            } else if out.failed {
                let run = out.run();
                self.graft(&run);
                if !accepted
                    && matches!(edit, Edit::Value { .. })
                    && grafts < WARM_UP_GRAFTS
                    && candidate.walk_verdict(&run) == Walked::Gap
                    && candidate.insert(&run)
                {
                    grafts += 1;
                    shared = Arc::new(candidate.clone());
                    continue;
                }
            }
            evidence.record(clean);
            if accepted {
                continue;
            }
            if !clean && self.sweep == SweepMode::Fast {
                return Ok(Verdict::Rejected);
            }
            match nd::gauntlet(&evidence, self.anchor, min_fails) {
                GauntletVerdict::Accept if !exercised => return Ok(Verdict::Rejected),
                GauntletVerdict::Accept => accepted = true,
                GauntletVerdict::Reject => return Ok(Verdict::Rejected),
                GauntletVerdict::Continue => {}
            }
        }
        if matches!(edit, Edit::Value { .. }) {
            candidate.retain_settled(&settled);
        }
        let graph = candidate.pruned();
        if graph.key() >= self.graph.key() {
            return Ok(Verdict::Rejected);
        }
        let witness = hegel_internal_unwrap!(
            witness,
            "graph shrink: the gauntlet accepted a candidate without a clean run"
        );
        Ok(Verdict::Accepted {
            graph,
            witness,
            lower_bound: evidence.lower_bound(),
        })
    }

    /// Install an accepted candidate: raise the anchor (capped at
    /// [`nd::anchor_ceiling`], never lowered), take its witness unless the
    /// standing witness is smaller and still a whole walk of the new graph,
    /// and tell the engine.
    fn adopt(
        &mut self,
        probe: &mut dyn GraphProbe,
        graph: Graph,
        witness: Outcome,
        lower_bound: f64,
    ) -> Result<(), RunError> {
        let lower_bound = lower_bound.min(nd::anchor_ceiling());
        if lower_bound > self.anchor {
            self.anchor = lower_bound;
        }
        let standing = Run::from_nodes(&self.witness, &self.witness_spans);
        if sort_key(&witness.nodes) < sort_key(&self.witness)
            || graph.walk_verdict(&standing) != Walked::Whole
        {
            self.witness = witness.nodes;
            self.witness_spans = witness.spans;
        }
        self.graph = Arc::new(graph);
        probe.adopted(
            &self.graph,
            (&self.witness, &self.witness_spans),
            self.anchor,
            self.longest,
        )
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/graph_shrink_tests.rs"]
mod tests;

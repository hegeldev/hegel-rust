//! Shrinking a counterexample graph (decision 78; experiments 018–019).
//!
//! The shrinker holds an incumbent [`Graph`] and its **witness** — the
//! smallest clean failing run replayed from it, the example the failure
//! is reported with — and proposes candidates: the graph with one edge
//! deleted, or with one draw's value replaced by a simpler one. A
//! candidate is judged by replaying it through the gauntlet
//! ([`nd::gauntlet`]): a **clean** replay fails with the target origin,
//! never diverged, and ended where the graph ends. In the fast sweep one
//! unclean replay rejects; the confirmation sweep drives every candidate
//! to a bound. The passes repeat until a confirmation sweep changes
//! nothing.
//!
//! A failing unclean replay is a failing run the graph could not fully
//! produce: it is grafted into the incumbent and the candidate unless it is
//! **foreign** — the graph's walk would have served a different value
//! somewhere, so no replay of the graph produces it (experiment 019). A
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

#![allow(dead_code)]

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
use crate::native::graph::{Addr, Graph, Ident, Run, Walked};
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

/// The engine's side of a graph shrink.
pub(crate) trait GraphProbe {
    /// Replay `graph` once as one test case, drawing at random past it up
    /// to `max_size` choices in total.
    fn replay<'s>(&'s mut self, graph: Arc<Graph>, max_size: usize) -> ProbeFuture<'s>;

    /// Charge one gauntlet proposal to the origin's alpha budget (decision
    /// 72) and return the failure minimum its verdicts use; `drive` in the
    /// confirmation sweep.
    fn charge(&mut self, drive: bool) -> u64;

    /// A candidate was accepted: `graph` is the incumbent, `witness` its
    /// reported example, `anchor` the raised anchor.
    fn adopted(
        &mut self,
        graph: &Graph,
        witness: &[ChoiceNode],
        anchor: f64,
    ) -> Result<(), RunError>;
}

/// Grafting replays a value edit's judgement absorbs before counting
/// evidence: each adds structure the edit opened, and a graph that keeps
/// opening structure is not converging on a counterexample.
const WARM_UP_GRAFTS: u32 = 8;

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

    pub(crate) fn graph(&self) -> &Arc<Graph> {
        &self.graph
    }

    pub(crate) fn witness(&self) -> (&[ChoiceNode], &[Span]) {
        (&self.witness, &self.witness_spans)
    }

    pub(crate) fn anchor(&self) -> f64 {
        self.anchor
    }

    pub(crate) fn replays(&self) -> u64 {
        self.replays
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
        for (p, (node, step)) in nodes.iter().zip(&run.steps).enumerate() {
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

    /// Run the delete and value passes until a confirmation sweep changes
    /// nothing, or the deadline passes.
    pub(crate) async fn shrink(&mut self, probe: &mut dyn GraphProbe) -> Result<(), RunError> {
        loop {
            let deleted = self.delete_pass(probe).await?;
            let shrunk = self.value_pass(probe).await?;
            if self.timed_out {
                return Ok(());
            }
            if deleted || shrunk {
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
    /// clean replay settled on are pruned.
    async fn judge(
        &mut self,
        probe: &mut dyn GraphProbe,
        mut candidate: Graph,
        edit: Edit,
    ) -> Result<Verdict, RunError> {
        let min_fails = probe.charge(self.sweep == SweepMode::Confirm);
        let mut evidence = Evidence::default();
        let mut witness: Option<Outcome> = None;
        let mut exercised = matches!(edit, Edit::Delete);
        let mut settled: Vec<(usize, usize)> = Vec::new();
        let mut grafts = 0;
        let mut shared = Arc::new(candidate.clone());
        loop {
            if self.expired() {
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
                if matches!(edit, Edit::Value { .. })
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
            if !clean && self.sweep == SweepMode::Fast {
                return Ok(Verdict::Rejected);
            }
            match nd::gauntlet(&evidence, self.anchor, min_fails) {
                GauntletVerdict::Accept => break,
                GauntletVerdict::Reject => return Ok(Verdict::Rejected),
                GauntletVerdict::Continue => {}
            }
        }
        if !exercised {
            return Ok(Verdict::Rejected);
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
        probe.adopted(&self.graph, &self.witness, self.anchor)
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/graph_shrink_tests.rs"]
mod tests;

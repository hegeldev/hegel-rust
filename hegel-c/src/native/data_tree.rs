//! Choice-value tree used by the engine driver ([`super::test_runner`]) for
//! novel-prefix generation and non-determinism detection.
//!
//! A small port of the subset of Hypothesis's
//! `internal/conjecture/datatree.py::DataTree` that the runner actually
//! consults: each node stores the [`ChoiceKind`] observed at that
//! position (fixed on first visit), child subtrees keyed by the value
//! drawn, an optional terminal `Status`, and a cached exhaustion flag
//! so the walker can short-circuit dead branches.

use std::collections::HashMap;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use rand::seq::SliceRandom;

use crate::control::hegel_internal_debug_assert;
use crate::native::bignum::BigInt;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Span, SpanEvent, Status};
use crate::native::rng::EngineRng;

/// Hashable choice-value key. `f64` is keyed by its bit pattern so `-0.0`
/// stays distinct from `0.0` and individual NaN payloads are tracked
/// separately — both matter for novel-prefix exhaustion accounting.
#[derive(Clone, PartialEq, Eq, Hash)]
enum ChoiceValueKey {
    Integer(BigInt),
    Boolean(bool),
    Float(u64),
    Bytes(Vec<u8>),
    String(Vec<u32>),
}

impl From<&ChoiceValue> for ChoiceValueKey {
    fn from(v: &ChoiceValue) -> Self {
        match v {
            ChoiceValue::Integer(n) => ChoiceValueKey::Integer(n.clone()),
            ChoiceValue::Boolean(b) => ChoiceValueKey::Boolean(*b),
            ChoiceValue::Float(f) => ChoiceValueKey::Float(f.to_bits()),
            ChoiceValue::Bytes(b) => ChoiceValueKey::Bytes(b.clone()),
            ChoiceValue::String(s) => ChoiceValueKey::String(s.clone()),
        }
    }
}

impl ChoiceValueKey {
    /// Inverse of the `From<&ChoiceValue>` conversion; the key holds the
    /// full value, so this is lossless.
    fn to_value(&self) -> ChoiceValue {
        match self {
            ChoiceValueKey::Integer(n) => ChoiceValue::Integer(n.clone()),
            ChoiceValueKey::Boolean(b) => ChoiceValue::Boolean(*b),
            ChoiceValueKey::Float(bits) => ChoiceValue::Float(f64::from_bits(*bits)),
            ChoiceValueKey::Bytes(b) => ChoiceValue::Bytes(b.clone()),
            ChoiceValueKey::String(s) => ChoiceValue::String(s.clone()),
        }
    }
}

/// The terminal outcome recorded at a leaf: everything needed to rebuild the
/// concluding [`RunResult`](super::test_runner::RunResult) without re-running
/// the test body. Spans aren't here — they're reconstructed from the per-node
/// [`SpanEvent`]s along the path — but the rest of the outcome is.
struct Conclusion {
    status: Status,
    /// Interesting-origin string, for an `Interesting` conclusion.
    origin: Option<String>,
    target_observations: HashMap<String, f64>,
}

/// A test case fully reconstructed from the tree by [`simulate_full`], without
/// executing the body: the realised nodes (from the walk), the spans (replayed
/// from per-node [`SpanEvent`]s), and the recorded [`Conclusion`].
pub(crate) struct SimulatedOutcome {
    pub status: Status,
    pub nodes: Vec<ChoiceNode>,
    pub spans: Vec<Span>,
    pub origin: Option<String>,
    pub target_observations: HashMap<String, f64>,
}

#[derive(Default)]
pub(crate) struct DataTreeNode {
    kind: Option<Arc<ChoiceKind>>,
    /// Whether the draw made *from* this node was forced (set alongside
    /// `kind` on first visit). Forcing is deterministic given the prefix, so
    /// a forced position has exactly one child — the forced value — and a
    /// replay's prefix value at that position is ignored. [`simulate`] needs
    /// this to predict realised values correctly.
    forced: bool,
    children: FxHashMap<ChoiceValueKey, Box<DataTreeNode>>,
    /// Span open/close events that fired at this node's draw position, in
    /// order. Recorded once (per path) by [`record_tree_full`] and replayed by
    /// [`simulate_full`] to rebuild the [`Span`] list faithfully — including
    /// zero-width spans, whose order can't be recovered from the finished list.
    span_events: Vec<SpanEvent>,
    /// Terminal outcome if the test case ended at this node. Only set
    /// when the recording run concluded with `Status >= Invalid`.
    conclusion: Option<Conclusion>,
    /// Cached: true iff the subtree rooted here has been fully explored.
    pub(crate) is_exhausted: bool,
}

/// Iterative drop so a thousands-deep single-path tree doesn't blow
/// the thread's stack via the default recursive drop of
/// `Box<DataTreeNode>`.
impl Drop for DataTreeNode {
    fn drop(&mut self) {
        let mut stack: Vec<Box<DataTreeNode>> =
            self.children.drain().map(|(_, child)| child).collect();
        while let Some(mut node) = stack.pop() {
            stack.extend(node.children.drain().map(|(_, child)| child));
        }
    }
}

impl DataTreeNode {
    /// Recompute `is_exhausted` based on current state. Mirrors
    /// Hypothesis's `TreeNode.check_exhausted`, including its key structural
    /// property: only the *cached* flags of direct children are consulted,
    /// never a recursive descent. [`record_tree`] calls this bottom-up along
    /// the recorded path, so on-path children are already up to date, and
    /// off-path children carry accurate flags from the runs that recorded
    /// them (exhaustion is monotone and only changes along recorded paths).
    /// A recursive descent here both overflows the stack on deep trees and
    /// re-scans whole subtrees on every ascent step.
    fn check_exhausted(&mut self) -> bool {
        if self.is_exhausted {
            return true;
        }
        if self.conclusion.is_some() {
            self.is_exhausted = true;
            return true;
        }
        if let Some(ref kind) = self.kind {
            // Exhausted iff every possible child value has its own node. A
            // forced position has exactly one possible child — the forced
            // value — regardless of the kind's full domain (datatree.py
            // counts forced indices as complete in `check_exhausted`);
            // comparing against the full domain would keep any path through
            // a forced draw from ever exhausting. For unforced positions we
            // only need `max_children <= explored`, and `explored` is a small
            // count, so the saturating form avoids building the huge
            // cardinality `BigUint` (and its `pow`) that `max_children()`
            // would for sequence kinds:
            // `max_children_saturating(explored + 1) <= explored` is exactly
            // `max_children <= explored`.
            let explored = self.children.len() as u128;
            let complete = if self.forced {
                explored >= 1
            } else {
                kind.max_children_saturating(explored + 1) <= explored
            };
            if complete && self.children.values().all(|c| c.is_exhausted) {
                self.is_exhausted = true;
                return true;
            }
        }
        false
    }
}

/// Walk `nodes` through `tree_root`, checking that the schema at every
/// position matches what was observed on previous runs. Records the terminal
/// `status` at the leaf (if the test concluded cleanly) and propagates
/// exhaustion up the path so `generate_novel_prefix` can skip dead branches.
/// `kill_depths` flags spans closed with `discard=true` (mirrors Python's
/// `kill_branch()`).
///
/// Returns `Some(message)` if a non-determinism mismatch was detected (the
/// schema at a position changed from a previous run), so the caller can fold
/// it into a failing [`TestRunResult`] rather than panicking — keeping it from
/// aborting an in-process engine driven over FFI. Returns `None` otherwise.
/// Convenience wrapper for recording a path with only a status (no origin,
/// observations, or span events) — used by the tree's own tests. Production
/// records the full outcome via [`record_tree_full`].
#[cfg(test)]
pub(crate) fn record_tree(
    tree_root: &mut DataTreeNode,
    nodes: &[ChoiceNode],
    status: Status,
    kill_depths: &[usize],
) -> Option<String> {
    record_tree_full(
        tree_root,
        nodes,
        status,
        None,
        &HashMap::new(),
        &[],
        kill_depths,
    )
}

/// As [`record_tree`], but also records everything needed to rebuild the full
/// concluding [`RunResult`](super::test_runner::RunResult) from the tree alone:
/// the interesting `origin`, the `target_observations`, and the per-position
/// `span_events`. With these the tree is lossless — [`simulate_full`] can serve
/// any recorded path, of any status, without re-executing the body.
pub(crate) fn record_tree_full(
    tree_root: &mut DataTreeNode,
    nodes: &[ChoiceNode],
    status: Status,
    origin: Option<&str>,
    target_observations: &HashMap<String, f64>,
    span_events: &[(usize, SpanEvent)],
    kill_depths: &[usize],
) -> Option<String> {
    // Iterative descent: a single-path walk can be thousands deep and
    // a recursive walk would blow the stack.
    let mut path: Vec<*mut DataTreeNode> = Vec::with_capacity(nodes.len() + 1);
    path.push(tree_root as *mut _);

    for first in nodes {
        let parent_ptr = *path.last().unwrap();
        // SAFETY: `parent_ptr` is either the original `tree_root` or a
        // pointer derived from the previous `or_insert_with` borrow; no
        // other live `&mut` aliases the node.
        let node = unsafe { &mut *parent_ptr };
        match &node.kind {
            Some(expected_kind) if *expected_kind != first.kind => {
                return Some(format!(
                    "Your data generation is non-deterministic: at the same choice \
                     position with the same prefix, the schema changed from {:?} to {:?}. \
                     This usually means a generator depends on global mutable state.",
                    expected_kind, first.kind
                ));
            }
            None => {
                node.kind = Some(first.kind.clone());
                node.forced = first.was_forced;
            }
            _ => {}
        }
        let key = ChoiceValueKey::from(&first.value);
        let child = node
            .children
            .entry(key)
            .or_insert_with(|| Box::new(DataTreeNode::default()));
        path.push(child.as_mut() as *mut _);
    }

    // Distribute the span events onto the node for each draw position. A run is
    // deterministic given its path, so re-assigning on a revisited node writes
    // the same events (idempotent); genuine divergence is caught as a schema
    // mismatch above or by the flaky re-verify.
    let mut by_pos: Vec<Vec<SpanEvent>> = vec![Vec::new(); nodes.len() + 1];
    for (pos, ev) in span_events {
        if let Some(slot) = by_pos.get_mut(*pos) {
            slot.push(ev.clone());
        }
    }
    for (depth, events) in by_pos.into_iter().enumerate() {
        // SAFETY: `path[depth]` is a unique pointer into the tree; no other
        // live reference aliases it here.
        let node = unsafe { &mut *path[depth] };
        node.span_events = events;
    }

    if status >= Status::Invalid {
        // SAFETY: leaf pointer is the only live reference into this subtree.
        let leaf = unsafe { &mut **path.last().unwrap() };
        leaf.conclusion = Some(Conclusion {
            status,
            origin: origin.map(str::to_string),
            target_observations: target_observations.clone(),
        });
    }

    for &depth in kill_depths {
        if depth < path.len() {
            // SAFETY: path[depth] is the only live reference to the node.
            let node = unsafe { &mut *path[depth] };
            node.is_exhausted = true;
        }
    }

    // Ascend, calling `check_exhausted` bottom-up.
    while let Some(p) = path.pop() {
        // SAFETY: `p` is the just-popped pointer; no other live
        // reference exists to that node at this point.
        let node = unsafe { &mut *p };
        node.check_exhausted();
    }

    None
}

/// Small-domain cap for enumeration fallback in
/// `pick_non_exhausted_value`.
const ENUMERATION_CAP: u64 = 1024;

/// Pick a choice value whose subtree is either absent from `children`
/// or present but not marked exhausted. Returns `None` only when every
/// known child is exhausted (and the caller should treat the parent as
/// exhausted too).
fn pick_non_exhausted_value(
    kind: &ChoiceKind,
    children: &FxHashMap<ChoiceValueKey, Box<DataTreeNode>>,
    rng: &mut EngineRng,
) -> Option<ChoiceValue> {
    for _ in 0..10 {
        let value = kind.random_value(rng);
        let key = ChoiceValueKey::from(&value);
        match children.get(&key) {
            Some(child) if child.is_exhausted => continue,
            _ => return Some(value),
        }
    }
    let candidates = kind.enumerate(ENUMERATION_CAP)?;
    let mut untried: Vec<ChoiceValue> = candidates
        .into_iter()
        .filter(|v| {
            let key = ChoiceValueKey::from(v);
            children.get(&key).is_none_or(|c| !c.is_exhausted)
        })
        .collect();
    if untried.is_empty() {
        // `check_exhausted` propagates exhausted-ness up the tree as
        // soon as every child is exhausted, so by the time we reach
        // here the caller would already have stopped walking.
        return None; // nocov
    }
    untried.shuffle(rng);
    untried.into_iter().next()
}

/// Walk the data tree and return a prefix of choice values that stops
/// at the first novel position. Port of
/// `DataTree.generate_novel_prefix` simplified for hegel's tree shape.
/// An empty prefix means "draw everything fresh" — correct on the
/// first call, when the tree is empty.
pub(crate) fn generate_novel_prefix(
    tree_root: &DataTreeNode,
    rng: &mut EngineRng,
) -> Vec<ChoiceValue> {
    if tree_root.is_exhausted {
        return Vec::new();
    }
    let mut prefix = Vec::new();
    let mut current = tree_root;
    while let Some(ref kind) = current.kind {
        if current.forced {
            // A forced position always realises the recorded value: the
            // replay ignores the prefix slot (though one must be present for
            // the cursor to advance), so emitting anything else would
            // truncate the walk here and "novelly" land on an
            // already-explored path. Mirror datatree.py, which appends the
            // forced value and keeps descending.
            let (key, child) = current
                .children
                .iter()
                .next()
                .expect("a forced node records its single child in the same run");
            prefix.push(key.to_value());
            // `check_exhausted` treats a forced position as having exactly
            // one possible child, so an exhausted child would have marked
            // `current` exhausted — and the walk never enters exhausted
            // nodes.
            hegel_internal_debug_assert!(!child.is_exhausted);
            current = child;
            continue;
        }
        let Some(value) = pick_non_exhausted_value(kind.as_ref(), &current.children, rng) else {
            break;
        };
        let key = ChoiceValueKey::from(&value);
        let next = current.children.get(&key);
        prefix.push(value);
        match next {
            Some(child) if !child.is_exhausted => current = child,
            _ => break,
        }
    }
    prefix
}

/// Predict the outcome of replaying `choices` through
/// [`super::core::NativeTestCase::for_choices`] *without* running the test
/// body, by walking them through the recorded tree. Port of the subset of
/// Hypothesis's `DataTree.simulate_test_function` the runner needs.
///
/// The walk reproduces how `resolve_choice` turns a replayed prefix value
/// into the *realised* value the tree is keyed on, which is **not** always
/// the prefix value itself:
///
/// * At a **forced** position the prefix value is ignored and the draw
///   always yields the single recorded (forced) value; the prefix cursor
///   still advances past the skipped slot.
/// * At an unforced position whose prefix value fails the requested kind's
///   validation, the draw puns to `kind.unit()` (there is no original-kind
///   information for a bare `for_choices` replay, so the `simplest()` branch
///   never applies).
///
/// Returns `Some(status)` when the walk reaches a recorded conclusion
/// (either because a previous run terminated exactly here, or because it
/// terminated at a prefix of `choices` whose trailing values are never
/// read). Returns `None` — Hypothesis's `PreviouslyUnseenBehaviour` — when
/// the realised path diverges from anything seen before, or when `choices`
/// runs out while the tree still expects another draw (a real run would
/// overrun, which `for_choices` never records as a conclusion); in both
/// cases the caller must actually execute to learn the outcome.
///
/// Only `Status >= Invalid` is ever recorded as a conclusion (see
/// [`record_tree`]), so `EarlyStop` is never returned.
#[cfg(test)]
pub(crate) fn simulate(tree_root: &DataTreeNode, choices: &[ChoiceValue]) -> Option<Status> {
    simulate_full(tree_root, choices).map(|o| o.status)
}

/// As [`simulate`], but returns the *entire* recorded outcome — realised nodes,
/// reconstructed spans, status, origin, and target observations — so a
/// tree-determined path of any status can be served without running the body.
///
/// The realised nodes are recovered from the walk (each value from the child's
/// [`ChoiceValueKey`], losslessly — see [`ChoiceValueKey::to_value`]). The spans
/// are rebuilt by replaying each node's [`SpanEvent`]s through the same
/// span-stack discipline `start_span`/`stop_span` use live, deriving
/// `start`/`end`/`depth`/`parent`/`discarded`; any spans still open at the
/// conclusion are closed there exactly as `freeze` does.
pub(crate) fn simulate_full(
    tree_root: &DataTreeNode,
    choices: &[ChoiceValue],
) -> Option<SimulatedOutcome> {
    let mut current = tree_root;
    let mut nodes: Vec<ChoiceNode> = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    // Indices into `spans` for currently-open spans (nesting order), mirroring
    // `NativeTestCase::span_stack`.
    let mut span_stack: Vec<usize> = Vec::new();
    // `i` tracks the prefix cursor, which equals `nodes.len()` in the real
    // run: every draw — forced or not — advances it by one.
    let mut i = 0usize;
    loop {
        // Replay the span events recorded at this draw position, before either
        // concluding or drawing — exactly when they fired live.
        let pos = nodes.len();
        for ev in &current.span_events {
            match ev {
                SpanEvent::Open { label } => {
                    let parent = span_stack.last().copied();
                    let depth = span_stack.len() as u32;
                    let idx = spans.len();
                    spans.push(Span {
                        start: pos,
                        end: pos,
                        label: label.to_string(),
                        depth,
                        parent,
                        discarded: false,
                    });
                    span_stack.push(idx);
                }
                SpanEvent::Close { discarded } => {
                    if let Some(idx) = span_stack.pop() {
                        spans[idx].end = pos;
                        spans[idx].discarded = *discarded;
                    }
                }
            }
        }
        // A run terminated here (drawing fewer choices than we may have walked
        // past); its outcome is fixed regardless of any later values.
        if let Some(concl) = &current.conclusion {
            // Close any spans still open at conclusion, as `freeze` does
            // (`end` = current position; `discarded` stays false).
            while let Some(idx) = span_stack.pop() {
                spans[idx].end = pos;
            }
            return Some(SimulatedOutcome {
                status: concl.status,
                nodes,
                spans,
                origin: concl.origin.clone(),
                target_observations: concl.target_observations.clone(),
            });
        }
        // No draw was ever made from this node (and it didn't conclude), so
        // the path beyond it is unknown.
        let kind = current.kind.as_ref()?;
        // `for_choices` caps `max_size` at `choices.len()`, so the draw that
        // would read position `choices.len()` overruns (EarlyStop) — and
        // that is never recorded as a conclusion. We can't predict it.
        if i >= choices.len() {
            return None;
        }
        let (realised, next) = if current.forced {
            // Forced: ignore the prefix value, follow the single recorded
            // (forced) child; its key is the forced value.
            let (key, next) = current.children.iter().next()?;
            (key.to_value(), next.as_ref())
        } else {
            let realised = if kind.validate(&choices[i]) {
                choices[i].clone()
            } else {
                kind.unit()
            };
            let next = current.children.get(&ChoiceValueKey::from(&realised))?;
            (realised, next.as_ref())
        };
        nodes.push(ChoiceNode::new((**kind).clone(), realised, current.forced));
        i += 1;
        current = next;
    }
}

/// Concatenate `database_key + b"." + sub` to derive a sub-corpus key.
/// Mirrors `ConjectureRunner.sub_key`.
pub(crate) fn sub_key(database_key: &[u8], sub: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(database_key.len() + 1 + sub.len());
    out.extend_from_slice(database_key);
    out.push(b'.');
    out.extend_from_slice(sub);
    out
}

#[cfg(test)]
#[path = "../../tests/embedded/native/data_tree_tests.rs"]
mod tests;

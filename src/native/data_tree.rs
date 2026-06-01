//! Choice-value tree used by [`super::test_runner::NativeTestRunner`] for
//! novel-prefix generation and non-determinism detection.
//!
//! A small port of the subset of Hypothesis's
//! `internal/conjecture/datatree.py::DataTree` that the runner actually
//! consults: each node stores the [`ChoiceKind`] observed at that
//! position (fixed on first visit), child subtrees keyed by the value
//! drawn, an optional terminal `Status`, and a cached exhaustion flag
//! so the walker can short-circuit dead branches.

use std::sync::Arc;

use rustc_hash::FxHashMap;

use rand::rngs::SmallRng;
use rand::seq::SliceRandom;

use crate::native::bignum::BigInt;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Status};

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
    /// Terminal status if the test case ended at this node. Only set
    /// when the recording run concluded with `Status >= Invalid`.
    conclusion: Option<Status>,
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
    /// Hypothesis's `TreeNode.check_exhausted`.
    fn check_exhausted(&mut self) -> bool {
        if self.is_exhausted {
            return true;
        }
        if self.conclusion.is_some() {
            self.is_exhausted = true;
            return true;
        }
        if let Some(ref kind) = self.kind {
            // Exhausted iff every possible child value has its own node. We only
            // need `max_children <= explored`, and `explored` is a small count,
            // so the saturating form avoids building the huge cardinality
            // `BigUint` (and its `pow`) that `max_children()` would for sequence
            // kinds: `max_children_saturating(explored + 1) <= explored` is
            // exactly `max_children <= explored`.
            let explored = self.children.len() as u128;
            if kind.max_children_saturating(explored + 1) <= explored {
                let all_exhausted = self.children.values_mut().all(|c| c.check_exhausted());
                if all_exhausted {
                    self.is_exhausted = true;
                    return true;
                }
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
pub(crate) fn record_tree(
    tree_root: &mut DataTreeNode,
    nodes: &[ChoiceNode],
    status: Status,
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

    if status >= Status::Invalid {
        // SAFETY: leaf pointer is the only live reference into this subtree.
        let leaf = unsafe { &mut **path.last().unwrap() };
        leaf.conclusion = Some(status);
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
    rng: &mut SmallRng,
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
    rng: &mut SmallRng,
) -> Vec<ChoiceValue> {
    if tree_root.is_exhausted {
        return Vec::new();
    }
    let mut prefix = Vec::new();
    let mut current = tree_root;
    while let Some(ref kind) = current.kind {
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
pub(crate) fn simulate(tree_root: &DataTreeNode, choices: &[ChoiceValue]) -> Option<Status> {
    let mut current = tree_root;
    // `i` tracks the prefix cursor, which equals `nodes.len()` in the real
    // run: every draw — forced or not — advances it by one.
    let mut i = 0usize;
    loop {
        // A run terminated here drawing fewer choices than we walked past;
        // its outcome is fixed regardless of any later values.
        if let Some(status) = current.conclusion {
            return Some(status);
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
        let next = if current.forced {
            // Forced: ignore the prefix value, follow the single recorded
            // (forced) child.
            current.children.values().next()?
        } else {
            let realised = if kind.validate(&choices[i]) {
                choices[i].clone()
            } else {
                kind.unit()
            };
            current.children.get(&ChoiceValueKey::from(&realised))?
        };
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

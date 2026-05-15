//! Choice-value tree used by [`super::test_runner::NativeTestRunner`] for
//! novel-prefix generation and non-determinism detection.
//!
//! A small port of the subset of Hypothesis's
//! `internal/conjecture/datatree.py::DataTree` that the runner actually
//! consults: each node stores the [`ChoiceKind`] observed at that
//! position (fixed on first visit), child subtrees keyed by the value
//! drawn, an optional terminal `Status`, and a cached exhaustion flag
//! so the walker can short-circuit dead branches.

use std::collections::HashMap;

use rand::rngs::SmallRng;
use rand::seq::SliceRandom;

use crate::native::bignum::BigUint;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Status};

/// Hashable choice-value key. `f64` is keyed by its bit pattern so `-0.0`
/// stays distinct from `0.0` and individual NaN payloads are tracked
/// separately — both matter for novel-prefix exhaustion accounting.
#[derive(Clone, PartialEq, Eq, Hash)]
enum ChoiceValueKey {
    Integer(i128),
    Boolean(bool),
    Float(u64),
    Bytes(Vec<u8>),
}

impl From<&ChoiceValue> for ChoiceValueKey {
    fn from(v: &ChoiceValue) -> Self {
        match v {
            ChoiceValue::Integer(n) => ChoiceValueKey::Integer(*n),
            ChoiceValue::Boolean(b) => ChoiceValueKey::Boolean(*b),
            ChoiceValue::Float(f) => ChoiceValueKey::Float(f.to_bits()),
            ChoiceValue::Bytes(b) => ChoiceValueKey::Bytes(b.clone()),
        }
    }
}

#[derive(Default)]
pub(crate) struct DataTreeNode {
    kind: Option<ChoiceKind>,
    children: HashMap<ChoiceValueKey, Box<DataTreeNode>>,
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
            let max_c = kind.max_children();
            if BigUint::from(self.children.len() as u64) >= max_c {
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

/// Walk `nodes` through `tree_root`, asserting that the schema at every
/// position matches what was observed on previous runs. A mismatch
/// panics with a non-determinism wording. Records the terminal `status`
/// at the leaf (if the test concluded cleanly) and propagates
/// exhaustion up the path so `generate_novel_prefix` can skip dead
/// branches. `kill_depths` flags spans closed with `discard=true`
/// (mirrors Python's `kill_branch()`).
pub(crate) fn record_tree(
    tree_root: &mut DataTreeNode,
    nodes: &[ChoiceNode],
    status: Status,
    kill_depths: &[usize],
) {
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
                panic!(
                    "Your data generation is non-deterministic: at the same choice \
                     position with the same prefix, the schema changed from {:?} to {:?}. \
                     This usually means a generator depends on global mutable state.",
                    expected_kind, first.kind
                );
            }
            None => {
                node.kind = Some(first.kind.clone());
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
    children: &HashMap<ChoiceValueKey, Box<DataTreeNode>>,
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
        let Some(value) = pick_non_exhausted_value(kind, &current.children, rng) else {
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

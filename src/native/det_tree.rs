//! Shared non-determinism detection trie.
//!
//! Records the [`ChoiceKind`] observed at each prefix position; a divergent
//! kind at a previously-seen position means the test is generating data
//! non-deterministically (e.g. depending on global mutable state).
//!
//! Pre-N1 this same trie was duplicated between [`super::tree`] and
//! [`super::test_runner`]: each had its own `ChoiceValueKey` enum, its own
//! trie struct (`TreeNode` / `DetTreeNode`), and its own copy of the
//! divergent-kind panic wording. Wording drift between the two caused
//! T0.1; structural drift would have caused more. This module is the
//! single source of truth for both runners.
//!
//! Note: `super::conjecture_runner` has a *separate*, richer trie
//! ([`super::conjecture_runner::DataTreeNode`]) that additionally tracks
//! conclusion status, kill depths, and exhaustion for novel-prefix
//! generation. It is *not* unified here — its bookkeeping is specific to
//! the conjecture runner's generation loop.

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue};

/// Hashable version of [`ChoiceValue`] for use as trie / cache keys.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChoiceValueKey {
    Integer(i128),
    Boolean(bool),
    Float(u64), // f64::to_bits()
    Bytes(Vec<u8>),
    String(Vec<u32>),
}

impl From<&ChoiceValue> for ChoiceValueKey {
    fn from(v: &ChoiceValue) -> Self {
        match v {
            ChoiceValue::Integer(n) => ChoiceValueKey::Integer(*n),
            ChoiceValue::Boolean(b) => ChoiceValueKey::Boolean(*b),
            ChoiceValue::Float(f) => ChoiceValueKey::Float(f.to_bits()),
            ChoiceValue::Bytes(b) => ChoiceValueKey::Bytes(b.clone()),
            ChoiceValue::String(s) => ChoiceValueKey::String(s.clone()),
        }
    }
}

/// A node in the non-determinism detection trie.
///
/// `kind` is the expected schema at this position (set on first visit and
/// shared by every draw made here); `children` branches to the position
/// following this draw, keyed by the choice value.
pub struct DetTreeNode {
    pub kind: Option<ChoiceKind>,
    pub children: HashMap<ChoiceValueKey, DetTreeNode>,
}

impl DetTreeNode {
    pub fn new() -> Self {
        DetTreeNode {
            kind: None,
            children: HashMap::new(),
        }
    }
}

impl Default for DetTreeNode {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DetTreeNode {
    fn drop(&mut self) {
        // Iterative drop so a thousands-deep single-path trie (e.g. one
        // built when an `vecs(vecs(vecs(booleans())))` produces long choice
        // sequences) doesn't overflow the thread's stack via the default
        // recursive drop.
        let mut stack: Vec<DetTreeNode> = self.children.drain().map(|(_, v)| v).collect();
        while let Some(mut node) = stack.pop() {
            stack.extend(node.children.drain().map(|(_, v)| v));
        }
    }
}

/// Walk `nodes` through `root`, recording the kind at each position and
/// panicking if a previously-seen position now reports a different kind
/// (the non-determinism diagnostic).
///
/// The wording is intentionally fixed here — both engine paths share it,
/// so user-facing diagnostics are identical regardless of which path
/// detected the divergence.
pub fn record_into(root: &mut DetTreeNode, nodes: &[ChoiceNode]) {
    let mut current = root;
    for node in nodes {
        if let Some(ref expected_kind) = current.kind {
            if *expected_kind != node.kind {
                panic!(
                    "Your data generation is non-deterministic: at the same choice \
                     position with the same prefix, the schema changed from {:?} to {:?}. \
                     This usually means a generator depends on global mutable state.",
                    expected_kind, node.kind
                );
            }
        } else {
            current.kind = Some(node.kind.clone());
        }
        let key = ChoiceValueKey::from(&node.value);
        current = current.children.entry(key).or_default();
    }
}

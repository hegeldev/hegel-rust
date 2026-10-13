//! Detection of an exhausted search space: a trie of the choice values of
//! every concluded generation-phase execution, kept only while every choice
//! drawn so far has a small finite domain — a boolean, a forced value, or an
//! integer over a range of at most [`MAX_ARITY`] values. The first draw
//! outside that (an unbounded integer, a float, bytes, a string, a clone),
//! an overrun, or a trie past [`MAX_NODES`] abandons tracking for the rest
//! of the run, since the space can no longer be shown exhausted. A position
//! is exhausted once every value of its domain leads to an exhausted
//! position, a conclusion being trivially exhausted; the run's space is
//! exhausted when the root is.
//!
//! This is the one job of Hypothesis's `DataTree` the flat execution cache
//! does not do. Tracking costs one hash lookup per choice and is off for
//! nearly every real test after its first wide draw, so it is far from the
//! wall-clock overhead that retired the tree.

use alloc::vec::Vec;

use crate::native::HashMap;
use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Status};

/// Largest integer range tracked per position.
const MAX_ARITY: u64 = 1 << 12;

/// Trie size at which tracking is abandoned.
const MAX_NODES: usize = 1 << 16;

#[derive(Default)]
struct Node {
    /// How many distinct values the choice at this position can take, once
    /// a run has drawn here.
    arity: Option<u64>,
    children: HashMap<u64, usize>,
    exhausted_children: u64,
    concluded: bool,
    exhausted: bool,
}

/// The run's record of which inputs have been tried.
pub(crate) struct SearchSpace {
    nodes: Vec<Node>,
    tracking: bool,
}

impl Default for SearchSpace {
    fn default() -> Self {
        SearchSpace {
            nodes: alloc::vec![Node::default()],
            tracking: true,
        }
    }
}

impl SearchSpace {
    /// Whether every input of the space has concluded.
    pub(crate) fn is_exhausted(&self) -> bool {
        self.tracking && self.nodes[0].exhausted
    }

    /// Stop tracking for the rest of the run.
    pub(crate) fn give_up(&mut self) {
        self.tracking = false;
        self.nodes = Vec::new();
    }

    /// Record one executed test case.
    pub(crate) fn record(&mut self, status: Status, nodes: &[ChoiceNode]) {
        if !self.tracking {
            return;
        }
        if status == Status::EarlyStop {
            self.give_up();
            return;
        }
        let mut path = Vec::with_capacity(nodes.len() + 1);
        let mut idx = 0;
        path.push(idx);
        for node in nodes {
            let Some((arity, key)) = child_key(node) else {
                self.give_up();
                return;
            };
            let here = &mut self.nodes[idx];
            if here.concluded || here.arity.is_some_and(|a| a != arity) {
                self.give_up();
                return;
            }
            here.arity = Some(arity);
            idx = match here.children.get(&key) {
                Some(&child) => child,
                None => {
                    let child = self.nodes.len();
                    if child >= MAX_NODES {
                        self.give_up();
                        return;
                    }
                    self.nodes[idx].children.insert(key, child);
                    self.nodes.push(Node::default());
                    child
                }
            };
            path.push(idx);
        }
        let leaf = &mut self.nodes[idx];
        if leaf.arity.is_some() {
            self.give_up();
            return;
        }
        leaf.concluded = true;
        if leaf.exhausted {
            return;
        }
        leaf.exhausted = true;
        for &parent in path.iter().rev().skip(1) {
            let node = &mut self.nodes[parent];
            node.exhausted_children += 1;
            if node.arity != Some(node.exhausted_children) {
                break;
            }
            node.exhausted = true;
        }
    }
}

/// The size of `node`'s domain and the index of its value within it, or
/// `None` when the domain is not small and finite.
fn child_key(node: &ChoiceNode) -> Option<(u64, u64)> {
    if node.was_forced {
        return Some((1, 0));
    }
    match (node.kind(), node.value()) {
        (ChoiceKind::Boolean(_), ChoiceValue::Boolean(value)) => Some((2, u64::from(value))),
        (ChoiceKind::Integer(choice), ChoiceValue::Integer(value)) => {
            let arity = (&choice.max_value - &choice.min_value + BigInt::from(1)).to_u64()?;
            if arity > MAX_ARITY {
                return None;
            }
            Some((arity, (value - &choice.min_value).to_u64()?))
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/exhaustion_tests.rs"]
mod tests;

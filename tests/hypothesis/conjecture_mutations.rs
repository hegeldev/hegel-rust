//! Ported from hypothesis-python/tests/conjecture/test_mutations.py
//!
//! Exercises the native engine's span-mutation generation behaviour
//! (`src/native/runner.rs::try_span_mutation`) through the public generator
//! API. Without span mutation, finding a tree whose root and right-child
//! share their `(value, left-subtree)` pair is extremely unlikely from
//! random draws alone, so this test pins the engine behaviour.

#![cfg(feature = "native")]

use crate::common::utils::find_any;
use hegel::generators::{self as gs, Generator};

#[derive(Debug, Clone, PartialEq)]
enum Tree {
    Leaf,
    Node(i64, Box<Tree>, Box<Tree>),
}

#[test]
fn test_can_find_duplicated_subtree() {
    // Look for a tree `(a, b, (a, b, _))` — root and right-child share
    // value `a` and left-subtree `b`. If we only required `(b, c, d)` to
    // appear twice we could hit it by chance; also matching the root's
    // value `a` makes this effectively unreachable without mutation.
    let tree_def = gs::deferred::<Tree>();
    let tree = tree_def.generator();
    tree_def.set(hegel::one_of!(
        gs::just(Tree::Leaf),
        gs::tuples!(gs::integers::<i64>(), tree.clone(), tree.clone())
            .map(|(v, l, r)| Tree::Node(v, Box::new(l), Box::new(r))),
    ));

    find_any(tree, |v: &Tree| match v {
        Tree::Node(a, b, c) => match c.as_ref() {
            Tree::Node(c0, c1, _) if matches!(b.as_ref(), Tree::Node(..)) => {
                a == c0 && b.as_ref() == c1.as_ref()
            }
            _ => false,
        },
        Tree::Leaf => false,
    });
}

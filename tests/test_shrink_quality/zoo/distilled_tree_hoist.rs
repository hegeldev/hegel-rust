//! Control: a recursive tree whose dead interior nodes must be *hoisted* out.
//!
//! `tree(depth)`: `kind ∈ {0, 1}` (only 0 at depth 0); kind 0 is `Leaf(v ∈ [0, 100])`, kind 1 is
//! `Node(children)` with the children drawn `many_more`-style — a boolean continue bit before
//! each child, `false` to stop — to depth 3. `any_leaf_nonzero` fails iff some leaf is non-zero
//! (ideal `Leaf(1)`, every wrapper dead); `nonzero_leaf_under_a_node` fails iff some non-zero
//! leaf has a parent (ideal `Node([Leaf(1)])`). Replacing `Node([x])` by `x` deletes the node's
//! kind and first continue bit *and* its closing `false`, two chunks with the child between. A
//! human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

#[derive(Debug, PartialEq, Eq)]
enum Tree {
    Leaf(i64),
    Node(Vec<Tree>),
}

fn tree(tc: &TestCase, depth: u32) -> Tree {
    let max_kind = if depth == 0 { 0 } else { 1 };
    let kind = tc.draw_silent(gs::integers::<u8>().max_value(max_kind));
    if kind == 0 {
        Tree::Leaf(tc.draw_silent(gs::integers::<i64>().min_value(0).max_value(100)))
    } else {
        let mut children = Vec::new();
        while children.len() < 3 && tc.draw_silent(gs::booleans()) {
            children.push(tree(tc, depth - 1));
        }
        Tree::Node(children)
    }
}

fn draw(tc: &TestCase) -> Tree {
    tree(tc, 3)
}

fn leaves(t: &Tree, depth: usize, out: &mut Vec<(i64, usize)>) {
    match t {
        Tree::Leaf(v) => out.push((*v, depth)),
        Tree::Node(children) => {
            for c in children {
                leaves(c, depth + 1, out);
            }
        }
    }
}

fn any_leaf_nonzero(t: &Tree) -> bool {
    let mut ls = Vec::new();
    leaves(t, 0, &mut ls);
    ls.iter().any(|&(v, _)| v != 0)
}

fn nonzero_leaf_under_a_node(t: &Tree) -> bool {
    let mut ls = Vec::new();
    leaves(t, 0, &mut ls);
    ls.iter().any(|&(v, d)| v != 0 && d >= 1)
}

#[test]
fn the_ideals_fail_and_are_smallest() {
    assert!(any_leaf_nonzero(&Tree::Leaf(1)));
    assert!(!any_leaf_nonzero(&Tree::Leaf(0)));
    assert!(!any_leaf_nonzero(&Tree::Node(vec![])));
    assert!(nonzero_leaf_under_a_node(&Tree::Node(vec![Tree::Leaf(1)])));
    assert!(!nonzero_leaf_under_a_node(&Tree::Leaf(1)));
    assert!(!nonzero_leaf_under_a_node(&Tree::Node(vec![Tree::Leaf(0)])));
    assert!(nonzero_leaf_under_a_node(&Tree::Node(vec![Tree::Node(
        vec![Tree::Leaf(1)]
    )])));
}

#[test]
fn control_every_wrapper_is_hoisted_away() {
    assert_shrinks_to(&Tree::Leaf(1), 30, 200, draw, any_leaf_nonzero);
}

#[test]
fn control_all_but_one_wrapper_is_hoisted_away() {
    assert_shrinks_to(
        &Tree::Node(vec![Tree::Leaf(1)]),
        30,
        200,
        draw,
        nonzero_leaf_under_a_node,
    );
}

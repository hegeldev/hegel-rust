mod common;

use common::utils::{
    assert_all_examples, assert_simple_property, check_can_generate_examples, expect_panic,
    find_any, minimal,
};
use hegel::generators::{self as gs, Generator, RecursiveGenerator};
use hegel::{Hegel, Settings, TestCase};

#[derive(Debug, Clone, PartialEq)]
enum Tree {
    Leaf(i32),
    Branch(Vec<Tree>),
}

impl Tree {
    fn height(&self) -> usize {
        match self {
            Tree::Leaf(_) => 0,
            Tree::Branch(children) => 1 + children.iter().map(Tree::height).max().unwrap_or(0),
        }
    }

    fn leaf_count(&self) -> usize {
        match self {
            Tree::Leaf(_) => 1,
            Tree::Branch(children) => children.iter().map(Tree::leaf_count).sum(),
        }
    }
}

fn trees() -> RecursiveGenerator<Tree> {
    gs::recursive(gs::integers::<i32>().map(Tree::Leaf), |subtrees| {
        gs::vecs(subtrees).max_size(3).map(Tree::Branch)
    })
}

#[derive(Debug, Clone, PartialEq)]
enum BinTree {
    Leaf,
    Branch(Box<BinTree>, Box<BinTree>),
}

impl BinTree {
    fn height(&self) -> usize {
        match self {
            BinTree::Leaf => 0,
            BinTree::Branch(left, right) => 1 + left.height().max(right.height()),
        }
    }

    fn leaf_count(&self) -> usize {
        match self {
            BinTree::Leaf => 1,
            BinTree::Branch(left, right) => left.leaf_count() + right.leaf_count(),
        }
    }
}

fn bin_trees() -> RecursiveGenerator<BinTree> {
    gs::recursive(gs::just(BinTree::Leaf), |subtrees| {
        hegel::tuples!(subtrees.clone(), subtrees)
            .map(|(left, right)| BinTree::Branch(Box::new(left), Box::new(right)))
    })
}

#[test]
fn test_recursive_default() {
    check_can_generate_examples(trees());
}

#[test]
fn test_recursive_max_depth() {
    assert_all_examples(trees().max_depth(3), |t| t.height() <= 3);
}

#[test]
fn test_recursive_max_leaves() {
    assert_all_examples(bin_trees().max_leaves(4), |t| t.leaf_count() <= 4);
}

#[test]
fn test_recursive_max_leaves_with_collection_branches() {
    assert_all_examples(trees().max_leaves(5), |t| t.leaf_count() <= 5);
}

#[test]
fn test_recursive_zero_max_leaves_generates_leafless_values() {
    assert_simple_property(trees().max_leaves(0), |t| t.leaf_count() == 0);
}

#[test]
fn test_recursive_rejects_when_leaves_are_unavoidable_on_zero_budget() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                tc.draw_silent(bin_trees().max_leaves(0));
            })
            .settings(Settings::new().database(None).seed(Some(0)))
            .run();
        },
        "FilterTooMuch",
    );
}

#[test]
fn test_recursive_propagates_rejection_from_the_leaf_generator() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                tc.draw_silent(gs::recursive(
                    gs::just(BinTree::Leaf).filter(|_| false),
                    |subtrees| {
                        hegel::tuples!(subtrees.clone(), subtrees)
                            .map(|(left, right)| BinTree::Branch(Box::new(left), Box::new(right)))
                    },
                ));
            })
            .settings(Settings::new().database(None).seed(Some(0)))
            .run();
        },
        "FilterTooMuch",
    );
}

#[test]
fn test_recursive_in_vec() {
    assert_all_examples(gs::vecs(trees().max_depth(2)).max_size(3), |v| {
        v.iter().all(|t| t.height() <= 2)
    });
}

#[test]
fn test_recursive_generates_branches() {
    find_any(trees(), |t| matches!(t, Tree::Branch(_)));
}

#[test]
fn test_recursive_generates_nested_branches() {
    find_any(trees(), |t| t.height() >= 2);
}

#[test]
fn test_recursive_shrinks_to_a_leaf() {
    assert_eq!(minimal(trees(), |_| true), Tree::Leaf(0));
}

#[test]
fn test_recursive_minimal_branch_is_empty() {
    assert_eq!(
        minimal(trees(), |t| matches!(t, Tree::Branch(_))),
        Tree::Branch(vec![])
    );
}

#[test]
fn test_recursive_minimal_binary_branch_is_two_leaves() {
    assert_eq!(
        minimal(bin_trees(), |t| matches!(t, BinTree::Branch(_, _))),
        BinTree::Branch(Box::new(BinTree::Leaf), Box::new(BinTree::Leaf))
    );
}

#[test]
fn test_recursive_shrinks_a_saturated_budget_to_an_exact_tree() {
    let t = minimal(bin_trees().max_leaves(8), |t| t.leaf_count() >= 8);
    assert_eq!(t.leaf_count(), 8);
    assert_eq!(t.height(), 7);
}

#[test]
fn test_recursive_can_generate_and_shrink_large_trees() {
    let t = minimal(trees(), |t| t.leaf_count() >= 30);
    assert_eq!(t.leaf_count(), 30);
}

#[hegel::test]
fn test_recursive_respects_randomized_bounds(tc: TestCase) {
    let max_depth = tc.draw(gs::integers::<usize>().max_value(5));
    let max_leaves = tc.draw(gs::integers::<usize>().min_value(1).max_value(20));
    let t = tc.draw(bin_trees().max_depth(max_depth).max_leaves(max_leaves));
    assert!(t.height() <= max_depth);
    assert!(t.leaf_count() <= max_leaves);
}

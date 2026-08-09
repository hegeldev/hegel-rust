mod common;

use common::utils::{assert_all_examples, expect_panic, find_any, minimal};
use hegel::generators::{self as gs, Generator};

#[derive(Debug, Clone, PartialEq)]
enum Tree {
    Leaf,
    Branch(Box<Tree>, Box<Tree>),
}

impl Tree {
    fn nodes(&self) -> usize {
        match self {
            Tree::Leaf => 1,
            Tree::Branch(left, right) => 1 + left.nodes() + right.nodes(),
        }
    }

    fn depth(&self) -> usize {
        match self {
            Tree::Leaf => 1,
            Tree::Branch(left, right) => 1 + left.depth().max(right.depth()),
        }
    }
}

fn trees(max_size: u64) -> gs::RecursiveGenerator<'static, Tree> {
    gs::recursive(
        gs::just(Tree::Leaf),
        |inner| {
            hegel::tuples!(inner.clone(), inner)
                .map(|(left, right)| Tree::Branch(Box::new(left), Box::new(right)))
        },
        max_size,
    )
}

#[test]
fn test_recursive_trees_respect_the_size_budget() {
    assert_all_examples(trees(50), |t| t.nodes() <= 100);
}

#[test]
fn test_recursive_generates_single_leaves() {
    find_any(trees(100), |t| *t == Tree::Leaf);
}

#[test]
fn test_recursive_generates_large_trees() {
    find_any(trees(100), |t| t.nodes() >= 50);
}

#[test]
fn test_recursive_minimal_example_is_a_single_leaf() {
    let t = minimal(trees(100), |_| true);
    assert_eq!(t, Tree::Leaf);
}

#[test]
fn test_recursive_minimal_branch_is_two_leaves() {
    let t = minimal(trees(100), |t| matches!(t, Tree::Branch(_, _)));
    assert_eq!(t, Tree::Branch(Box::new(Tree::Leaf), Box::new(Tree::Leaf)));
}

#[test]
fn test_recursive_minimal_deep_tree_is_a_bare_chain() {
    let t = minimal(trees(100), |t| t.depth() >= 4);
    assert_eq!(t.depth(), 4);
    assert!(t.nodes() <= 9, "expected a minimal chain, got {t:?}");
}

#[test]
fn test_recursive_size_budget_of_zero_is_a_usage_error() {
    expect_panic(
        || {
            gs::recursive(gs::just(Tree::Leaf), |inner| inner, 0);
        },
        "recursive: max_size must be at least 1",
    );
}

#[test]
fn test_recursive_with_budget_of_one_only_generates_leaves() {
    assert_all_examples(trees(1), |t| *t == Tree::Leaf);
}

#[test]
fn test_recursive_generator_can_be_drawn_repeatedly_in_one_test_case() {
    hegel::Hegel::new(|tc: hegel::TestCase| {
        let generator = trees(30);
        let first = tc.draw(&generator);
        let second = tc.draw(&generator);
        assert!(first.nodes() <= 60);
        assert!(second.nodes() <= 60);
    })
    .settings(hegel::Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_recursive_generator_works_from_a_cloned_test_case_on_another_thread() {
    hegel::Hegel::new(|tc: hegel::TestCase| {
        let generator = trees(30);
        let generator_worker = generator.clone();
        let tc_worker = tc.clone();
        let handle = std::thread::spawn(move || tc_worker.draw(generator_worker));
        let local = tc.draw(&generator);
        let remote = handle.join().unwrap();
        assert!(local.nodes() <= 60);
        assert!(remote.nodes() <= 60);
    })
    .settings(hegel::Settings::new().test_cases(50).database(None))
    .run();
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Value(i64),
    Negate(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
}

impl Expr {
    fn nodes(&self) -> usize {
        match self {
            Expr::Value(_) => 1,
            Expr::Negate(inner) => 1 + inner.nodes(),
            Expr::Add(left, right) => 1 + left.nodes() + right.nodes(),
        }
    }
}

fn exprs(max_size: u64) -> gs::BoxedGenerator<'static, Expr> {
    gs::recursive(
        gs::integers::<i64>().map(Expr::Value),
        |inner| {
            hegel::one_of!(
                inner.clone().map(|e| Expr::Negate(Box::new(e))),
                hegel::tuples!(inner.clone(), inner)
                    .map(|(l, r)| Expr::Add(Box::new(l), Box::new(r))),
            )
        },
        max_size,
    )
    .boxed()
}

#[test]
fn test_recursive_mixed_arity_extend_generates_all_variants() {
    find_any(exprs(100), |e| matches!(e, Expr::Value(_)));
    find_any(exprs(100), |e| matches!(e, Expr::Negate(_)));
    find_any(exprs(100), |e| matches!(e, Expr::Add(_, _)));
}

#[test]
fn test_recursive_mixed_arity_extend_generates_large_expressions() {
    find_any(exprs(100), |e| e.nodes() >= 40);
}

#[test]
fn test_recursive_mixed_arity_minimal_is_the_simplest_value() {
    let e = minimal(exprs(100), |_| true);
    assert_eq!(e, Expr::Value(0));
}

mod common;

use hegel::TestCase;
use hegel::generators as gs;

#[derive(Debug, Clone, hegel::PrettyPrintable)]
enum BinTree {
    Leaf(),
    Branch(Box<BinTree>, Box<BinTree>),
}

impl BinTree {
    fn size(&self) -> usize {
        match self {
            BinTree::Leaf() => 1,
            BinTree::Branch(left, right) => 1 + left.size() + right.size(),
        }
    }
}

#[hegel::composite]
fn tree(tc: &TestCase) -> BinTree {
    return tc.draw(hegel::one_of!(
        gs::just(BinTree::Leaf()),
        hegel::compose!(|tc| {
            BinTree::Branch(Box::new(tc.draw(tree())), Box::new(tc.draw(tree())))
        }),
    ));
}

#[hegel::test]
fn test_can_generate_tree(tc: TestCase) {
    let t = tc.draw(tree());
    assert!(t.size() >= 1);
}

#[hegel::composite]
fn tree_via_direct_self_reference(tc: &TestCase) -> BinTree {
    tc.draw(hegel::one_of!(
        gs::just(BinTree::Leaf()),
        tree_via_direct_self_reference(),
    ))
}

#[hegel::test]
fn test_composite_can_appear_inside_its_own_definition(tc: TestCase) {
    tc.draw(tree_via_direct_self_reference());
}

#[hegel::composite]
fn bounded_tree(tc: &TestCase, max_size: usize) -> BinTree {
    if max_size <= 1 {
        return BinTree::Leaf();
    }
    tc.draw(hegel::one_of!(
        gs::just(BinTree::Leaf()),
        hegel::compose!(|tc| {
            BinTree::Branch(
                Box::new(tc.draw(bounded_tree(max_size / 2))),
                Box::new(tc.draw(bounded_tree(max_size / 2))),
            )
        }),
    ))
}

#[hegel::test]
fn test_recursive_composite_with_arguments(tc: TestCase) {
    let t = tc.draw(bounded_tree(64));
    assert!(t.size() <= 127);
}

#[hegel::test]
fn test_can_generate_branches(tc: TestCase) {
    let t = tc.draw(tree());
    tc.assume(matches!(t, BinTree::Branch(_, _)));
}

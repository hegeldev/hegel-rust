//! Ported from hypothesis-python/tests/conjecture/test_choice_tree.py
//!
//! Exercises [`hegel::__native_test_internals::ChoiceTree`] and its
//! [`prefix_selection_order`]/[`random_selection_order`] helpers. The
//! `ChoiceTree` powers several shrink passes that need to enumerate
//! choice sequences while remembering which branches are dead.

#![cfg(feature = "native")]

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use hegel::__native_test_internals::{
    ChoiceTree, Chooser, DeadBranch, prefix_selection_order, random_selection_order,
};
use hegel::generators as gs;
use rand::SeedableRng;
use rand::rngs::SmallRng;

use crate::common::utils::assert_all_examples;

fn select(args: &[usize]) -> Box<dyn FnMut(usize, usize) -> Vec<usize>> {
    prefix_selection_order(args)
}

/// Rust equivalent of the Python test helper `exhaust`: run the tree to
/// completion, collecting whatever `f` returns on each successful run.
fn exhaust<T, F>(mut f: F) -> Vec<T>
where
    F: FnMut(&mut Chooser) -> Result<T, DeadBranch>,
{
    let results: RefCell<Vec<T>> = RefCell::new(Vec::new());
    let mut tree = ChoiceTree::new();
    let mut prefix: Vec<usize> = Vec::new();
    while !tree.exhausted() {
        prefix = tree.step(prefix_selection_order(&prefix), |chooser| {
            let v = f(chooser)?;
            results.borrow_mut().push(v);
            Ok(())
        });
    }
    results.into_inner()
}

#[test]
fn test_can_enumerate_a_shallow_set() {
    assert_all_examples(gs::vecs(gs::integers::<i64>()), |ls: &Vec<i64>| {
        let ls = ls.clone();
        let results = exhaust(|chooser| chooser.choose(&ls, |_| true));
        let mut got = results.clone();
        got.sort();
        let mut want = ls.clone();
        want.sort();
        got == want
    });
}

#[test]
fn test_can_enumerate_a_nested_set() {
    let values: Vec<i64> = (0..10).collect();
    let nested = exhaust(|chooser| {
        let i = chooser.choose(&values, |_| true)?;
        let j = chooser.choose(&values, |j| *j > i)?;
        Ok((i, j))
    });
    let mut got = nested;
    got.sort();
    let mut want: Vec<(i64, i64)> = Vec::new();
    for i in 0..10i64 {
        for j in (i + 1)..10 {
            want.push((i, j));
        }
    }
    assert_eq!(got, want);
}

#[test]
fn test_can_enumerate_empty() {
    let empty = exhaust(|_chooser| Ok::<_, DeadBranch>(1_i64));
    assert_eq!(empty, vec![1_i64]);
}

#[test]
fn test_all_filtered_child() {
    let values: Vec<i64> = (0..10).collect();
    let all_filtered = exhaust(|chooser| {
        chooser.choose(&values, |_| false)?;
        Ok::<i64, DeadBranch>(0)
    });
    assert_eq!(all_filtered, Vec::<i64>::new());
}

#[test]
fn test_skips_over_exhausted_children() {
    let results: Rc<RefCell<Vec<(i64, i64)>>> = Rc::new(RefCell::new(Vec::new()));
    let three: Vec<i64> = (0..3).collect();
    let two: Vec<i64> = (0..2).collect();

    let run = |tree: &mut ChoiceTree, prefix: &[usize]| {
        let results = Rc::clone(&results);
        let three = three.clone();
        let two = two.clone();
        tree.step(select(prefix), move |chooser| {
            let x = chooser.choose(&three, |x| *x > 0)?;
            let y = chooser.choose(&two, |_| true)?;
            results.borrow_mut().push((x, y));
            Ok(())
        });
    };

    let mut tree = ChoiceTree::new();
    run(&mut tree, &[1, 0]);
    run(&mut tree, &[1, 1]);
    run(&mut tree, &[0, 0]);

    assert_eq!(*results.borrow(), vec![(1, 0), (1, 1), (2, 0)]);
}

#[test]
fn test_extends_prefix_from_right() {
    let four: Vec<i64> = (0..4).collect();
    let mut tree = ChoiceTree::new();
    let result = tree.step(select(&[]), |chooser| {
        chooser.choose(&four, |_| true)?;
        Ok(())
    });
    assert_eq!(result, vec![3]);
}

#[test]
fn test_starts_from_the_end() {
    let three: Vec<i64> = (0..3).collect();
    let mut tree = ChoiceTree::new();
    let result = tree.step(select(&[]), |chooser| {
        chooser.choose(&three, |_| true)?;
        Ok(())
    });
    assert_eq!(result, vec![2]);
}

#[test]
fn test_skips_over_exhausted_subtree() {
    let ten: Vec<i64> = (0..10).collect();
    let mut tree = ChoiceTree::new();
    let first = {
        let ten = ten.clone();
        tree.step(select(&[8]), move |chooser| {
            chooser.choose(&ten, |_| true)?;
            Ok(())
        })
    };
    assert_eq!(first, vec![8]);
    let second = tree.step(select(&[8]), move |chooser| {
        chooser.choose(&ten, |_| true)?;
        Ok(())
    });
    assert_eq!(second, vec![7]);
}

#[test]
fn test_exhausts_randomly() {
    let ten: Vec<i64> = (0..10).collect();
    let mut tree = ChoiceTree::new();
    let rng = Rc::new(RefCell::new(SmallRng::seed_from_u64(0)));
    let mut seen: HashSet<Vec<usize>> = HashSet::new();
    for _ in 0..10 {
        let ten = ten.clone();
        let prefix = tree.step(random_selection_order(Rc::clone(&rng)), move |chooser| {
            chooser.choose(&ten, |_| true)?;
            Ok(())
        });
        seen.insert(prefix);
    }
    assert_eq!(seen.len(), 10);
    assert!(tree.exhausted());
}

#[test]
fn test_exhausts_randomly_when_filtering() {
    let ten: Vec<i64> = (0..10).collect();
    let mut tree = ChoiceTree::new();
    let rng = Rc::new(RefCell::new(SmallRng::seed_from_u64(0)));
    tree.step(random_selection_order(rng), move |chooser| {
        chooser.choose(&ten, |_| false)?;
        Ok(())
    });
    assert!(tree.exhausted());
}

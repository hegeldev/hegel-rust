//! Ported from hypothesis-python/tests/cover/test_intervalset.py

#![cfg(feature = "native")]

use hegel::__native_test_internals::IntervalSet;
use hegel::generators as gs;
use hegel::{Hegel, Settings, TestCase};
use std::collections::HashSet;

fn build_intervals(mut ints: Vec<u32>) -> Vec<(u32, u32)> {
    if ints.len() % 2 != 0 {
        ints.pop();
    }
    ints.sort();
    let mut pairs = Vec::with_capacity(ints.len() / 2);
    let mut iter = ints.into_iter();
    while let Some(a) = iter.next() {
        let b = iter.next().unwrap();
        pairs.push((a, b));
    }
    pairs
}

fn draw_interval_list(tc: &TestCase, max_codepoint: u32) -> Vec<(u32, u32)> {
    let ints: Vec<u32> =
        tc.draw(gs::vecs(gs::integers::<u32>().min_value(0).max_value(max_codepoint)).unique(true));
    build_intervals(ints)
}

fn draw_intervals(tc: &TestCase, max_codepoint: u32) -> IntervalSet {
    IntervalSet::new(draw_interval_list(tc, max_codepoint))
}

fn intervals_to_set(ints: &[(u32, u32)]) -> HashSet<u32> {
    IntervalSet::new(ints.to_vec()).iter().collect()
}

fn default_settings() -> Settings {
    Settings::new().test_cases(100).database(None)
}

#[test]
fn test_intervals_are_equivalent_to_their_lists() {
    Hegel::new(|tc| {
        let intervals = draw_intervals(&tc, 200);
        let ls: Vec<u32> = intervals.iter().collect();
        assert_eq!(ls.len(), intervals.len());
        for (i, &v) in ls.iter().enumerate() {
            assert_eq!(v, intervals.get(i as isize).unwrap());
        }
        let len = ls.len() as isize;
        for i in 1..(len - 1).max(1) {
            assert_eq!(ls[(len - i) as usize], intervals.get(-i).unwrap());
        }
    })
    .settings(default_settings())
    .run();
}

#[test]
fn test_intervals_match_indexes() {
    Hegel::new(|tc| {
        let intervals = draw_intervals(&tc, 200);
        let ls: Vec<u32> = intervals.iter().collect();
        for &v in &ls {
            let ls_index = ls.iter().position(|&x| x == v).unwrap();
            assert_eq!(ls_index, intervals.index(v).unwrap());
        }
    })
    .settings(default_settings())
    .run();
}

#[test]
fn test_error_for_index_of_not_present_value_examples() {
    let xs = IntervalSet::new(vec![(1, 1)]);
    assert!(!xs.contains(0));
    assert!(xs.index(0).is_none());

    let empty = IntervalSet::new(vec![]);
    assert!(!empty.contains(0));
    assert!(empty.index(0).is_none());
}

#[test]
fn test_error_for_index_of_not_present_value() {
    Hegel::new(|tc| {
        let intervals = draw_intervals(&tc, 0x10FFFF);
        let v: u32 = tc.draw(gs::integers::<u32>().min_value(0).max_value(0x10FFFF));
        tc.assume(!intervals.contains(v));
        assert!(intervals.index(v).is_none());
    })
    .settings(default_settings())
    .run();
}

#[test]
fn test_validates_index() {
    let empty = IntervalSet::new(vec![]);
    assert!(empty.is_empty());
    assert!(empty.get(1).is_none());
    let small = IntervalSet::new(vec![(1, 10)]);
    assert!(!small.is_empty());
    assert!(small.get(11).is_none());
    assert!(small.get(-11).is_none());
}

#[test]
fn test_index_above_is_index_if_present() {
    assert_eq!(IntervalSet::new(vec![(1, 10)]).index_above(1), 0);
    assert_eq!(IntervalSet::new(vec![(1, 10)]).index_above(2), 1);
}

#[test]
fn test_index_above_is_length_if_higher() {
    assert_eq!(IntervalSet::new(vec![(1, 10)]).index_above(100), 10);
}

#[test]
fn test_subtraction_of_intervals_examples() {
    // @example(x=[(0, 1), (3, 3)], y=[(1, 3)])
    check_subtraction(&[(0, 1), (3, 3)], &[(1, 3)]);
    // @example(x=[(0, 1)], y=[(0, 0), (1, 1)])
    check_subtraction(&[(0, 1)], &[(0, 0), (1, 1)]);
    // @example(x=[(0, 1)], y=[(1, 1)])
    check_subtraction(&[(0, 1)], &[(1, 1)]);
}

fn check_subtraction(x: &[(u32, u32)], y: &[(u32, u32)]) {
    let xs = intervals_to_set(x);
    let ys = intervals_to_set(y);
    if xs.is_disjoint(&ys) {
        return;
    }
    let z = IntervalSet::new(x.to_vec())
        .difference(&IntervalSet::new(y.to_vec()))
        .intervals;
    let mut z_sorted = z.clone();
    z_sorted.sort();
    assert_eq!(z, z_sorted);
    for (a, b) in &z {
        assert!(a <= b);
    }
    let diff: HashSet<u32> = xs.difference(&ys).copied().collect();
    assert_eq!(intervals_to_set(&z), diff);
}

#[test]
fn test_subtraction_of_intervals() {
    Hegel::new(|tc| {
        let x = draw_interval_list(&tc, 200);
        let y = draw_interval_list(&tc, 200);
        let xs = intervals_to_set(&x);
        let ys = intervals_to_set(&y);
        tc.assume(!xs.is_disjoint(&ys));
        let z = IntervalSet::new(x.clone())
            .difference(&IntervalSet::new(y.clone()))
            .intervals;
        let mut z_sorted = z.clone();
        z_sorted.sort();
        assert_eq!(z, z_sorted);
        for (a, b) in &z {
            assert!(a <= b);
        }
        let diff: HashSet<u32> = xs.difference(&ys).copied().collect();
        assert_eq!(intervals_to_set(&z), diff);
    })
    .settings(default_settings())
    .run();
}

#[test]
fn test_interval_intersection() {
    Hegel::new(|tc| {
        let x = draw_intervals(&tc, 200);
        let y = draw_intervals(&tc, 200);
        let xs: HashSet<u32> = x.iter().collect();
        let ys: HashSet<u32> = y.iter().collect();
        let inter: HashSet<u32> = x.intersection(&y).iter().collect();
        let expected: HashSet<u32> = xs.intersection(&ys).copied().collect();
        assert_eq!(inter, expected);
    })
    .settings(default_settings())
    .run();
}

#[test]
fn test_char_in_shrink_order() {
    let xs = IntervalSet::new(vec![(0, 256)]);
    assert_eq!(xs.get(xs.idx_of_zero() as isize).unwrap(), '0' as u32);
    assert_eq!(xs.get(xs.idx_of_z()).unwrap(), 'Z' as u32);
    let rewritten: Vec<u32> = (0..256)
        .map(|i| xs.char_in_shrink_order(i) as u32)
        .collect();
    let natural: Vec<u32> = (0..256).collect();
    assert_ne!(rewritten, natural);
    let mut rewritten_sorted = rewritten.clone();
    rewritten_sorted.sort();
    assert_eq!(rewritten_sorted, natural);
}

#[test]
fn test_index_from_char_in_shrink_order() {
    let xs = IntervalSet::new(vec![(0, 256)]);
    for i in xs.iter() {
        let c = xs.char_in_shrink_order(i as usize);
        assert_eq!(xs.index_from_char_in_shrink_order(c), i as usize);
    }
}

#[test]
fn test_intervalset_equal() {
    let xs1 = IntervalSet::new(vec![(0, 256)]);
    let xs2 = IntervalSet::new(vec![(0, 256)]);
    assert_eq!(xs1, xs2);

    let xs3 = IntervalSet::new(vec![(0, 255)]);
    assert_ne!(xs2, xs3);
}

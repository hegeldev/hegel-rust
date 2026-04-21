//! Ported from resources/pbtkit/tests/shrink_quality/test_flatmap.py

use crate::common::utils::{Minimal, minimal};
use hegel::generators::{self as gs, Generator};

#[test]
fn test_can_simplify_flatmap_with_bounded_left_hand_size() {
    let result = minimal(
        gs::booleans().flat_map(|x| gs::vecs(gs::just(x))),
        |x: &Vec<bool>| x.len() >= 10,
    );
    assert_eq!(result, vec![false; 10]);
}

#[test]
fn test_can_simplify_across_flatmap_of_just() {
    let result = minimal(
        gs::integers::<i64>().flat_map(gs::just::<i64>),
        |_: &i64| true,
    );
    assert_eq!(result, 0);
}

#[test]
fn test_can_simplify_on_right_hand_strategy_of_flatmap() {
    let result = minimal(
        gs::integers::<i64>().flat_map(|x| gs::vecs(gs::just(x))),
        |_: &Vec<i64>| true,
    );
    assert_eq!(result, Vec::<i64>::new());
}

#[test]
fn test_can_ignore_left_hand_side_of_flatmap() {
    let result = minimal(
        gs::integers::<i64>().flat_map(|_| gs::vecs(gs::integers::<i64>())),
        |x: &Vec<i64>| x.len() >= 10,
    );
    assert_eq!(result, vec![0i64; 10]);
}

#[test]
fn test_can_simplify_on_both_sides_of_flatmap() {
    let result = minimal(
        gs::integers::<i64>().flat_map(|x| gs::vecs(gs::just(x))),
        |x: &Vec<i64>| x.len() >= 10,
    );
    assert_eq!(result, vec![0i64; 10]);
}

#[test]
fn test_flatmap_rectangles() {
    let result = Minimal::new(
        gs::integers::<i64>()
            .min_value(0)
            .max_value(10)
            .flat_map(|w| {
                gs::vecs(
                    gs::vecs(gs::sampled_from(vec!["a", "b"]))
                        .min_size(w as usize)
                        .max_size(w as usize),
                )
            }),
        |x: &Vec<Vec<&str>>| x.iter().any(|inner| inner.as_slice() == ["a", "b"]),
    )
    .test_cases(2000)
    .run();
    assert_eq!(result, vec![vec!["a", "b"]]);
}

fn shrink_through_a_binding_case(n: usize) {
    let result = minimal(
        gs::integers::<i64>()
            .min_value(0)
            .max_value(100)
            .flat_map(|k| {
                gs::vecs(gs::booleans())
                    .min_size(k as usize)
                    .max_size(k as usize)
            }),
        move |x: &Vec<bool>| x.iter().filter(|&&b| b).count() >= n,
    );
    assert_eq!(result, vec![true; n]);
}

#[test]
fn test_can_shrink_through_a_binding_1() {
    shrink_through_a_binding_case(1);
}

#[test]
fn test_can_shrink_through_a_binding_3() {
    shrink_through_a_binding_case(3);
}

#[test]
fn test_can_shrink_through_a_binding_5() {
    shrink_through_a_binding_case(5);
}

#[test]
fn test_can_shrink_through_a_binding_9() {
    shrink_through_a_binding_case(9);
}

fn delete_in_middle_of_a_binding_case(n: usize) {
    let result = minimal(
        gs::integers::<i64>()
            .min_value(1)
            .max_value(100)
            .flat_map(|k| {
                gs::vecs(gs::booleans())
                    .min_size(k as usize)
                    .max_size(k as usize)
            }),
        move |x: &Vec<bool>| {
            x.len() >= 2
                && x[0]
                && *x.last().unwrap()
                && x.iter().filter(|&&b| !b).count() >= n
        },
    );
    let mut expected = vec![false; n + 2];
    expected[0] = true;
    expected[n + 1] = true;
    assert_eq!(result, expected);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_1() {
    delete_in_middle_of_a_binding_case(1);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_3() {
    delete_in_middle_of_a_binding_case(3);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_5() {
    delete_in_middle_of_a_binding_case(5);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_9() {
    delete_in_middle_of_a_binding_case(9);
}

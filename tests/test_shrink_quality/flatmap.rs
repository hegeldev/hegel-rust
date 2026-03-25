use crate::common::utils::{Minimal, minimal};
use hegel::generators::{self, Generator};

#[test]
fn test_can_simplify_flatmap_with_bounded_left_hand_size() {
    assert_eq!(
        minimal(
            generators::booleans().flat_map(|x| generators::vecs(generators::just(x))),
            |x: &Vec<bool>| x.len() >= 10
        ),
        vec![false; 10]
    );
}

#[test]
fn test_can_simplify_across_flatmap_of_just() {
    assert_eq!(
        minimal(
            generators::integers::<i64>().flat_map(generators::just),
            |_| true
        ),
        0
    );
}

#[test]
fn test_can_simplify_on_right_hand_strategy_of_flatmap() {
    let result: Vec<i64> = minimal(
        generators::integers::<i64>().flat_map(|x| generators::vecs(generators::just(x))),
        |_| true,
    );
    let empty: Vec<i64> = vec![];
    assert_eq!(result, empty);
}

#[test]
fn test_can_ignore_left_hand_side_of_flatmap() {
    assert_eq!(
        minimal(
            generators::integers::<i64>()
                .flat_map(|_| generators::vecs(generators::integers::<i64>())),
            |x: &Vec<i64>| x.len() >= 10
        ),
        vec![0; 10]
    );
}

#[test]
fn test_can_simplify_on_both_sides_of_flatmap() {
    assert_eq!(
        minimal(
            generators::integers::<i64>().flat_map(|x| generators::vecs(generators::just(x))),
            |x: &Vec<i64>| x.len() >= 10
        ),
        vec![0; 10]
    );
}

#[test]
fn test_flatmap_rectangles() {
    let result = Minimal::new(
        generators::integers::<usize>()
            .min_value(0)
            .max_value(10)
            .flat_map(|w| {
                generators::vecs(
                    generators::vecs(generators::sampled_from(vec!['a', 'b']))
                        .min_size(w)
                        .max_size(w),
                )
            }),
        |x: &Vec<Vec<char>>| x.contains(&vec!['a', 'b']),
    )
    .test_cases(2000)
    .run();
    assert_eq!(result, vec![vec!['a', 'b']]);
}

// From nocover/test_flatmap.py

#[test]
fn test_can_shrink_through_a_binding_1() {
    let n = 1;
    let result = minimal(
        generators::integers::<usize>()
            .min_value(0)
            .max_value(100)
            .flat_map(|k| {
                generators::vecs(generators::booleans())
                    .min_size(k)
                    .max_size(k)
            }),
        move |x: &Vec<bool>| x.iter().filter(|&&b| b).count() >= n,
    );
    assert_eq!(result, vec![true; n]);
}

#[test]
fn test_can_shrink_through_a_binding_3() {
    let n = 3;
    let result = minimal(
        generators::integers::<usize>()
            .min_value(0)
            .max_value(100)
            .flat_map(|k| {
                generators::vecs(generators::booleans())
                    .min_size(k)
                    .max_size(k)
            }),
        move |x: &Vec<bool>| x.iter().filter(|&&b| b).count() >= n,
    );
    assert_eq!(result, vec![true; n]);
}

#[test]
fn test_can_shrink_through_a_binding_5() {
    let n = 5;
    let result = minimal(
        generators::integers::<usize>()
            .min_value(0)
            .max_value(100)
            .flat_map(|k| {
                generators::vecs(generators::booleans())
                    .min_size(k)
                    .max_size(k)
            }),
        move |x: &Vec<bool>| x.iter().filter(|&&b| b).count() >= n,
    );
    assert_eq!(result, vec![true; n]);
}

#[test]
fn test_can_shrink_through_a_binding_9() {
    let n = 9;
    let result = minimal(
        generators::integers::<usize>()
            .min_value(0)
            .max_value(100)
            .flat_map(|k| {
                generators::vecs(generators::booleans())
                    .min_size(k)
                    .max_size(k)
            }),
        move |x: &Vec<bool>| x.iter().filter(|&&b| b).count() >= n,
    );
    assert_eq!(result, vec![true; n]);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_1() {
    let n = 1;
    let result = minimal(
        generators::integers::<usize>()
            .min_value(1)
            .max_value(100)
            .flat_map(|k| {
                generators::vecs(generators::booleans())
                    .min_size(k)
                    .max_size(k)
            }),
        move |x: &Vec<bool>| {
            x.len() >= 2
                && *x.first().unwrap()
                && *x.last().unwrap()
                && x.iter().filter(|&&b| !b).count() >= n
        },
    );
    let mut expected = vec![true];
    expected.extend(vec![false; n]);
    expected.push(true);
    assert_eq!(result, expected);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_3() {
    let n = 3;
    let result = minimal(
        generators::integers::<usize>()
            .min_value(1)
            .max_value(100)
            .flat_map(|k| {
                generators::vecs(generators::booleans())
                    .min_size(k)
                    .max_size(k)
            }),
        move |x: &Vec<bool>| {
            x.len() >= 2
                && *x.first().unwrap()
                && *x.last().unwrap()
                && x.iter().filter(|&&b| !b).count() >= n
        },
    );
    let mut expected = vec![true];
    expected.extend(vec![false; n]);
    expected.push(true);
    assert_eq!(result, expected);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_5() {
    let n = 5;
    let result = minimal(
        generators::integers::<usize>()
            .min_value(1)
            .max_value(100)
            .flat_map(|k| {
                generators::vecs(generators::booleans())
                    .min_size(k)
                    .max_size(k)
            }),
        move |x: &Vec<bool>| {
            x.len() >= 2
                && *x.first().unwrap()
                && *x.last().unwrap()
                && x.iter().filter(|&&b| !b).count() >= n
        },
    );
    let mut expected = vec![true];
    expected.extend(vec![false; n]);
    expected.push(true);
    assert_eq!(result, expected);
}

#[test]
fn test_can_delete_in_middle_of_a_binding_9() {
    let n = 9;
    let result = minimal(
        generators::integers::<usize>()
            .min_value(1)
            .max_value(100)
            .flat_map(|k| {
                generators::vecs(generators::booleans())
                    .min_size(k)
                    .max_size(k)
            }),
        move |x: &Vec<bool>| {
            x.len() >= 2
                && *x.first().unwrap()
                && *x.last().unwrap()
                && x.iter().filter(|&&b| !b).count() >= n
        },
    );
    let mut expected = vec![true];
    expected.extend(vec![false; n]);
    expected.push(true);
    assert_eq!(result, expected);
}

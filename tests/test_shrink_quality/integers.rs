use crate::common::utils::{Minimal, minimal};
use hegel::generators::{self, Generator};

#[test]
fn test_integers_from_minimizes_leftwards() {
    assert_eq!(
        minimal(generators::integers::<i64>().min_value(101), |_| true),
        101
    );
}

#[test]
fn test_minimize_bounded_integers_to_zero() {
    assert_eq!(
        minimal(
            generators::integers::<i64>().min_value(-10).max_value(10),
            |_| true
        ),
        0
    );
}

#[test]
fn test_minimize_bounded_integers_to_positive() {
    assert_eq!(
        minimal(
            generators::integers::<i64>()
                .min_value(-10)
                .max_value(10)
                .filter(|&x| x != 0),
            |_| true
        ),
        1
    );
}

#[test]
fn test_minimize_single_element_in_silly_large_int_range() {
    let result = minimal(
        generators::integers::<i64>()
            .min_value(i64::MIN / 2)
            .max_value(i64::MAX / 2),
        |&x| x >= i64::MIN / 4,
    );
    assert_eq!(result, 0);
}

#[test]
fn test_minimize_multiple_elements_in_silly_large_int_range() {
    let result = Minimal::new(
        generators::vecs(
            generators::integers::<i64>()
                .min_value(i64::MIN / 2)
                .max_value(i64::MAX / 2),
        ),
        |x: &Vec<i64>| x.len() >= 20,
    )
    .test_cases(10000)
    .run();
    assert_eq!(result, vec![0; 20]);
}

#[hegel::composite]
fn bounded_int_vec(tc: hegel::TestCase) -> Vec<i64> {
    tc.draw(generators::vecs(
        generators::integers::<i64>()
            .min_value(0)
            .max_value(i64::MAX / 2),
    ))
}

#[test]
fn test_minimize_multiple_elements_min_is_not_dupe() {
    let target: Vec<i64> = (0..20).collect();
    let x = Minimal::new(bounded_int_vec(), move |x: &Vec<i64>| {
        x.len() >= 20 && (0..20).all(|i| x[i] >= target[i])
    })
    .test_cases(10000)
    .run();
    assert_eq!(x, (0..20).collect::<Vec<i64>>());
}

#[test]
fn test_can_find_an_int() {
    assert_eq!(minimal(generators::integers::<i64>(), |_| true), 0);
}

#[test]
fn test_can_find_an_int_above_13() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 13), 13);
}

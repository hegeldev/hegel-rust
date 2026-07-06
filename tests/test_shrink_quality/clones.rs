//! Shrink-quality tests for test bodies that clone the `TestCase` and draw
//! on the clones. Each test returns a tuple of the per-handle results and
//! asserts every component is individually minimal.

use crate::common::utils::{minimal, minimal_with};
use hegel::TestCase;
use hegel::generators::{self as gs, Generator};

fn small_int(tc: &TestCase) -> i64 {
    tc.draw(gs::integers::<i64>().min_value(0).max_value(1000))
}

#[test]
fn test_original_and_clone_each_shrink_to_their_threshold() {
    let result = minimal_with(
        |tc| {
            let a = small_int(tc);
            let b = small_int(&tc.clone());
            (a, b)
        },
        |&(a, b): &(i64, i64)| a >= 10 && b >= 10,
    );
    assert_eq!(result, (10, 10));
}

#[test]
fn test_unconstrained_draws_on_original_and_clone_shrink_to_zero() {
    let result = minimal_with(
        |tc| {
            let a = small_int(tc);
            let b = small_int(&tc.clone());
            (a, b)
        },
        |_: &(i64, i64)| true,
    );
    assert_eq!(result, (0, 0));
}

#[test]
fn test_draw_only_on_a_clone_shrinks_to_threshold() {
    let result = minimal_with(|tc| small_int(&tc.clone()), |&x: &i64| x >= 5);
    assert_eq!(result, 5);
}

#[test]
fn test_three_sibling_clones_shrink_independently() {
    let result = minimal_with(
        |tc| {
            (
                small_int(&tc.clone()),
                small_int(&tc.clone()),
                small_int(&tc.clone()),
            )
        },
        |&(a, b, c): &(i64, i64, i64)| a >= 3 && b >= 7 && c >= 11,
    );
    assert_eq!(result, (3, 7, 11));
}

#[test]
fn test_nested_clones_shrink_independently() {
    let result = minimal_with(
        |tc| {
            let child = tc.clone();
            let grandchild = child.clone();
            (small_int(tc), small_int(&child), small_int(&grandchild))
        },
        |&(a, b, c): &(i64, i64, i64)| a >= 1 && b >= 2 && c >= 3,
    );
    assert_eq!(result, (1, 2, 3));
}

#[test]
fn test_interleaved_draws_on_original_and_clone_shrink_to_thresholds() {
    let result = minimal_with(
        |tc| {
            let clone = tc.clone();
            let a = small_int(tc);
            let b = small_int(&clone);
            let c = small_int(tc);
            let d = small_int(&clone);
            (a, b, c, d)
        },
        |&(a, b, c, d): &(i64, i64, i64, i64)| a >= 10 && b >= 20 && c >= 30 && d >= 40,
    );
    assert_eq!(result, (10, 20, 30, 40));
}

#[test]
fn test_collection_drawn_on_a_clone_shrinks_to_minimal() {
    let result = minimal_with(
        |tc| {
            let n = small_int(tc);
            let v: Vec<i64> = tc
                .clone()
                .draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(100)));
            (n, v)
        },
        |(n, v): &(i64, Vec<i64>)| *n >= 1 && v.len() >= 3,
    );
    assert_eq!(result, (1, vec![0, 0, 0]));
}

#[test]
fn test_flat_map_drawn_on_a_clone_shrinks_through_the_binding() {
    let result = minimal_with(
        |tc| {
            tc.clone().draw(
                gs::integers::<i64>()
                    .min_value(0)
                    .max_value(100)
                    .flat_map(|k| {
                        gs::vecs(gs::booleans())
                            .min_size(k as usize)
                            .max_size(k as usize)
                    }),
            )
        },
        |x: &Vec<bool>| x.iter().filter(|&&b| b).count() >= 3,
    );
    assert_eq!(result, vec![true; 3]);
}

#[test]
fn test_clone_inside_compose_shrinks_each_component() {
    let result = minimal(
        hegel::compose!(|tc| {
            let a = small_int(&tc);
            let b = small_int(&tc.clone());
            (a, b)
        }),
        |&(a, b): &(i64, i64)| a >= 10 && b >= 10,
    );
    assert_eq!(result, (10, 10));
}

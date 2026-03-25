use std::collections::HashSet;

use crate::common::utils::find_any;
use hegel::generators;

#[test]
fn test_can_produce_long_lists() {
    find_any(
        generators::vecs(generators::integers::<i64>()),
        |x: &Vec<i64>| x.len() >= 10,
    );
}

#[test]
fn test_can_produce_short_lists() {
    find_any(
        generators::vecs(generators::integers::<i64>()),
        |x: &Vec<i64>| x.len() <= 10,
    );
}

#[test]
fn test_can_produce_the_same_int_twice() {
    find_any(
        generators::vecs(generators::integers::<i64>()),
        |x: &Vec<i64>| {
            let unique: HashSet<_> = x.iter().collect();
            unique.len() < x.len()
        },
    );
}

#[test]
fn test_sampled_from_large_number_can_mix() {
    let items: Vec<i64> = (0..50).collect();
    find_any(
        generators::vecs(generators::sampled_from(items)).min_size(50),
        |x: &Vec<i64>| {
            let unique: HashSet<_> = x.iter().collect();
            unique.len() >= 25
        },
    );
}

#[test]
fn test_non_empty_subset_of_two_is_usually_large() {
    find_any(
        generators::hashsets(generators::sampled_from(vec![1i64, 2])),
        |x: &HashSet<i64>| x.len() == 2,
    );
}

#[test]
fn test_subset_of_ten_is_sometimes_empty() {
    find_any(
        generators::hashsets(generators::integers::<i64>().min_value(1).max_value(10)),
        |x: &HashSet<i64>| x.is_empty(),
    );
}

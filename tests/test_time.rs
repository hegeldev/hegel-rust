mod common;

use common::utils::assert_all_examples;
use hegel::generators;
use std::time::{Duration, Instant};

#[test]
fn test_durations_default() {
    assert_all_examples(generators::durations(), |d| *d >= Duration::ZERO);
}

#[test]
fn test_durations_bounded() {
    let min = Duration::from_secs(5);
    let max = Duration::from_secs(60);
    assert_all_examples(
        generators::durations().min_value(min).max_value(max),
        move |d| *d >= min && *d <= max,
    );
}

#[test]
fn test_instants_default() {
    let before = Instant::now();
    let max_offset = Duration::from_secs(3600);
    assert_all_examples(generators::instants(), move |i| {
        *i >= before && *i <= Instant::now() + max_offset
    });
}

#[test]
fn test_instants_bounded() {
    let max_offset = Duration::from_secs(10);
    let before = Instant::now();
    assert_all_examples(generators::instants().max_offset(max_offset), move |i| {
        *i >= before && *i <= Instant::now() + max_offset
    });
}

mod common;

use common::utils::assert_all_examples;
use hegel::generators;
use std::time::Duration;

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
fn test_durations_in_vec() {
    let max = Duration::from_secs(60);
    assert_all_examples(
        generators::vecs(generators::durations().max_value(max)).max_size(5),
        move |v| v.iter().all(|d| *d <= max),
    );
}

#[test]
fn test_duration_default_generator() {
    assert_all_examples(generators::default::<Duration>(), |d| *d >= Duration::ZERO);
}

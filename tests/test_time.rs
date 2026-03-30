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

//! Tests for `tc.target()`, the public targeted property-based testing API.

mod common;

use common::utils::expect_panic;
use hegel::generators as gs;
use hegel::{Hegel, Settings};

/// `tc.target_labelled(observation, label)` compiles and runs without panicking.
#[test]
fn test_allowed_inputs_to_target() {
    Hegel::new(|tc| {
        let observation: f64 = tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false));
        let label: String = tc.draw(gs::text());
        tc.target_labelled(observation, label);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

/// `tc.target_labelled(observation, label)` works for a restricted set of labels.
#[test]
fn test_allowed_inputs_to_target_fewer_labels() {
    Hegel::new(|tc| {
        let observation: f64 = tc.draw(gs::floats::<f64>().min_value(1.0).allow_infinity(false));
        let label: &str = tc.draw(gs::sampled_from(vec!["a", "few", "labels"]));
        tc.target_labelled(observation, label);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

/// `tc.target(observation)` works with the empty default label.
#[test]
fn test_target_without_label() {
    Hegel::new(|tc| {
        let observation: f64 = tc.draw(gs::floats::<f64>().min_value(1.0).max_value(10.0));
        tc.target(observation);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

/// Multiple `tc.target_labelled()` calls with different labels all execute without error.
#[test]
fn test_multiple_target_calls() {
    Hegel::new(|tc| {
        let n: usize = tc.draw(gs::integers::<usize>().min_value(1).max_value(20));
        for i in 0..n {
            let observation: f64 =
                tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false));
            tc.target_labelled(observation, i.to_string());
        }
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

/// Stress-test with many distinct target labels.
#[test]
fn test_respects_max_pool_size() {
    Hegel::new(|tc| {
        let observations: Vec<f64> = tc.draw(
            gs::vecs(gs::floats::<f64>().allow_nan(false).allow_infinity(false))
                .min_size(11)
                .max_size(20),
        );
        for (i, obs) in observations.iter().enumerate() {
            tc.target_labelled(*obs, i.to_string());
        }
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

/// Targeting must not call the test body more times than `max_examples`.
#[test]
fn test_max_examples_is_not_exceeded() {
    let m: u64 = 10000;
    for max_examples in [1usize, 5, 25, 99] {
        let mut calls: usize = 0;
        Hegel::new(|tc| {
            calls += 1;
            let n: u64 = tc.draw(gs::integers::<u64>().max_value(m));
            tc.target((n * (m - n)) as f64);
        })
        .settings(
            Settings::new()
                .test_cases(max_examples as u64)
                .database(None),
        )
        .run();
        assert_eq!(calls, max_examples, "max_examples = {max_examples}");
    }
}

/// Targeting with a 2D quadratic score drives the optimizer to (500, 500).
#[test]
fn test_finds_a_local_maximum() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let m: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
                let n: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
                let score = -(((m as i64) - 500).pow(2) + ((n as i64) - 500).pow(2));
                tc.target(score as f64);
                assert!(m != 500 || n != 500);
            })
            .settings(Settings::new().test_cases(200).database(None))
            .run();
        },
        "Property test failed",
    );
}

/// Targeting can drive a sum score to its maximum and trigger an assertion failure.
#[test]
fn test_can_target_a_score_upwards_to_interesting() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let n: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
                let m: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
                let score = n + m;
                tc.target(score as f64);
                assert!(score < 2000);
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

/// Targeting drives the maximum observed sum to 2000 without any assertion failure.
#[test]
fn test_can_target_a_score_upwards_without_failing() {
    let mut max_score: u64 = 0;
    Hegel::new(|tc| {
        let n: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
        let m: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
        let score = n + m;
        tc.target(score as f64);
        if score > max_score {
            max_score = score;
        }
    })
    .settings(Settings::new().test_cases(1000).database(None))
    .run();
    assert_eq!(max_score, 2000);
}

/// When most test cases yield the same score on the first two draws, targeting
/// still drives the third draw to its maximum.
#[test]
fn test_targeting_when_most_do_not_benefit() {
    let big: u64 = 10000;
    expect_panic(
        move || {
            Hegel::new(move |tc| {
                tc.draw(gs::integers::<u64>().max_value(1000));
                tc.draw(gs::integers::<u64>().max_value(1000));
                let score: u64 = tc.draw(gs::integers::<u64>().max_value(big));
                tc.target(score as f64);
                assert!(score < big);
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

/// Targeting with `choice(0)` (always 0) must not produce a negative value.
/// The targeting optimizer checks for `step=-1` on value 0; the guard must fire
/// and return `False` rather than producing a negative choice.
#[test]
fn test_targeting_adjust_avoids_negative_values() {
    Hegel::new(|tc| {
        let n: u64 = tc.draw(gs::integers::<u64>().max_value(0));
        tc.target(n as f64);
    })
    .settings(Settings::new().test_cases(200).database(None))
    .run();
}

/// Targeting can drive a score downwards and find a case where the sum is 0.
#[test]
fn test_can_target_a_score_downwards() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let n: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
                let m: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
                let score = n + m;
                tc.target(-(score as f64));
                assert!(score > 0);
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

/// Sanity check that `#[hegel::test]` accepts the rewritten `tc.target(expr)`
/// form. The rewrite of `tc.target(expr)` to `tc.target_labelled(expr, "expr")`
/// is verified directly by the unit tests in `hegel-macros`.
#[hegel::test(test_cases = 5)]
fn test_target_rewrite_compiles_in_hegel_test(tc: hegel::TestCase) {
    let n: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(100));
    tc.target(n as f64);
    tc.target((n * 2) as f64);
    tc.target_labelled(n as f64, "explicit");
}

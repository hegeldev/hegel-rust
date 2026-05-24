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

/// Targeting on a non-trailing draw whose value changes the number of
/// downstream draws drives the hill-climber through the resize-restart
/// branch of `hill_climb`. When `find_integer` flips `big` from false to
/// true, the body draws five extra integers, and the next outer-loop
/// iteration sees `current_nodes.len() != prev_len` and resets `i` to the
/// new tail. The walk then re-encounters indices that were in the
/// pre-resize `nodes_examined` set, exercising the already-examined skip.
/// Targeting on a non-monotone score (peak at `n=10`) with `n` controlling
/// a downstream loop drives the hill-climber through a sequence whose
/// length changes mid-walk. From a random best near (but not at) the peak,
/// `find_integer` steps `n` toward 10, shrinking or growing the realised
/// choice sequence each commit, which trips the resize-restart at the
/// next outer-loop iteration (and the already-examined skip, since
/// `nodes_examined` from the pre-resize pass stays populated).
#[test]
fn test_targeting_walks_through_choice_count_change() {
    // Score depends on both `n` and the downstream booleans: each `true`
    // boolean contributes +1. Random sampling rarely produces "all
    // booleans true", so hill_climb actually makes progress flipping
    // them, then eventually reaches the integer at the head of the
    // sequence — where `find_integer` grows `n`, the trial pulls extra
    // booleans from the random fallback (some `true`, raising the
    // score), and `current_nodes.len()` changes mid-walk. The next
    // outer iteration trips the resize-restart, and the
    // `nodes_examined` set from the pre-resize pass forces the
    // already-examined skip on the way back down.
    Hegel::new(|tc| {
        let _filler1: bool = tc.draw(gs::booleans());
        let _filler2: bool = tc.draw(gs::booleans());
        let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(20));
        let mut sum: i64 = 0;
        for _ in 0..n {
            if tc.draw(gs::booleans()) {
                sum += 1;
            }
        }
        tc.target(sum as f64);
    })
    .settings(
        Settings::new()
            .test_cases(500)
            .database(None)
            .derandomize(true),
    )
    .run();
}

/// Constant score means every perturbation is a lateral move. A non-trailing
/// boolean whose flip adds five downstream draws is the canonical
/// lateral-grow case — `try_replace`'s `!strict && grew` guard rejects it.
/// A perturbation that drives the climbed integer onto an `assume()`-
/// excluded value comes back from the runner with `Status::Invalid`;
/// `try_replace` rejects it via its `trial.status < Status::Valid`
/// guard rather than spuriously recording it as a step.
#[test]
fn test_targeting_rejects_perturbation_that_fails_assume() {
    Hegel::new(|tc| {
        let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(20));
        tc.assume(n != 7);
        // Peak at n=7, but n=7 is filtered out, so the best Valid sample
        // is n=6 or n=8 (score = -1). Hill-climb walks toward n=7,
        // hits the assume(), and `try_replace` rejects the Invalid trial.
        tc.target(-((n - 7).saturating_abs() as f64));
    })
    .settings(
        Settings::new()
            .test_cases(500)
            .database(None)
            .derandomize(true),
    )
    .run();
}

#[test]
fn test_targeting_rejects_growing_lateral_move() {
    Hegel::new(|tc| {
        let _filler: bool = tc.draw(gs::booleans());
        let big: bool = tc.draw(gs::booleans());
        if big {
            for _ in 0..5 {
                let _ = tc.draw(gs::booleans());
            }
        }
        tc.target(1.0);
    })
    .settings(
        Settings::new()
            .test_cases(500)
            .database(None)
            .derandomize(true),
    )
    .run();
}

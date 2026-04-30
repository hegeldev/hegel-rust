//! Ported from resources/pbtkit/tests/test_targeting.py
//!
//! All tests require targeting to actively guide generation.
//! The native backend records target observations but does not feed them back
//! to guide the search, so tests that depend on targeting driving generation
//! toward a goal are gated with `#[cfg(not(feature = "native"))]`.
//!
//! Individually-skipped tests:
//! - `test_can_target_a_score_upwards_to_interesting` (stdout check) — the
//!   upstream asserts specific `capsys` stdout lines; ported without the
//!   stdout check (only the panic is asserted).
//! - `test_targeting_when_most_do_not_benefit` (stdout check) — same reason.
//! - `test_can_target_a_score_downwards` (stdout check) — same reason.

#[cfg(not(feature = "native"))]
use crate::common::utils::expect_panic;
use hegel::generators::{self as gs};
use hegel::{Hegel, Settings};

/// Targeting must not call the test body more times than max_examples.
/// Ported from test_max_examples_is_not_exceeded (parametrized 1..100);
/// a representative subset [1, 5, 25, 99] is checked here.
/// Server mode only: the native runner makes extra calls per valid test case
/// for span mutations, so the call count exceeds max_examples in native mode.
#[cfg(not(feature = "native"))]
#[test]
fn test_max_examples_is_not_exceeded() {
    let m: u64 = 10000;
    for max_examples in [1usize, 5, 25, 99] {
        let mut calls: usize = 0;
        Hegel::new(|tc| {
            calls += 1;
            let n: u64 = tc.draw(gs::integers::<u64>().max_value(m));
            tc.target((n * (m - n)) as f64, "");
        })
        .settings(Settings::new().test_cases(max_examples as u64).database(None))
        .run();
        assert_eq!(calls, max_examples, "max_examples = {max_examples}");
    }
}

/// Targeting with a 2D quadratic score drives the optimizer to (500, 500).
/// Ported from test_finds_a_local_maximum (parametrized over 100 seeds).
/// Server mode only: native does not use target observations to guide generation.
#[cfg(not(feature = "native"))]
#[test]
fn test_finds_a_local_maximum() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let m: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
                let n: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
                let score = -(((m as i64) - 500).pow(2) + ((n as i64) - 500).pow(2));
                tc.target(score as f64, "");
                assert!(m != 500 || n != 500);
            })
            .settings(Settings::new().test_cases(200).database(None))
            .run();
        },
        "Property test failed",
    );
}

/// Targeting can drive a sum score to its maximum and trigger an assertion failure.
/// Ported from test_can_target_a_score_upwards_to_interesting (stdout check omitted).
/// Server mode only: native does not use target observations to guide generation.
#[cfg(not(feature = "native"))]
#[test]
fn test_can_target_a_score_upwards_to_interesting() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let n: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
                let m: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
                let score = n + m;
                tc.target(score as f64, "");
                assert!(score < 2000);
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

/// Targeting drives the maximum observed sum to 2000 without any assertion failure.
/// Ported from test_can_target_a_score_upwards_without_failing.
/// Server mode only: native does not use target observations to guide generation.
#[cfg(not(feature = "native"))]
#[test]
fn test_can_target_a_score_upwards_without_failing() {
    let mut max_score: u64 = 0;
    Hegel::new(|tc| {
        let n: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
        let m: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
        let score = n + m;
        tc.target(score as f64, "");
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
/// Ported from test_targeting_when_most_do_not_benefit (stdout check omitted).
/// Server mode only: native does not use target observations to guide generation.
#[cfg(not(feature = "native"))]
#[test]
fn test_targeting_when_most_do_not_benefit() {
    let big: u64 = 10000;
    expect_panic(
        move || {
            Hegel::new(move |tc| {
                tc.draw(gs::integers::<u64>().max_value(1000));
                tc.draw(gs::integers::<u64>().max_value(1000));
                let score: u64 = tc.draw(gs::integers::<u64>().max_value(big));
                tc.target(score as f64, "");
                assert!(score < big);
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

/// Targeting with choice(0) (always 0) must not produce a negative value.
/// The targeting optimizer checks for step=-1 on value 0; the guard must fire
/// and return False rather than producing a negative choice.
#[test]
fn test_targeting_adjust_avoids_negative_values() {
    Hegel::new(|tc| {
        let n: u64 = tc.draw(gs::integers::<u64>().max_value(0));
        tc.target(n as f64, "");
    })
    .settings(Settings::new().test_cases(200).database(None))
    .run();
}

/// Targeting can drive a score downwards and find a case where the sum is 0.
/// Ported from test_can_target_a_score_downwards (stdout check omitted).
/// Server mode only: native does not use target observations to guide generation.
#[cfg(not(feature = "native"))]
#[test]
fn test_can_target_a_score_downwards() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let n: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
                let m: u64 = tc.draw(gs::integers::<u64>().max_value(1000));
                let score = n + m;
                tc.target(-(score as f64), "");
                assert!(score > 0);
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        },
        "Property test failed",
    );
}

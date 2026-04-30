//! Ported from hypothesis-python/tests/cover/test_targeting.py and
//! hypothesis-python/tests/nocover/test_targeting.py
//!
//! Individually-skipped tests:
//! - `test_disallowed_inputs_to_target` — hegel-rust's API is typed (`f64`,
//!   `Into<String>`), so invalid types (non-float score, non-string label) are
//!   compile-time errors rather than runtime `InvalidArgument` exceptions.
//!   NaN and infinity are valid `f64` values in Rust and pass through silently.
//! - `test_cannot_target_outside_test` — hegel-rust has no free
//!   `hegel::target()` function; targeting is only possible via `tc.target()`
//!   inside a test closure, so this case is statically unreachable.
//! - `test_cannot_target_same_label_twice` / `test_cannot_target_default_label_twice`
//!   — hegel-rust silently overwrites duplicate labels rather than raising.
//! - `test_target_returns_value` — `tc.target()` returns `()`, not the score.
//! - `test_reports_target_results` — requires capture of pytest's stdout output
//!   format; no portable Rust counterpart via the public API.
//! - `test_targeting_can_be_disabled` — requires `Phase`-based settings
//!   (`Phase::Target`); hegel-rust has no public `Phase`/`phases` API.

#[cfg(not(feature = "native"))]
use crate::common::utils::expect_panic;
use hegel::generators as gs;
use hegel::{Hegel, Settings};

/// tc.target(observation, label) compiles and runs without panicking.
#[test]
fn test_allowed_inputs_to_target() {
    Hegel::new(|tc| {
        let observation: f64 = tc.draw(
            gs::floats::<f64>()
                .allow_nan(false)
                .allow_infinity(false),
        );
        let label: String = tc.draw(gs::text());
        tc.target(observation, label);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

/// tc.target(observation, label) works for a restricted set of labels.
#[test]
fn test_allowed_inputs_to_target_fewer_labels() {
    Hegel::new(|tc| {
        let observation: f64 = tc.draw(
            gs::floats::<f64>()
                .min_value(1.0)
                .allow_infinity(false),
        );
        let label: &str = tc.draw(gs::sampled_from(vec!["a", "few", "labels"]));
        tc.target(observation, label);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

/// tc.target(observation, "") works with the empty default label.
#[test]
fn test_target_without_label() {
    Hegel::new(|tc| {
        let observation: f64 = tc.draw(
            gs::floats::<f64>()
                .min_value(1.0)
                .max_value(10.0),
        );
        tc.target(observation, "");
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

/// Multiple tc.target() calls with different labels all execute without error.
#[test]
fn test_multiple_target_calls() {
    Hegel::new(|tc| {
        let n: usize = tc.draw(gs::integers::<usize>().min_value(1).max_value(20));
        for i in 0..n {
            let observation: f64 = tc.draw(
                gs::floats::<f64>()
                    .allow_nan(false)
                    .allow_infinity(false),
            );
            tc.target(observation, i.to_string());
        }
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

/// Stress-test with many distinct target labels (mirrors test_respects_max_pool_size).
#[test]
fn test_respects_max_pool_size() {
    Hegel::new(|tc| {
        let observations: Vec<f64> = tc.draw(
            gs::vecs(gs::floats::<f64>().allow_nan(false).allow_infinity(false))
                .min_size(11)
                .max_size(20),
        );
        for (i, obs) in observations.iter().enumerate() {
            tc.target(*obs, i.to_string());
        }
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

/// Targeting can drive a sum score toward its maximum and find a counterexample.
/// Ported from pbtkit/tests/test_targeting.py::test_can_target_a_score_upwards_to_interesting.
/// Only meaningful in server mode where target observations actively guide generation.
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

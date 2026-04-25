//! Ported from hypothesis-python/tests/nocover/test_conjecture_utils.py
//!
//! All three tests exercise `hypothesis.internal.conjecture.utils`, which
//! has no public-API counterpart in hegel-rust. The native equivalents
//! live under `__native_test_internals` (`Sampler`, `calc_p_continue`,
//! `p_continue_to_avg`), so the file is native-gated.
//!
//! `Sampler` is a Vose's-alias-method port currently stubbed with
//! `todo!()` — `test_sampler_matches_distribution` will hit the runtime
//! panic until the alias-method routine is filled in.

#![cfg(feature = "native")]

use std::collections::HashMap;

use hegel::__native_test_internals::{
    NativeTestCase, Sampler, calc_p_continue, p_continue_to_avg,
};
use hegel::{Hegel, Settings, TestCase};
use hegel::generators::{self as gs, Generator};
use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};

/// `BUFFER_SIZE // 2` from the upstream `@given(st.floats(0, BUFFER_SIZE // 2), …)`
/// bound. `BUFFER_SIZE` is `8 * 1024` in both engines.
const HALF_BUFFER: f64 = (8 * 1024 / 2) as f64;
const HALF_BUFFER_INT: i64 = 8 * 1024 / 2;

fn check_p_continue(average_size: f64, max_size: f64) {
    if average_size > max_size {
        // mirrors the upstream `assume(average_size <= max_size)`.
        return;
    }
    let p = calc_p_continue(average_size, max_size);
    assert!((0.0..=1.0).contains(&p), "p={p} outside [0, 1]");
    assert!(0.0 < p || average_size < 1e-5, "p={p} but avg={average_size}");
    let abs_err = (average_size - p_continue_to_avg(p, max_size)).abs();
    assert!(
        abs_err < 0.01,
        "abs_err={abs_err} for avg={average_size}, max={max_size}, p={p}",
    );
}

#[test]
fn test_p_continue_examples() {
    // `next_up(0.0)` — smallest positive subnormal.
    let smallest_positive = f64::from_bits(1);
    // `sys.float_info.min` — smallest *normal* positive float.
    let float_info_min = f64::MIN_POSITIVE;
    let inf = f64::INFINITY;

    check_p_continue(0.0, 1.0);
    check_p_continue(0.0, inf);
    check_p_continue(smallest_positive, 2.0 * smallest_positive);
    check_p_continue(smallest_positive, 1.0);
    check_p_continue(smallest_positive, inf);
    check_p_continue(float_info_min, 1.0);
    check_p_continue(float_info_min, inf);
    check_p_continue(10.0, 10.0);
    check_p_continue(10.0, inf);
}

#[test]
fn test_p_continue_property() {
    Hegel::new(|tc: TestCase| {
        let avg: f64 = tc.draw(gs::floats::<f64>().min_value(0.0).max_value(HALF_BUFFER));
        let max: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(HALF_BUFFER_INT));
        check_p_continue(avg, max as f64);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

fn check_p_continue_to_average(p_continue: f64, max_size: f64) {
    let average = p_continue_to_avg(p_continue, max_size);
    assert!(
        (0.0..=max_size).contains(&average),
        "average={average} outside [0, {max_size}] for p={p_continue}",
    );
}

#[test]
fn test_p_continue_to_average_examples() {
    // The single upstream `@example(1.1, 10)` case — `p_continue >= 1`
    // short-circuits to `max_size`.
    check_p_continue_to_average(1.1, 10.0);
}

#[test]
fn test_p_continue_to_average_property() {
    Hegel::new(|tc: TestCase| {
        let p_continue: f64 = tc.draw(gs::floats::<f64>().min_value(0.0).max_value(1.0));
        let max_size: i64 =
            tc.draw(gs::integers::<i64>().min_value(0).max_value(HALF_BUFFER_INT));
        check_p_continue_to_average(p_continue, max_size as f64);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

/// Draw a weights vector by drawing integer "buckets" and normalising. The
/// upstream `integer_weights()` strategy generates a `dict[int, float]`
/// whose values drive `Sampler`; we substitute integer-buckets-then-scale
/// so the inputs to `Sampler::new` shrink as integer choices (the native
/// float-shrinker pulls in a separate code path that's tangential to
/// what this test exercises).
fn weights_strategy() -> impl Generator<Vec<f64>> {
    gs::vecs(gs::integers::<u64>().min_value(1).max_value(1000))
        .min_size(1)
        .max_size(20)
        .map(|buckets: Vec<u64>| {
            let total: f64 = buckets.iter().map(|&b| b as f64).sum();
            buckets.iter().map(|&b| b as f64 / total).collect()
        })
}

#[test]
fn test_sampler_matches_distribution() {
    Hegel::new(|tc: TestCase| {
        let weights: Vec<f64> = tc.draw(weights_strategy());
        let seed: u64 = tc.draw(gs::integers::<u64>());

        let sampler = Sampler::new(&weights);
        let mut counter: HashMap<usize, u64> = HashMap::new();
        let mut seed_rng = SmallRng::seed_from_u64(seed);
        for _ in 0..10_000 {
            let inner_seed: u64 = seed_rng.random::<u64>();
            let mut data = NativeTestCase::new_random(SmallRng::seed_from_u64(inner_seed));
            let n = sampler.sample(&mut data).ok().unwrap();
            *counter.entry(n).or_insert(0) += 1;
        }

        let total_w: f64 = weights.iter().sum();
        let total_c: u64 = counter.values().sum();
        let expected: Vec<f64> = weights.iter().map(|w| w / total_w).collect();
        let actual: Vec<f64> = (0..weights.len())
            .map(|i| *counter.get(&i).unwrap_or(&0) as f64 / total_c as f64)
            .collect();
        for (p1, p2) in expected.iter().zip(actual.iter()) {
            assert!(
                (p1 - p2).abs() < 0.05,
                "expected={expected:?}, actual={actual:?}",
            );
        }
    })
    .settings(Settings::new().test_cases(3).database(None))
    .run();
}

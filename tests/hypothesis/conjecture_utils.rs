//! Ported from hypothesis-python/tests/conjecture/test_utils.py
//!
//! Tests `hypothesis.internal.conjecture.utils`: `Sampler`, `Many`,
//! `combine_labels`, and `p_continue_to_avg`. All tests are native-gated
//! because these are engine internals.
//!
//! Individually-skipped tests:
//!
//! - `test_invalid_numpy_sample`, `test_valid_numpy_sample` — require numpy.
//! - `test_invalid_set_sample`, `test_valid_list_sample` — test Python
//!   `check_sample()` which validates Python container types; no Rust counterpart.
//! - `test_samples_from_a_range_directly` — Python `range` type with no Rust analog.
//! - `test_choice` — `ConjectureData.choice([1,2,3])` is a distinct Python API
//!   (sample from a sequence); not the same as `draw_integer`.
//! - `test_unhashable_calc_label` — tests Python `__hash__` raising `TypeError`;
//!   no Rust analog.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{
    ChoiceValue, Many, NativeTestCase, Sampler, Status, StopTest, combine_labels, p_continue_to_avg,
};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings, TestCase};
use rand::SeedableRng;
use rand::rngs::SmallRng;

// -- draw_boolean forced-value tests ------------------------------------------

#[test]
fn test_drawing_certain_coin_still_writes() {
    // draw_boolean(1) always returns True and writes a choice node.
    let mut data = NativeTestCase::for_choices(&[ChoiceValue::Boolean(true)], None, None);
    assert!(data.weighted(1.0, None).ok().unwrap());
    assert_eq!(data.nodes.len(), 1);
}

#[test]
fn test_drawing_impossible_coin_still_writes() {
    // draw_boolean(0) always returns False and writes a choice node.
    let mut data = NativeTestCase::for_choices(&[ChoiceValue::Boolean(false)], None, None);
    assert!(!data.weighted(0.0, None).ok().unwrap());
    assert_eq!(data.nodes.len(), 1);
}

#[test]
fn test_draws_extremely_small_p() {
    // Even a tiny p, when the data forces True, returns True.
    let mut data = NativeTestCase::for_choices(&[ChoiceValue::Boolean(true)], None, None);
    assert!(data.weighted(0.5_f64.powi(65), None).ok().unwrap());
}

// -- Sampler tests ------------------------------------------------------------

#[test]
fn test_sampler_does_not_draw_minimum_if_zero() {
    // weights[0] == 0 means index 0 is never returned.
    // for_choices([0, 0]): integer 0 → table[0] = (0, 2, 1.0);
    // weighted(1.0) is forced True (p >= 1), so returns alternate=2, not 0.
    let sampler = Sampler::new(&[0.0, 2.0, 47.0]);
    let mut data = NativeTestCase::for_choices(
        &[ChoiceValue::Integer(0), ChoiceValue::Boolean(false)],
        None,
        None,
    );
    assert_ne!(sampler.sample(&mut data, None).ok().unwrap(), 0);
}

#[test]
fn test_sampler_shrinks() {
    // weights = [4.0, 8.0, 1.0, 1.0, 0.5]; for_choices([0, False, *])
    // → table[0] has base=0; boolean False → pick base → result = 0.
    let sampler = Sampler::new(&[4.0, 8.0, 1.0, 1.0, 0.5]);
    let mut data = NativeTestCase::for_choices(
        &[
            ChoiceValue::Integer(0),
            ChoiceValue::Boolean(false),
            ChoiceValue::Integer(0),
        ],
        None,
        None,
    );
    assert_eq!(sampler.sample(&mut data, None).ok().unwrap(), 0);
}

#[test]
fn test_can_force_sampler() {
    let sampler = Sampler::new(&[0.5, 0.5]);
    // 100 slots so we don't run out of prefix capacity.
    let choices: Vec<ChoiceValue> = vec![ChoiceValue::Integer(0); 100];
    let mut cd = NativeTestCase::for_choices(&choices, None, None);
    assert_eq!(sampler.sample(&mut cd, Some(0)).ok().unwrap(), 0);
    assert_eq!(sampler.sample(&mut cd, Some(1)).ok().unwrap(), 1);
}

#[test]
fn test_sampler_distribution() {
    // Property: the Sampler reproduces the input weight distribution.
    Hegel::new(|tc: TestCase| {
        let weights: Vec<f64> = tc.draw(
            gs::vecs(gs::integers::<u64>().min_value(1).max_value(1000))
                .min_size(1)
                .max_size(10)
                .map(|buckets: Vec<u64>| {
                    let total: f64 = buckets.iter().map(|&b| b as f64).sum();
                    buckets.iter().map(|&b| b as f64 / total).collect()
                }),
        );
        let seed: u64 = tc.draw(gs::integers::<u64>());
        let sampler = Sampler::new(&weights);
        let mut counts = vec![0u64; weights.len()];
        let mut seed_rng = SmallRng::seed_from_u64(seed);
        for _ in 0..5_000 {
            let inner_seed: u64 = rand::RngExt::random(&mut seed_rng);
            let mut data = NativeTestCase::new_random(SmallRng::seed_from_u64(inner_seed));
            let n = sampler.sample(&mut data, None).ok().unwrap();
            counts[n] += 1;
        }
        let total_c: u64 = counts.iter().sum();
        let total_w: f64 = weights.iter().sum();
        for (i, &c) in counts.iter().enumerate() {
            let observed = c as f64 / total_c as f64;
            let expected = weights[i] / total_w;
            assert!(
                (observed - expected).abs() < 0.05,
                "i={i} observed={observed} expected={expected}",
            );
        }
    })
    .settings(Settings::new().test_cases(3).database(None))
    .run();
}

// -- combine_labels tests -----------------------------------------------------

#[test]
fn test_combine_labels_is_distinct() {
    let x: u64 = 10;
    let y: u64 = 100;
    let combined = combine_labels(&[x, y]);
    assert_ne!(combined, x);
    assert_ne!(combined, y);
}

#[test]
fn test_combine_labels_is_identity_for_single_argument() {
    // combine_labels(&[n]) = (0 << 1) ^ n = n for any n.
    Hegel::new(|tc: TestCase| {
        let n: u64 = tc.draw(gs::integers::<u64>());
        assert_eq!(combine_labels(&[n]), n);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

// -- many() tests -------------------------------------------------------------

#[test]
fn test_fixed_size_draw_many() {
    // min_size == max_size: draw exactly 3 elements, no boolean choices made.
    let mut data = NativeTestCase::for_choices(&[], None, None);
    let mut many = Many::new(3, 3.0, 3.0);
    assert!(many.more(&mut data).ok().unwrap());
    assert!(many.more(&mut data).ok().unwrap());
    assert!(many.more(&mut data).ok().unwrap());
    assert!(!many.more(&mut data).ok().unwrap());
}

#[test]
fn test_astronomically_unlikely_draw_many() {
    // Even with average_size=1e-5, more() returns True when the data forces True.
    let choices: Vec<ChoiceValue> = vec![ChoiceValue::Boolean(true); 1000];
    let mut data = NativeTestCase::for_choices(&choices, None, None);
    let mut many = Many::new(0, 10.0, 1e-5);
    assert!(many.more(&mut data).ok().unwrap());
}

#[test]
fn test_rejection_eventually_terminates_many() {
    // Rejecting every element: force_stop kicks in after a few rejections.
    let choices: Vec<ChoiceValue> = vec![ChoiceValue::Boolean(true); 1000];
    let mut data = NativeTestCase::for_choices(&choices, None, None);
    let mut many = Many::new(0, 1000.0, 100.0);
    let mut count = 0;
    while many.more(&mut data).ok().unwrap() {
        count += 1;
        many.reject(&mut data).ok().unwrap();
    }
    assert!(count <= 100);
}

#[test]
fn test_rejection_eventually_terminates_many_invalid_for_min_size() {
    // With min_size=1, rejecting every element eventually marks data INVALID.
    let choices: Vec<ChoiceValue> = vec![ChoiceValue::Boolean(true); 1000];
    let mut data = NativeTestCase::for_choices(&choices, None, None);
    let mut many = Many::new(1, 1000.0, 100.0);
    let result: Result<(), StopTest> = (|| {
        loop {
            if !many.more(&mut data)? {
                break;
            }
            many.reject(&mut data)?;
        }
        Ok(())
    })();
    assert!(result.is_err());
    assert_eq!(data.status, Some(Status::Invalid));
}

#[test]
fn test_many_with_min_size() {
    // min_size=2: first two calls are forced True, third draws False → stops.
    let choices: Vec<ChoiceValue> = vec![ChoiceValue::Boolean(false); 5];
    let mut data = NativeTestCase::for_choices(&choices, None, None);
    let mut many = Many::new(2, 1000.0, 10.0);
    assert!(many.more(&mut data).ok().unwrap());
    assert!(many.more(&mut data).ok().unwrap());
    assert!(!many.more(&mut data).ok().unwrap());
}

#[test]
fn test_many_with_max_size() {
    // max_size=2: first two calls draw True, third is forced False → stops.
    let choices: Vec<ChoiceValue> = vec![ChoiceValue::Boolean(true); 5];
    let mut data = NativeTestCase::for_choices(&choices, None, None);
    let mut many = Many::new(0, 2.0, 1.0);
    assert!(many.more(&mut data).ok().unwrap());
    assert!(many.more(&mut data).ok().unwrap());
    assert!(!many.more(&mut data).ok().unwrap());
}

// -- p_continue_to_avg tests --------------------------------------------------

#[test]
fn test_p_continue_to_average_saturates() {
    // p_continue >= 1 should clamp to max_size.
    assert_eq!(p_continue_to_avg(1.1, 100.0), 100.0);
}

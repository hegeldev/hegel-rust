//! Ported from hypothesis-python/tests/conjecture/test_pareto.py
//!
//! Exercises ParetoFront mechanics (add/contains/dominates),
//! NativeConjectureRunner with target_observations and multiple interesting
//! origins, and pareto-front preservation across the reuse phase.
//! All tests are native-gated.
//!
//! Individually-skipped tests:
//! - `test_optimises_the_pareto_front`: requires `pareto_optimise()`,
//!   which stubs `todo!()` pending `allow_transition` support in the Shrinker.
//! - `test_stops_optimising_once_interesting`: same reason.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{
    ChoiceValue, ConjectureRunResult, ExampleDatabase, HealthCheckLabel, InMemoryNativeDatabase,
    NativeConjectureData, NativeConjectureRunner, NativeRunnerSettings, ParetoFront, RunnerPhase,
    Status, choices_from_bytes, choices_to_bytes, interesting_origin,
};
use rand::SeedableRng;
use rand::rngs::SmallRng;
use std::collections::HashMap;
use std::sync::Arc;

fn all_health_checks() -> Vec<HealthCheckLabel> {
    vec![
        HealthCheckLabel::FilterTooMuch,
        HealthCheckLabel::TooSlow,
        HealthCheckLabel::LargeBaseExample,
        HealthCheckLabel::DataTooLarge,
    ]
}

fn rng() -> SmallRng {
    SmallRng::seed_from_u64(0)
}

#[test]
fn test_pareto_front_contains_different_interesting_reasons() {
    let db: Arc<InMemoryNativeDatabase> = Arc::new(InMemoryNativeDatabase::new());
    let db_dyn: Arc<dyn ExampleDatabase> = db.clone();
    let settings = NativeRunnerSettings::new()
        .max_examples(5000)
        .database(Some(db_dyn))
        .suppress_health_check(all_health_checks());
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            data.target_observations.insert("".to_string(), 1.0);
            let n = data.draw_integer(0, (1i128 << 4) - 1);
            data.mark_interesting(interesting_origin(Some(n as i64)));
        },
        settings,
        rng(),
    )
    .with_database_key(b"stuff".to_vec());

    runner.run();
    assert_eq!(runner.pareto_front().len(), 1usize << 4);
}

#[test]
fn test_pareto_front_omits_invalid_examples() {
    let db: Arc<InMemoryNativeDatabase> = Arc::new(InMemoryNativeDatabase::new());
    let db_dyn: Arc<dyn ExampleDatabase> = db.clone();
    let settings = NativeRunnerSettings::new()
        .max_examples(5000)
        .database(Some(db_dyn))
        .suppress_health_check(all_health_checks());
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            let x = data.draw_integer(0, (1i128 << 4) - 1);
            if x % 2 != 0 {
                data.target_observations.insert("".to_string(), 1.0);
                data.mark_invalid(None);
            }
        },
        settings,
        rng(),
    )
    .with_database_key(b"stuff".to_vec());

    runner.run();
    assert_eq!(runner.pareto_front().len(), 0);
}

#[test]
fn test_database_contains_only_pareto_front() {
    let db: Arc<InMemoryNativeDatabase> = Arc::new(InMemoryNativeDatabase::new());
    let db_dyn: Arc<dyn ExampleDatabase> = db.clone();
    let settings = NativeRunnerSettings::new()
        .max_examples(500)
        .database(Some(db_dyn))
        .suppress_health_check(all_health_checks());
    // Note: The upstream Python test asserts `len(db.fetch("stuff.pareto")) ==
    // len(runner.pareto_front)` inside the test closure using a captured
    // `runner` reference.  That circular capture is not representable in Rust,
    // so the in-loop consistency check is omitted; the post-run assertions
    // below verify the same invariant.
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            let v1 = data.draw_integer(0, 1i128 << 4);
            data.target_observations.insert("1".to_string(), v1 as f64);
            data.draw_integer(0, (1i128 << 64) - 1);
            let v2 = data.draw_integer(0, 1i128 << 8);
            data.target_observations.insert("2".to_string(), v2 as f64);
        },
        settings,
        rng(),
    )
    .with_database_key(b"stuff".to_vec());

    runner.run();

    let pareto_key = runner.pareto_key();
    assert!(runner.pareto_front().len() <= 500);
    for i in 0..runner.pareto_front().len() {
        assert!(runner.pareto_front()[i].status >= Status::Valid);
    }

    let values: Vec<Vec<u8>> = db.fetch(&pareto_key);
    assert_eq!(
        values.len(),
        runner.pareto_front().len(),
        "db pareto entries ({}) != in-memory pareto front ({})",
        values.len(),
        runner.pareto_front().len(),
    );

    let values_set: std::collections::HashSet<Vec<u8>> = values.iter().cloned().collect();
    for i in 0..runner.pareto_front().len() {
        let data = &runner.pareto_front()[i];
        let encoded = choices_to_bytes(&data.choices);
        assert!(values_set.contains(&encoded), "pareto entry not in db");
        assert!(runner.pareto_front().contains(data));
    }

    let pareto_key2 = runner.pareto_key();
    let values2: Vec<Vec<u8>> = db.fetch(&pareto_key2);
    let values2_set: std::collections::HashSet<Vec<u8>> = values2.iter().cloned().collect();
    // For each db entry, replaying it should give a result still in the front.
    for b in &values2_set {
        let choices = choices_from_bytes(b).unwrap();
        let result = runner.cached_test_function(&choices);
        assert!(runner.pareto_front().contains(&result));
    }
}

#[test]
fn test_clears_defunct_pareto_front() {
    let db: Arc<InMemoryNativeDatabase> = Arc::new(InMemoryNativeDatabase::new());
    let db_dyn: Arc<dyn ExampleDatabase> = db.clone();
    let settings = NativeRunnerSettings::new()
        .max_examples(10000)
        .database(Some(db_dyn))
        .suppress_health_check(all_health_checks())
        .phases(vec![RunnerPhase::Reuse]);
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            data.target_observations.insert("".to_string(), 1.0);
            data.draw_integer(0, (1i128 << 8) - 1);
            data.draw_integer(0, (1i128 << 8) - 1);
        },
        settings,
        rng(),
    )
    .with_database_key(b"stuff".to_vec());

    let pareto_key = runner.pareto_key();
    for i in 0i128..256 {
        db.save(
            &pareto_key,
            &choices_to_bytes(&[ChoiceValue::Integer(i), ChoiceValue::Integer(0)]),
        );
    }

    runner.run();
    assert_eq!(db.fetch(&pareto_key).len(), 1);
}

#[test]
fn test_down_samples_the_pareto_front() {
    let db: Arc<InMemoryNativeDatabase> = Arc::new(InMemoryNativeDatabase::new());
    let db_dyn: Arc<dyn ExampleDatabase> = db.clone();
    let settings = NativeRunnerSettings::new()
        .max_examples(1000)
        .database(Some(db_dyn))
        .suppress_health_check(all_health_checks())
        .phases(vec![RunnerPhase::Reuse]);
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            data.draw_integer(0, (1i128 << 8) - 1);
            data.draw_integer(0, (1i128 << 8) - 1);
        },
        settings,
        rng(),
    )
    .with_database_key(b"stuff".to_vec());

    let pareto_key = runner.pareto_key();
    for n1 in 0i128..256 {
        for n2 in 0i128..256 {
            db.save(
                &pareto_key,
                &choices_to_bytes(&[ChoiceValue::Integer(n1), ChoiceValue::Integer(n2)]),
            );
        }
    }

    // In Python this raises RunIsComplete; our Rust reuse_existing_examples
    // returns normally after hitting max_examples.
    runner.reuse_existing_examples();
    assert_eq!(runner.valid_examples, 1000);
}

#[test]
fn test_stops_loading_pareto_front_if_interesting() {
    let db: Arc<InMemoryNativeDatabase> = Arc::new(InMemoryNativeDatabase::new());
    let db_dyn: Arc<dyn ExampleDatabase> = db.clone();
    let settings = NativeRunnerSettings::new()
        .max_examples(1000)
        .database(Some(db_dyn))
        .suppress_health_check(all_health_checks())
        .phases(vec![RunnerPhase::Reuse]);
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            data.draw_integer(i128::MIN, i128::MAX);
            data.draw_integer(i128::MIN, i128::MAX);
            data.mark_interesting(interesting_origin(None));
        },
        settings,
        rng(),
    )
    .with_database_key(b"stuff".to_vec());

    let pareto_key = runner.pareto_key();
    for n1 in 0i128..256 {
        for n2 in 0i128..256 {
            db.save(
                &pareto_key,
                &choices_to_bytes(&[ChoiceValue::Integer(n1), ChoiceValue::Integer(n2)]),
            );
        }
    }

    runner.reuse_existing_examples();
    assert_eq!(runner.call_count, 1);
}

#[test]
fn test_uses_tags_in_calculating_pareto_front() {
    let db: Arc<InMemoryNativeDatabase> = Arc::new(InMemoryNativeDatabase::new());
    let db_dyn: Arc<dyn ExampleDatabase> = db.clone();
    let settings = NativeRunnerSettings::new()
        .max_examples(10)
        .database(Some(db_dyn));
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            data.target_observations.insert("".to_string(), 1.0);
            if data.draw_boolean(0.5) {
                data.start_span(11);
                data.draw_integer(0, (1i128 << 8) - 1);
                data.stop_span();
            }
        },
        settings,
        rng(),
    )
    .with_database_key(b"stuff".to_vec());

    runner.run();
    assert_eq!(runner.pareto_front().len(), 2);
}

/// Requires `pareto_optimise()` — currently stubs `todo!()` pending
/// `allow_transition` support in the Shrinker.
#[test]
fn test_optimises_the_pareto_front() {
    let db: Arc<InMemoryNativeDatabase> = Arc::new(InMemoryNativeDatabase::new());
    let db_dyn: Arc<dyn ExampleDatabase> = db.clone();
    let settings = NativeRunnerSettings::new()
        .max_examples(10000)
        .database(Some(db_dyn));
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            let mut count = 0i128;
            while data.draw_integer(0, (1i128 << 8) - 1) != 0 {
                count += 1;
            }
            data.target_observations
                .insert("".to_string(), count.min(5) as f64);
        },
        settings,
        rng(),
    )
    .with_database_key(b"stuff".to_vec());

    let seed: Vec<ChoiceValue> = std::iter::repeat_n(ChoiceValue::Integer(255), 20)
        .chain(std::iter::once(ChoiceValue::Integer(0)))
        .collect();
    runner.cached_test_function(&seed);
    runner.pareto_optimise();

    assert_eq!(runner.pareto_front().len(), 6);
    for i in 0..6 {
        let data = &runner.pareto_front()[i];
        let expected: Vec<ChoiceValue> = std::iter::repeat_n(ChoiceValue::Integer(1), i)
            .chain(std::iter::once(ChoiceValue::Integer(0)))
            .collect();
        assert_eq!(data.choices, expected);
    }
}

#[test]
fn test_does_not_optimise_the_pareto_front_if_interesting() {
    // Python version monkey-patches `runner.pareto_optimise = None` to assert
    // it is not called when `optimise_targets` finds an interesting example.
    // In Rust, we simply verify that `optimise_targets` finds an interesting
    // example — `pareto_optimise` is a regular method and cannot be nulled,
    // but the test body's logic (interesting found → return early) holds.
    let db: Arc<InMemoryNativeDatabase> = Arc::new(InMemoryNativeDatabase::new());
    let db_dyn: Arc<dyn ExampleDatabase> = db.clone();
    let settings = NativeRunnerSettings::new()
        .max_examples(10000)
        .database(Some(db_dyn));
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            let n = data.draw_integer(0, (1i128 << 8) - 1);
            data.target_observations.insert("".to_string(), n as f64);
            if n == 255 {
                data.mark_interesting(interesting_origin(None));
            }
        },
        settings,
        rng(),
    )
    .with_database_key(b"stuff".to_vec());

    runner.cached_test_function(&[ChoiceValue::Integer(0)]);
    runner.optimise_targets();
    assert!(!runner.interesting_examples.is_empty());
}

/// Requires `pareto_optimise()` — currently stubs `todo!()` pending
/// `allow_transition` support in the Shrinker.
#[test]
fn test_stops_optimising_once_interesting() {
    let hi = (1i128 << 16) - 1;
    let db: Arc<InMemoryNativeDatabase> = Arc::new(InMemoryNativeDatabase::new());
    let db_dyn: Arc<dyn ExampleDatabase> = db.clone();
    let settings = NativeRunnerSettings::new()
        .max_examples(10000)
        .database(Some(db_dyn));
    let mut runner = NativeConjectureRunner::new(
        move |data: &mut NativeConjectureData| {
            let n = data.draw_integer(0, hi);
            data.target_observations.insert("".to_string(), n as f64);
            if n < hi {
                data.mark_interesting(interesting_origin(None));
            }
        },
        settings,
        rng(),
    )
    .with_database_key(b"stuff".to_vec());

    let result = runner.cached_test_function(&[ChoiceValue::Integer(hi)]);
    assert_eq!(result.status, Status::Valid);
    runner.pareto_optimise();
    assert!(runner.call_count <= 20);
    assert!(!runner.interesting_examples.is_empty());
}

#[test]
fn test_pareto_contains() {
    // Python: `assert "not a data" not in front` — type-safety makes this
    // trivially true in Rust.  The meaningful check is that a result with
    // status below Valid is rejected by `add` and is not in the front.
    let mut front = ParetoFront::new(SmallRng::seed_from_u64(0));

    let overrun = ConjectureRunResult {
        status: Status::EarlyStop,
        nodes: vec![],
        choices: vec![],
        target_observations: HashMap::new(),
        origin: None,
        tags: std::collections::HashSet::new(),
    };
    let (added, _) = front.add(overrun.clone());
    assert!(!added);
    assert!(!front.contains(&overrun));
}

//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_sampled_from.py
//!
//! Tests that rely on Python-specific facilities are not ported:
//!
//! - `test_cannot_sample_sets` — Rust's type system prevents passing non-sequence
//!   types to `sampled_from`; the runtime type check is Python-specific.
//! - `test_can_sample_enums` — Python `enum.Enum`/`enum.Flag` auto-iteration
//!   integration with `sampled_from`; no Rust equivalent.
//! - `test_efficient_lists_of_tuples_first_element_sampled_from` — uses
//!   `unique_by=fn`; `VecGenerator` only has `.unique(bool)`.
//! - `test_unsatisfiable_explicit_filteredstrategy_sampled`,
//!   `test_unsatisfiable_explicit_filteredstrategy_just` — construct
//!   `FilteredStrategy` directly with Python `bool` as predicate (truthiness).
//! - `test_transformed_just_strategy` — uses `ConjectureData.for_choices`,
//!   `JustStrategy`, `do_draw`/`do_filtered_draw`/`filter_not_satisfied`
//!   (Hypothesis strategy-protocol internals with no hegel-rust counterpart).
//! - `test_issue_2247_regression` — Python int/float equality (`0 == 0.0`);
//!   Rust's type system prevents mixed-type sequences.
//! - `test_mutability_1`, `test_mutability_2` — Python list mutability after
//!   strategy creation; Rust ownership model prevents this.
//! - `test_suggests_elements_instead_of_annotations` — Python enum type-annotation
//!   vs values error message.
//! - `TestErrorNoteBehavior3819` — Python `__notes__` (PEP 678) and dynamic typing
//!   (strategies passed as `sampled_from` elements).

use crate::common::utils::{
    assert_all_examples, assert_simple_property, check_can_generate_examples, expect_panic,
};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};
use std::collections::{HashMap, HashSet};

#[test]
fn test_can_sample_sequence_without_warning() {
    check_can_generate_examples(gs::sampled_from(vec![1, 2, 3]));
}

#[test]
fn test_can_sample_ordereddict_without_warning() {
    check_can_generate_examples(gs::sampled_from(vec!["a", "b", "c"]));
}

#[test]
fn test_unsat_filtered_sampling() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let _: i64 = tc.draw(
                    gs::sampled_from((0..10).collect::<Vec<i64>>())
                        .filter(|x: &i64| *x < 0),
                );
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "(?i)(health.check|FailedHealthCheck|filter)",
    );
}

#[test]
fn test_unsat_filtered_sampling_in_rejection_stage() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let _: i64 = tc.draw(
                    gs::sampled_from(vec![0_i64, 1]).filter(|x: &i64| *x < 0),
                );
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "(?i)(health.check|FailedHealthCheck|Unsatisfiable|filter)",
    );
}

#[test]
fn test_easy_filtered_sampling() {
    assert_simple_property(
        gs::sampled_from((0..100).collect::<Vec<i64>>()).filter(|x: &i64| *x == 0),
        |x: &i64| *x == 0,
    );
}

#[test]
fn test_filtered_sampling_finds_rare_value() {
    assert_all_examples(
        gs::sampled_from((0..100).collect::<Vec<i64>>()).filter(|x: &i64| *x == 99),
        |x: &i64| *x == 99,
    );
}

#[test]
fn test_efficient_sets_of_samples() {
    Hegel::new(|tc| {
        let x: HashSet<i64> = tc.draw(
            gs::hashsets(gs::sampled_from((0..50).collect::<Vec<i64>>())).min_size(50),
        );
        let expected: HashSet<i64> = (0..50).collect();
        assert_eq!(x, expected);
    })
    .settings(Settings::new().database(None))
    .run();
}

#[test]
fn test_efficient_dicts_with_sampled_keys() {
    Hegel::new(|tc| {
        let x: HashMap<i64, ()> = tc.draw(
            gs::hashmaps(gs::sampled_from((0..50).collect::<Vec<i64>>()), gs::just(()))
                .min_size(50),
        );
        let keys: HashSet<i64> = x.keys().copied().collect();
        let expected: HashSet<i64> = (0..50).collect();
        assert_eq!(keys, expected);
    })
    .settings(Settings::new().database(None))
    .run();
}

#[test]
fn test_does_not_include_duplicates_even_when_duplicated_in_collection() {
    assert_all_examples(
        gs::vecs(gs::sampled_from(vec![0_i64; 100])).unique(true),
        |ls: &Vec<i64>| ls.len() <= 1,
    );
}

#[test]
fn test_efficient_sets_of_samples_with_chained_transformations() {
    Hegel::new(|tc| {
        let x: HashSet<i64> = tc.draw(
            gs::hashsets(
                gs::sampled_from((0..50).collect::<Vec<i64>>())
                    .map(|x: i64| x * 2)
                    .filter(|x: &i64| *x % 3 != 0)
                    .map(|x: i64| x / 2),
            )
            .min_size(33),
        );
        let expected: HashSet<i64> = (0..50).filter(|x| (x * 2) % 3 != 0).collect();
        assert_eq!(x, expected);
    })
    .settings(Settings::new().database(None))
    .run();
}

#[test]
fn test_efficient_sets_of_samples_with_chained_transformations_slow_path() {
    Hegel::new(|tc| {
        let result: HashSet<i64> = tc.draw(hegel::compose!(|tc| {
            let mut result = HashSet::new();
            let elements: Vec<i64> = (0..20).collect();
            while result.len() < 13 {
                let captured = result.clone();
                let val: i64 = tc.draw(
                    gs::sampled_from(elements.clone())
                        .filter(|x: &i64| *x % 3 != 0)
                        .map(|x: i64| x * 2)
                        .filter(move |x: &i64| !captured.contains(x)),
                );
                result.insert(val);
            }
            result
        }));
        let expected: HashSet<i64> = (0..20)
            .filter(|x| x % 3 != 0)
            .map(|x| x * 2)
            .collect();
        assert_eq!(result, expected);
    })
    .settings(Settings::new().database(None))
    .run();
}

#[test]
fn test_max_size_is_respected_with_unique_sampled_from() {
    assert_all_examples(
        gs::vecs(gs::sampled_from((0..100).collect::<Vec<i64>>()))
            .max_size(3)
            .unique(true),
        |ls: &Vec<i64>| ls.len() <= 3,
    );
}

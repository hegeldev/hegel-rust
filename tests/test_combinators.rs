mod common;

use common::utils::{assert_all_examples, expect_panic, find_any};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings, TestCase};

#[hegel::test]
fn test_sampled_from_returns_element_from_list(tc: TestCase) {
    let options = tc.draw(gs::vecs(gs::integers::<i32>()).min_size(1));
    let value = tc.draw(gs::sampled_from(options.clone()));
    assert!(options.contains(&value));
}

#[hegel::test]
fn test_sampled_from_strings(tc: TestCase) {
    let options = tc.draw(gs::vecs(gs::text()).min_size(1));
    let value = tc.draw(gs::sampled_from(options.clone()));
    assert!(options.contains(&value));
}

#[test]
fn test_optional_can_generate_some() {
    find_any(gs::optional(gs::integers::<i32>()), |v| v.is_some());
}

#[test]
fn test_optional_can_generate_none() {
    find_any(gs::optional(gs::integers::<i32>()), |v| v.is_none());
}

#[hegel::test]
fn test_optional_respects_inner_generator_bounds(tc: TestCase) {
    let value = tc.draw(gs::optional(gs::integers().min_value(10).max_value(20)));
    if let Some(n) = value {
        assert!((10..=20).contains(&n));
    }
}

#[hegel::test]
fn test_one_of_returns_value_from_one_generator(tc: TestCase) {
    let value = tc.draw(hegel::one_of!(
        gs::integers().min_value(0).max_value(10),
        gs::integers().min_value(100).max_value(110),
    ));
    assert!((0..=10).contains(&value) || (100..=110).contains(&value));
}

#[hegel::test]
fn test_one_of_with_different_types_via_map(tc: TestCase) {
    let value = tc.draw(hegel::one_of!(
        gs::integers::<i32>()
            .min_value(0)
            .max_value(100)
            .map(|n| format!("number: {}", n)),
        gs::text()
            .min_size(1)
            .max_size(10)
            .map(|s| format!("text: {}", s)),
    ));
    assert!(value.starts_with("number: ") || value.starts_with("text: "));
}

#[hegel::test]
fn test_one_of_many(tc: TestCase) {
    let value = tc.draw(gs::one_of((0..10).map(|i| gs::just(i).boxed())));
    assert!((0..10).contains(&value));
}

#[hegel::test]
fn test_flat_map(tc: TestCase) {
    let value = tc.draw(
        gs::integers::<usize>()
            .min_value(1)
            .max_value(5)
            .flat_map(|len| gs::text().min_size(len).max_size(len)),
    );
    assert!(!value.is_empty());
    assert!(value.chars().count() <= 5);
}

#[hegel::test]
fn test_filter(tc: TestCase) {
    let value = tc.draw(
        gs::integers::<i32>()
            .min_value(0)
            .max_value(100)
            .filter(|n| n % 2 == 0),
    );
    assert!(value % 2 == 0);
    assert!((0..=100).contains(&value));
}

#[hegel::test]
fn test_boxed_generator_clone(tc: TestCase) {
    let gen1 = gs::integers::<i32>().min_value(0).max_value(10).boxed();
    let gen2 = gen1.clone();
    let v1 = tc.draw(gen1);
    let v2 = tc.draw(gen2);
    assert!((0..=10).contains(&v1));
    assert!((0..=10).contains(&v2));
}

#[hegel::test]
fn test_boxed_generator_double_boxed(tc: TestCase) {
    // Calling .boxed() on an already-boxed generator should not re-wrap
    let gen1 = gs::integers::<i32>().min_value(0).max_value(10).boxed();
    let gen2 = gen1.boxed();
    let value = tc.draw(gen2);
    assert!((0..=10).contains(&value));
}

#[hegel::test]
fn test_sampled_from_accepts_slice(tc: TestCase) {
    // Pass a borrowed slice directly — no `.to_vec()` or `.iter().collect()` needed.
    const NAMES: &[&str] = &["alice", "bob", "carol"];
    let value = tc.draw(gs::sampled_from(NAMES));
    assert!(NAMES.contains(&value));
}

#[hegel::test]
fn test_sampled_from_accepts_array(tc: TestCase) {
    // Pass a borrowed fixed-size array — coerces to &[T].
    let options = [1i32, 2, 3, 4, 5];
    let value = tc.draw(gs::sampled_from(&options));
    assert!(options.contains(&value));
}

#[hegel::test]
fn test_sampled_from_non_primitive(tc: TestCase) {
    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct Point {
        x: i32,
        y: i32,
    }

    let options = vec![
        Point { x: 1, y: 2 },
        Point { x: 3, y: 4 },
        Point { x: 5, y: 6 },
    ];
    let value = tc.draw(gs::sampled_from(options.clone()));
    assert!(options.contains(&value));
}

#[hegel::test]
fn test_optional_mapped(tc: TestCase) {
    let value = tc.draw(gs::optional(
        gs::integers::<i32>()
            .min_value(0)
            .max_value(100)
            .map(|n| format!("value: {}", n)),
    ));
    if let Some(s) = value {
        assert!(s.starts_with("value: "));
    }
}

#[hegel::test]
fn test_draw_silent_non_debug(tc: TestCase) {
    // Closure is not Debug, so this can only work with draw_silent
    let f = tc.draw_silent(
        gs::integers::<i32>()
            .min_value(0)
            .max_value(1000)
            .map(|n| move |x: i32| x + n),
    );
    assert_eq!(f(10), 10 + f(0));
}

#[test]
fn test_optional_mapped_find_any() {
    find_any(
        gs::optional(gs::integers::<i32>().map(|n| n.wrapping_mul(2))),
        |v| v.is_some(),
    );

    find_any(
        gs::optional(gs::integers::<i32>().map(|n| n.wrapping_mul(2))),
        |v| v.is_none(),
    );
}

// Tests for enumerate_values / filtered sampled_from optimization.

/// A rare value (x == 0) should always be found via the enumerate_values fallback.
#[test]
fn test_sampled_from_filter_rare_value() {
    assert_all_examples(
        gs::sampled_from((0..100_i64).collect::<Vec<i64>>()).filter(|x: &i64| *x == 0),
        |x: &i64| *x == 0,
    );
}

/// A selective filter on sampled_from should only produce values satisfying
/// the predicate, not trigger a FilterTooMuch health check.
#[test]
fn test_sampled_from_filter_produces_only_valid_values() {
    assert_all_examples(
        gs::sampled_from(vec![1_i64, 2, 3, 4, 5]).filter(|x: &i64| *x > 2),
        |x: &i64| *x > 2,
    );
}

/// When all elements are rejected, panic immediately with a clear message
/// rather than triggering FilterTooMuch or silently passing vacuously.
#[test]
fn test_sampled_from_unsatisfiable_filter_panics() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let _: i64 =
                    tc.draw(gs::sampled_from((0..10_i64).collect::<Vec<i64>>()).filter(|x| *x < 0));
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "(?i)(unsatisfiable|filter)",
    );
}

/// Chained .map().filter() on sampled_from should also use enumerate_values.
#[test]
fn test_sampled_from_mapped_then_filtered() {
    assert_all_examples(
        gs::sampled_from(vec![1_i64, 2, 3, 4, 5])
            .map(|x: i64| x * 2)
            .filter(|x: &i64| *x > 4),
        |x: &i64| *x > 4,
    );
}

/// Boxed filtered sampled_from forwards enumerate_values through the box.
#[test]
fn test_sampled_from_filtered_boxed() {
    assert_all_examples(
        gs::sampled_from(vec![1_i64, 2, 3, 4, 5])
            .filter(|x: &i64| *x % 2 == 0)
            .boxed(),
        |x: &i64| *x % 2 == 0,
    );
}

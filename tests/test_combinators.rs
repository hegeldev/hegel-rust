mod common;

use common::utils::find_any;
use hegel::generators::{self, Generate};

#[hegel::test]
fn test_sampled_from_returns_element_from_list() {
    let options = hegel::draw(&generators::vecs(generators::integers::<i32>()).min_size(1));
    let value = hegel::draw(&generators::sampled_from(options.clone()));
    assert!(options.contains(&value));
}

#[hegel::test]
fn test_sampled_from_strings() {
    let options = hegel::draw(&generators::vecs(generators::text()).min_size(1));
    let value = hegel::draw(&generators::sampled_from(options.clone()));
    assert!(options.contains(&value));
}

#[test]
fn test_optional_can_generate_some() {
    find_any(generators::optional(generators::integers::<i32>()), |v| {
        v.is_some()
    });
}

#[test]
fn test_optional_can_generate_none() {
    find_any(generators::optional(generators::integers::<i32>()), |v| {
        v.is_none()
    });
}

#[hegel::test]
fn test_optional_respects_inner_generator_bounds() {
    let value = hegel::draw(&generators::optional(
        generators::integers().min_value(10).max_value(20),
    ));
    if let Some(n) = value {
        assert!((10..=20).contains(&n));
    }
}

#[hegel::test]
fn test_one_of_returns_value_from_one_generator() {
    let value = hegel::draw(&hegel::one_of!(
        generators::integers().min_value(0).max_value(10),
        generators::integers().min_value(100).max_value(110),
    ));
    assert!((0..=10).contains(&value) || (100..=110).contains(&value));
}

#[hegel::test]
fn test_one_of_with_different_types_via_map() {
    let value = hegel::draw(&hegel::one_of!(
        generators::integers::<i32>()
            .min_value(0)
            .max_value(100)
            .map(|n| format!("number: {}", n)),
        generators::text()
            .min_size(1)
            .max_size(10)
            .map(|s| format!("text: {}", s)),
    ));
    assert!(value.starts_with("number: ") || value.starts_with("text: "));
}

#[hegel::test]
fn test_one_of_many() {
    let generators = (0..10).map(|i| generators::just(i).boxed()).collect();
    let value = hegel::draw(&generators::one_of(generators));
    assert!((0..10).contains(&value));
}

#[hegel::test]
fn test_flat_map() {
    let value = hegel::draw(
        &generators::integers::<usize>()
            .min_value(1)
            .max_value(5)
            .flat_map(|len| generators::text().min_size(len).max_size(len)),
    );
    assert!(!value.is_empty());
    assert!(value.chars().count() <= 5);
}

#[hegel::test]
fn test_filter() {
    let value = hegel::draw(
        &generators::integers::<i32>()
            .min_value(0)
            .max_value(100)
            .filter(|n| n % 2 == 0),
    );
    assert!(value % 2 == 0);
    assert!((0..=100).contains(&value));
}

#[hegel::test]
fn test_boxed_generator_clone() {
    let gen1 = generators::integers::<i32>()
        .min_value(0)
        .max_value(10)
        .boxed();
    let gen2 = gen1.clone();
    let v1 = hegel::draw(&gen1);
    let v2 = hegel::draw(&gen2);
    assert!((0..=10).contains(&v1));
    assert!((0..=10).contains(&v2));
}

#[hegel::test]
fn test_boxed_generator_double_boxed() {
    // Calling .boxed() on an already-boxed generator should not re-wrap
    let gen1 = generators::integers::<i32>()
        .min_value(0)
        .max_value(10)
        .boxed();
    let gen2 = gen1.boxed();
    let value = hegel::draw(&gen2);
    assert!((0..=10).contains(&value));
}

#[hegel::test]
fn test_sampled_from_non_primitive() {
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
    let value = hegel::draw(&generators::sampled_from(options.clone()));
    assert!(options.contains(&value));
}

#[hegel::test]
fn test_optional_mapped() {
    let value = hegel::draw(&generators::optional(
        generators::integers::<i32>()
            .min_value(0)
            .max_value(100)
            .map(|n| format!("value: {}", n)),
    ));
    if let Some(s) = value {
        assert!(s.starts_with("value: "));
    }
}

#[test]
fn test_optional_mapped_find_any() {
    find_any(
        generators::optional(generators::integers::<i32>().map(|n| n.wrapping_mul(2))),
        |v| v.is_some(),
    );

    find_any(
        generators::optional(generators::integers::<i32>().map(|n| n.wrapping_mul(2))),
        |v| v.is_none(),
    );
}

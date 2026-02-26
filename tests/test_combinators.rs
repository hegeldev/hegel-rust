mod common;

use common::utils::find_any;
use hegel::generators::{self, Generate};

#[test]
fn test_sampled_from_returns_element_from_list() {
    hegel::hegel(|| {
        let options = hegel::draw(&generators::vecs(generators::integers::<i32>()));
        let value = hegel::draw(&generators::sampled_from(options.clone()));
        assert!(options.contains(&value));
    });
}

#[test]
fn test_sampled_from_strings() {
    hegel::hegel(|| {
        let options = hegel::draw(&generators::vecs(generators::text()));
        let value = hegel::draw(&generators::sampled_from(options.clone()));
        assert!(options.contains(&value));
    });
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

#[test]
fn test_optional_respects_inner_generator_bounds() {
    hegel::hegel(|| {
        let value: Option<i32> = hegel::draw(&generators::optional(
            generators::integers().with_min(10).with_max(20),
        ));
        if let Some(n) = value {
            assert!((10..=20).contains(&n));
        }
    });
}

#[test]
fn test_one_of_returns_value_from_one_generator() {
    hegel::hegel(|| {
        let value: i32 = hegel::draw(&hegel::one_of!(
            generators::integers().with_min(0).with_max(10),
            generators::integers().with_min(100).with_max(110),
        ));
        assert!((0..=10).contains(&value) || (100..=110).contains(&value));
    });
}

#[test]
fn test_one_of_with_different_types_via_map() {
    hegel::hegel(|| {
        let value: String = hegel::draw(&hegel::one_of!(
            generators::integers::<i32>()
                .with_min(0)
                .with_max(100)
                .map(|n| format!("number: {}", n)),
            generators::text()
                .with_min_size(1)
                .with_max_size(10)
                .map(|s| format!("text: {}", s)),
        ));
        assert!(value.starts_with("number: ") || value.starts_with("text: "));
    });
}

#[test]
fn test_one_of_many() {
    hegel::hegel(|| {
        let generators: Vec<_> = (0..10).map(|i| generators::just(i).boxed()).collect();
        let value: i32 = hegel::draw(&generators::one_of(generators));
        assert!((0..10).contains(&value));
    });
}

#[test]
fn test_flat_map() {
    hegel::hegel(|| {
        let value: String = hegel::draw(
            &generators::integers::<usize>()
                .with_min(1)
                .with_max(5)
                .flat_map(|len| generators::text().with_min_size(len).with_max_size(len)),
        );
        assert!(!value.is_empty());
        assert!(value.chars().count() <= 5);
    });
}

#[test]
fn test_filter() {
    hegel::hegel(|| {
        let value: i32 = hegel::draw(
            &generators::integers::<i32>()
                .with_min(0)
                .with_max(100)
                .filter(|n| n % 2 == 0),
        );
        assert!(value % 2 == 0);
        assert!((0..=100).contains(&value));
    });
}

#[test]
fn test_boxed_generator_clone() {
    hegel::hegel(|| {
        let gen1 = generators::integers::<i32>()
            .with_min(0)
            .with_max(10)
            .boxed();
        let gen2 = gen1.clone();
        let v1 = hegel::draw(&gen1);
        let v2 = hegel::draw(&gen2);
        assert!((0..=10).contains(&v1));
        assert!((0..=10).contains(&v2));
    });
}

#[test]
fn test_boxed_generator_double_boxed() {
    hegel::hegel(|| {
        // Calling .boxed() on an already-boxed generator should not re-wrap
        let gen1 = generators::integers::<i32>()
            .with_min(0)
            .with_max(10)
            .boxed();
        let gen2 = gen1.boxed();
        let value = hegel::draw(&gen2);
        assert!((0..=10).contains(&value));
    });
}

#[test]
fn test_sampled_from_non_primitive() {
    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct Point {
        x: i32,
        y: i32,
    }

    hegel::hegel(|| {
        let options = vec![
            Point { x: 1, y: 2 },
            Point { x: 3, y: 4 },
            Point { x: 5, y: 6 },
        ];
        let value = hegel::draw(&generators::sampled_from(options.clone()));
        assert!(options.contains(&value));
    });
}

#[test]
fn test_optional_mapped() {
    hegel::hegel(|| {
        let value: Option<String> = hegel::draw(&generators::optional(
            generators::integers::<i32>()
                .with_min(0)
                .with_max(100)
                .map(|n| format!("value: {}", n)),
        ));
        if let Some(s) = value {
            assert!(s.starts_with("value: "));
        }
    });

    find_any(
        generators::optional(generators::integers::<i32>().map(|n| n.wrapping_mul(2))),
        |v| v.is_some(),
    );

    find_any(
        generators::optional(generators::integers::<i32>().map(|n| n.wrapping_mul(2))),
        |v| v.is_none(),
    );
}

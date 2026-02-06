mod common;

use common::utils::assert_all_examples;
use hegel::gen::{self, Generate};

#[test]
fn test_compose_basic() {
    hegel::hegel(|| {
        let value = hegel::compose!({
            let x = gen::integers::<i32>().with_min(0).with_max(100).generate();
            x
        })
        .generate();
        assert!((0..=100).contains(&value));
    });
}

#[test]
fn test_compose_dependent_generation() {
    hegel::hegel(|| {
        let (x, y) = hegel::compose!({
            let x = gen::integers::<i32>().with_min(0).with_max(50).generate();
            let y = gen::integers::<i32>().with_min(x).with_max(100).generate();
            (x, y)
        })
        .generate();
        assert!(y >= x);
        assert!((0..=50).contains(&x));
        assert!((0..=100).contains(&y));
    });
}

#[test]
fn test_compose_with_map() {
    hegel::hegel(|| {
        let value = hegel::compose!({
            let n = gen::integers::<i32>().with_min(0).with_max(10).generate();
            n
        })
        .map(|n| n * 2)
        .generate();
        assert!(value % 2 == 0);
        assert!((0..=20).contains(&value));
    });
}

#[test]
fn test_compose_with_filter() {
    hegel::hegel(|| {
        let value =
            hegel::compose!({ gen::integers::<i32>().with_min(0).with_max(100).generate() })
                .filter(|n| n % 2 == 0)
                .generate();
        assert!(value % 2 == 0);
    });
}

#[test]
fn test_compose_with_boxed() {
    hegel::hegel(|| {
        let gen =
            hegel::compose!({ gen::integers::<i32>().with_min(0).with_max(50).generate() }).boxed();
        let value = gen.generate();
        assert!((0..=50).contains(&value));
    });
}

#[test]
fn test_compose_assert_all_examples() {
    assert_all_examples(
        hegel::compose!({
            let x = gen::integers::<i32>().with_min(0).with_max(100).generate();
            let y = gen::integers::<i32>().with_min(0).with_max(100).generate();
            (x, y)
        }),
        |&(x, y)| x >= 0 && x <= 100 && y >= 0 && y <= 100,
    );
}

#[test]
fn test_compose_inside_one_of() {
    hegel::hegel(|| {
        let value: i32 = hegel::one_of!(
            hegel::compose!({
                let x = gen::integers::<i32>().with_min(0).with_max(10).generate();
                x
            }),
            gen::integers::<i32>().with_min(100).with_max(110),
        )
        .generate();
        assert!((0..=10).contains(&value) || (100..=110).contains(&value));
    });
}

#[test]
fn test_compose_list_with_index() {
    hegel::hegel(|| {
        let (list, index) = hegel::compose!({
            let list = gen::vecs(gen::integers::<i32>())
                .with_min_size(1)
                .with_max_size(20)
                .generate();
            let index = gen::integers::<usize>()
                .with_min(0)
                .with_max(list.len() - 1)
                .generate();
            (list, index)
        })
        .generate();
        assert!(!list.is_empty());
        assert!(index < list.len());
    });
}

#[test]
fn test_compose_string_building() {
    hegel::hegel(|| {
        let s = hegel::compose!({
            let prefix = gen::sampled_from(vec!["hello", "world"]).generate();
            let n = gen::integers::<i32>().with_min(0).with_max(99).generate();
            format!("{}-{}", prefix, n)
        })
        .generate();
        assert!(s.starts_with("hello-") || s.starts_with("world-"));
    });
}

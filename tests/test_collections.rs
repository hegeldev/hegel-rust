mod common;

use common::project::TempRustProject;
use hegel::TestCase;
use hegel::generators::{self as gs, DefaultGenerator, Generator};
use std::collections::{HashMap, HashSet};

#[derive(Debug, PartialEq, hegel::DefaultGenerator)]
struct Wrapper {
    value: i32,
}

// writing this more nicely requires Eq + Hash on our test structs; but I want to test structs
// which have minimal traits.
fn assert_all_unique<T: PartialEq + std::fmt::Debug>(items: &[T]) {
    for (i, a) in items.iter().enumerate() {
        for b in &items[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[hegel::test]
fn test_vec_with_max_size(tc: TestCase) {
    let max_size: usize = tc.draw(gs::integers());
    let vec: Vec<i32> = tc.draw(gs::vecs(gs::integers::<i32>()).max_size(max_size));
    assert!(vec.len() <= max_size);
}

#[hegel::test]
fn test_vec_with_min_size(tc: TestCase) {
    let min_size: usize = tc.draw(gs::integers().max_value(20));
    let vec: Vec<i32> = tc.draw(gs::vecs(gs::integers::<i32>()).min_size(min_size));
    assert!(vec.len() >= min_size);
}

#[hegel::test]
fn test_vec_with_min_and_max_size(tc: TestCase) {
    let min_size: usize = tc.draw(gs::integers().max_value(10));
    let max_size = tc.draw(gs::integers().min_value(min_size));
    let vec: Vec<i32> = tc.draw(
        gs::vecs(gs::integers::<i32>())
            .min_size(min_size)
            .max_size(max_size),
    );
    assert!(vec.len() >= min_size && vec.len() <= max_size);
}

#[hegel::test]
fn test_vec_unique(tc: TestCase) {
    let max_size: usize = tc.draw(gs::integers());
    let vec: Vec<i32> = tc.draw(
        gs::vecs(gs::integers::<i32>())
            .max_size(max_size)
            .unique(true),
    );

    let set: HashSet<_> = vec.iter().collect();
    assert_eq!(set.len(), vec.len());
}

#[hegel::test]
fn test_vec_unique_with_min_size(tc: TestCase) {
    let min_size: usize = tc.draw(gs::integers().max_value(20));
    let vec: Vec<i32> = tc.draw(
        gs::vecs(gs::integers::<i32>())
            .min_size(min_size)
            .unique(true),
    );

    assert!(vec.len() >= min_size);

    let set: HashSet<_> = vec.iter().collect();
    assert_eq!(set.len(), vec.len());
}

#[hegel::composite]
fn composite_integer(tc: TestCase) -> i32 {
    tc.draw(gs::integers())
}

// explicit regression test for https://github.com/hegeldev/hegel-rust/issues/179
#[hegel::composite]
fn composite_u8(tc: TestCase) -> u8 {
    tc.draw(gs::integers())
}

#[hegel::test]
fn test_vec_unique_composite_u8(tc: TestCase) {
    let vec: Vec<u8> = tc.draw(gs::vecs(composite_u8()).unique(true));
    assert_all_unique(&vec);
}

#[hegel::test]
fn test_vec_unique_composite(tc: TestCase) {
    let max_size: usize = tc.draw(gs::integers());
    let vec: Vec<i32> = tc.draw(
        gs::vecs(composite_integer())
            .max_size(max_size)
            .unique(true),
    );

    let set: HashSet<_> = vec.iter().collect();
    assert_eq!(set.len(), vec.len());
}

#[hegel::test]
fn test_vec_unique_false_after_true(tc: TestCase) {
    // .unique(false) unsets uniqueness. With unique(true), min_size(5) on booleans
    // would be impossible (only 2 distinct values), so this proves it was unset.
    let vec: Vec<bool> = tc.draw(
        gs::vecs(gs::booleans())
            .min_size(5)
            .unique(true)
            .unique(false),
    );
    assert!(vec.len() >= 5);
}

#[hegel::test]
fn test_vec_unique_composite_with_min_size(tc: TestCase) {
    let min_size: usize = tc.draw(gs::integers().max_value(20));
    let vec: Vec<i32> = tc.draw(
        gs::vecs(composite_integer())
            .min_size(min_size)
            .unique(true),
    );

    assert!(vec.len() >= min_size);

    let set: HashSet<_> = vec.iter().collect();
    assert_eq!(set.len(), vec.len());
}

#[hegel::test]
fn test_vec_with_mapped_elements(tc: TestCase) {
    let vec: Vec<i32> = tc.draw(
        gs::vecs(
            gs::integers::<i32>()
                .min_value(i32::MIN / 2)
                .max_value(i32::MAX / 2)
                .map(|x| x * 2),
        )
        .max_size(10),
    );
    assert!(vec.iter().all(|&x| x % 2 == 0));
}

#[hegel::test]
fn test_hashset_with_max_size(tc: TestCase) {
    let max_size: usize = tc.draw(gs::integers());
    let set: HashSet<i32> = tc.draw(gs::hashsets(gs::integers::<i32>()).max_size(max_size));
    assert!(set.len() <= max_size);
}

#[hegel::test]
fn test_hashset_with_min_size(tc: TestCase) {
    let min_size: usize = tc.draw(gs::integers().max_value(20));
    let set: HashSet<i32> = tc.draw(gs::hashsets(gs::integers::<i32>()).min_size(min_size));
    assert!(set.len() >= min_size);
}

#[hegel::test]
fn test_hashset_with_min_and_max_size(tc: TestCase) {
    let min_size: usize = tc.draw(gs::integers().max_value(10));
    let max_size = tc.draw(gs::integers().min_value(min_size));
    let set: HashSet<i32> = tc.draw(
        gs::hashsets(gs::integers::<i32>())
            .min_size(min_size)
            .max_size(max_size),
    );
    assert!(set.len() >= min_size && set.len() <= max_size);
}

#[hegel::test]
fn test_hashset_with_mapped_elements(tc: TestCase) {
    // Exclude i32::MIN to avoid overflow when taking abs
    let set: HashSet<i32> = tc.draw(
        gs::hashsets(
            gs::integers::<i32>()
                .min_value(i32::MIN + 1)
                .map(|x| x.abs()),
        )
        .max_size(10),
    );
    assert!(set.iter().all(|&x| x >= 0));
}

#[hegel::test]
fn test_vec_of_hashsets(tc: TestCase) {
    let vec_of_sets: Vec<HashSet<i32>> = tc.draw(
        gs::vecs(gs::hashsets(gs::integers::<i32>().min_value(0).max_value(100)).max_size(5))
            .max_size(3),
    );
    for set in &vec_of_sets {
        assert!(set.len() <= 5);
        assert!(set.iter().all(|&x| (0..=100).contains(&x)));
    }
}

#[hegel::test]
fn test_hashmap_with_max_size(tc: TestCase) {
    let max_size: usize = tc.draw(gs::integers());
    let map: HashMap<i32, i32> =
        tc.draw(gs::hashmaps(gs::integers::<i32>(), gs::integers::<i32>()).max_size(max_size));
    assert!(map.len() <= max_size);
}

#[hegel::test]
fn test_hashmap_with_min_size(tc: TestCase) {
    let min_size: usize = tc.draw(gs::integers().max_value(20));
    let map: HashMap<i32, i32> =
        tc.draw(gs::hashmaps(gs::integers::<i32>(), gs::integers::<i32>()).min_size(min_size));
    assert!(map.len() >= min_size);
}

#[hegel::test]
fn test_hashmap_with_min_and_max_size(tc: TestCase) {
    let min_size: usize = tc.draw(gs::integers().max_value(10));
    let max_size = tc.draw(gs::integers().min_value(min_size));
    let map: HashMap<i32, i32> = tc.draw(
        gs::hashmaps(gs::integers::<i32>(), gs::integers::<i32>())
            .min_size(min_size)
            .max_size(max_size),
    );
    assert!(map.len() >= min_size && map.len() <= max_size);
}

#[hegel::test]
fn test_hashmap_with_mapped_keys(tc: TestCase) {
    let map: HashMap<i32, i32> = tc.draw(
        gs::hashmaps(
            gs::integers::<i32>()
                .min_value(i32::MIN / 2)
                .max_value(i32::MAX / 2)
                .map(|x| x * 2),
            gs::integers(),
        )
        .max_size(10),
    );
    assert!(map.keys().all(|&k| k % 2 == 0));
}

#[hegel::test]
fn test_binary_with_max_size(tc: TestCase) {
    let data = tc.draw(gs::binary().max_size(50));
    assert!(data.len() <= 50);
}

#[hegel::test]
fn test_vec_unique_partial_eq_struct(tc: TestCase) {
    let vec: Vec<Wrapper> = tc.draw(gs::vecs(Wrapper::default_generator()).unique(true));
    assert_all_unique(&vec);
}

#[hegel::composite]
fn composite_wrapper(tc: TestCase) -> Wrapper {
    Wrapper {
        value: tc.draw(gs::integers()),
    }
}

#[hegel::test]
fn test_vec_unique_partial_eq_struct_composite(tc: TestCase) {
    let vec: Vec<Wrapper> = tc.draw(gs::vecs(composite_wrapper()).unique(true));
    assert_all_unique(&vec);
}

#[test]
fn test_vec_no_partial_eq_compiles_without_unique() {
    #[derive(hegel::DefaultGenerator)]
    struct NoEq {
        #[allow(dead_code)]
        value: i32,
    }
    let _ = gs::vecs(NoEq::default_generator());
}

#[hegel::test]
fn test_vec_non_basic_generator_with_max_size(tc: TestCase) {
    // filter() removes as_basic(), forcing the non-basic Collection path.
    // max_size exercises the map_insert("max_size") branch in ServerBackend::new_collection.
    let vec: Vec<i32> = tc.draw(
        gs::vecs(gs::integers::<i32>().filter(|_| true)).max_size(5),
    );
    assert!(vec.len() <= 5);
}

#[test]
fn test_vec_unique_requires_partial_eq() {
    TempRustProject::new()
        .expect_failure("doesn't satisfy `NoEq: PartialEq`")
        .main_file(
            r#"
use hegel::generators::{self as gs, DefaultGenerator, Generator};

#[derive(hegel::DefaultGenerator)]
struct NoEq { value: i32 }

fn main() {
    let _ = gs::vecs(NoEq::default_generator()).unique(true);
}
"#,
        )
        .cargo_run(&[]);
}

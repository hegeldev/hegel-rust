use hegel::generators::{self, Generator};
use hegel::TestCase;
use std::collections::{HashMap, HashSet};

#[hegel::test]
fn test_vec_with_max_size(tc: TestCase) {
    let max_size: usize = tc.draw(generators::integers().min_value(0).max_value(20));
    let vec: Vec<i32> = tc.draw(generators::vecs(generators::integers::<i32>()).max_size(max_size));
    assert!(vec.len() <= max_size);
}

#[hegel::test]
fn test_vec_with_min_size(tc: TestCase) {
    let min_size: usize = tc.draw(generators::integers().min_value(0).max_value(20));
    let vec: Vec<i32> = tc.draw(generators::vecs(generators::integers::<i32>()).min_size(min_size));
    assert!(vec.len() >= min_size);
}

#[hegel::test]
fn test_vec_with_min_and_max_size(tc: TestCase) {
    let min_size: usize = tc.draw(generators::integers().min_value(0).max_value(10));
    let max_size = tc.draw(generators::integers().min_value(min_size));
    let vec: Vec<i32> = tc.draw(
        generators::vecs(generators::integers::<i32>())
            .min_size(min_size)
            .max_size(max_size),
    );
    assert!(vec.len() >= min_size && vec.len() <= max_size);
}

#[hegel::test]
fn test_vec_unique(tc: TestCase) {
    let max_size: usize = tc.draw(generators::integers().min_value(0).max_value(50));
    let vec: Vec<i32> = tc.draw(
        generators::vecs(generators::integers::<i32>())
            .max_size(max_size)
            .unique(),
    );

    let set: HashSet<_> = vec.iter().collect();
    assert_eq!(set.len(), vec.len());
}

#[hegel::test]
fn test_vec_unique_with_min_size(tc: TestCase) {
    let min_size: usize = tc.draw(generators::integers().min_value(0).max_value(20));
    let vec: Vec<i32> = tc.draw(
        generators::vecs(generators::integers::<i32>())
            .min_size(min_size)
            .unique(),
    );

    assert!(vec.len() >= min_size);

    let set: HashSet<_> = vec.iter().collect();
    assert_eq!(set.len(), vec.len());
}

#[hegel::test]
fn test_vec_with_mapped_elements(tc: TestCase) {
    let vec: Vec<i32> = tc.draw(
        generators::vecs(
            generators::integers::<i32>()
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
    let max_size: usize = tc.draw(generators::integers().min_value(0).max_value(20));
    let set: HashSet<i32> =
        tc.draw(generators::hashsets(generators::integers::<i32>()).max_size(max_size));
    assert!(set.len() <= max_size);
}

#[hegel::test]
fn test_hashset_with_min_size(tc: TestCase) {
    let min_size: usize = tc.draw(generators::integers().min_value(0).max_value(20));
    let set: HashSet<i32> =
        tc.draw(generators::hashsets(generators::integers::<i32>()).min_size(min_size));
    assert!(set.len() >= min_size);
}

#[hegel::test]
fn test_hashset_with_min_and_max_size(tc: TestCase) {
    let min_size: usize = tc.draw(generators::integers().min_value(0).max_value(10));
    let max_size = tc.draw(generators::integers().min_value(min_size));
    let set: HashSet<i32> = tc.draw(
        generators::hashsets(generators::integers::<i32>())
            .min_size(min_size)
            .max_size(max_size),
    );
    assert!(set.len() >= min_size && set.len() <= max_size);
}

#[hegel::test]
fn test_hashset_with_mapped_elements(tc: TestCase) {
    // Exclude i32::MIN to avoid overflow when taking abs
    let set: HashSet<i32> = tc.draw(
        generators::hashsets(
            generators::integers::<i32>()
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
        generators::vecs(
            generators::hashsets(generators::integers::<i32>().min_value(0).max_value(100))
                .max_size(5),
        )
        .max_size(3),
    );
    for set in &vec_of_sets {
        assert!(set.len() <= 5);
        assert!(set.iter().all(|&x| (0..=100).contains(&x)));
    }
}

#[hegel::test]
fn test_hashmap_with_max_size(tc: TestCase) {
    let max_size: usize = tc.draw(generators::integers().min_value(0).max_value(20));
    let map: HashMap<i32, i32> = tc.draw(
        generators::hashmaps(generators::integers::<i32>(), generators::integers::<i32>())
            .max_size(max_size),
    );
    assert!(map.len() <= max_size);
}

#[hegel::test]
fn test_hashmap_with_min_size(tc: TestCase) {
    let min_size: usize = tc.draw(generators::integers().min_value(0).max_value(20));
    let map: HashMap<i32, i32> = tc.draw(
        generators::hashmaps(generators::integers::<i32>(), generators::integers::<i32>())
            .min_size(min_size),
    );
    assert!(map.len() >= min_size);
}

#[hegel::test]
fn test_hashmap_with_min_and_max_size(tc: TestCase) {
    let min_size: usize = tc.draw(generators::integers().min_value(0).max_value(10));
    let max_size = tc.draw(generators::integers().min_value(min_size));
    let map: HashMap<i32, i32> = tc.draw(
        generators::hashmaps(generators::integers::<i32>(), generators::integers::<i32>())
            .min_size(min_size)
            .max_size(max_size),
    );
    assert!(map.len() >= min_size && map.len() <= max_size);
}

#[hegel::test]
fn test_hashmap_with_mapped_keys(tc: TestCase) {
    let map: HashMap<i32, i32> = tc.draw(
        generators::hashmaps(
            generators::integers::<i32>()
                .min_value(i32::MIN / 2)
                .max_value(i32::MAX / 2)
                .map(|x| x * 2),
            generators::integers(),
        )
        .max_size(10),
    );
    assert!(map.keys().all(|&k| k % 2 == 0));
}

#[hegel::test]
fn test_binary_with_max_size(tc: TestCase) {
    let data = tc.draw(generators::binary().max_size(50));
    assert!(data.len() <= 50);
}

use hegel::TestCase;
use hegel::generators::{self, Generator};
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
            .unique(true),
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
            .unique(true),
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

#[hegel::test]
fn test_vec_with_non_basic_elements(tc: TestCase) {
    // flat_map produces a generator without as_basic(), forcing Vec through the
    // Collection-based fallback path (new_collection / collection_more protocol)
    let vec: Vec<String> = tc.draw(
        generators::vecs(
            generators::integers::<usize>()
                .min_value(1)
                .max_value(3)
                .flat_map(|n| generators::text().min_size(n).max_size(n)),
        )
        .min_size(1)
        .max_size(5),
    );
    assert!(!vec.is_empty());
    assert!(vec.len() <= 5);
}

#[hegel::test]
fn test_hashset_with_non_basic_elements(tc: TestCase) {
    // flat_map produces a generator without as_basic(), forcing the HashSet fallback path
    let set: HashSet<String> = tc.draw(
        generators::hashsets(
            generators::integers::<usize>()
                .min_value(1)
                .max_value(3)
                .flat_map(|n| generators::text().min_size(n).max_size(n)),
        )
        .min_size(1)
        .max_size(5),
    );
    assert!(!set.is_empty());
    assert!(set.len() <= 5);
}

#[hegel::test]
fn test_hashmap_with_non_basic_keys(tc: TestCase) {
    // flat_map on keys produces a generator without as_basic(), forcing HashMap fallback
    let map: HashMap<String, i32> = tc.draw(
        generators::hashmaps(
            generators::integers::<usize>()
                .min_value(1)
                .max_value(5)
                .flat_map(|n| generators::text().min_size(n).max_size(n)),
            generators::integers::<i32>(),
        )
        .min_size(1)
        .max_size(5),
    );
    assert!(!map.is_empty());
    assert!(map.len() <= 5);
}

// Non-basic collection tests with small domains to stress min_size enforcement
// and duplicate rejection through the Collection protocol.

#[hegel::test]
fn test_vec_non_basic_min_size_respected(tc: TestCase) {
    // Small domain (0..4) via flat_map to force non-basic path
    let vec: Vec<i32> = tc.draw(
        generators::vecs(
            generators::integers::<i32>()
                .min_value(0)
                .max_value(3)
                .flat_map(|n| generators::integers::<i32>().min_value(n).max_value(n)),
        )
        .min_size(3)
        .max_size(8),
    );
    assert!(
        vec.len() >= 3,
        "min_size 3 not respected: got {}",
        vec.len()
    );
    assert!(vec.len() <= 8);
}

#[hegel::test]
fn test_hashset_non_basic_small_domain_min_size(tc: TestCase) {
    // Elements from domain {0,1,2,3,4} via flat_map, min_size=3
    // This forces many duplicate rejections through collection_reject
    let set: HashSet<i32> = tc.draw(
        generators::hashsets(
            generators::integers::<i32>()
                .min_value(0)
                .max_value(4)
                .flat_map(|n| generators::integers::<i32>().min_value(n).max_value(n)),
        )
        .min_size(3)
        .max_size(5),
    );
    assert!(
        set.len() >= 3,
        "min_size 3 not respected: got {} elements: {:?}",
        set.len(),
        set
    );
    assert!(set.len() <= 5);
    assert!(set.iter().all(|&v| (0..=4).contains(&v)));
}

#[hegel::test]
fn test_hashmap_non_basic_small_domain_min_size(tc: TestCase) {
    // Keys from domain {0,1,2,3,4} via flat_map, min_size=3
    let map: HashMap<i32, bool> = tc.draw(
        generators::hashmaps(
            generators::integers::<i32>()
                .min_value(0)
                .max_value(4)
                .flat_map(|n| generators::integers::<i32>().min_value(n).max_value(n)),
            generators::booleans(),
        )
        .min_size(3)
        .max_size(5),
    );
    assert!(
        map.len() >= 3,
        "min_size 3 not respected: got {} entries: {:?}",
        map.len(),
        map
    );
    assert!(map.len() <= 5);
    assert!(map.keys().all(|&k| (0..=4).contains(&k)));
}

#[hegel::test]
fn test_hashset_non_basic_exact_domain_equals_min_size(tc: TestCase) {
    // Domain has exactly 3 values, min_size=3 — must produce all 3
    let set: HashSet<i32> = tc.draw(
        generators::hashsets(
            generators::integers::<i32>()
                .min_value(0)
                .max_value(2)
                .flat_map(|n| generators::integers::<i32>().min_value(n).max_value(n)),
        )
        .min_size(3),
    );
    assert_eq!(set.len(), 3);
    assert!(set.contains(&0) && set.contains(&1) && set.contains(&2));
}

#[hegel::test]
fn test_fixed_dicts_basic(tc: TestCase) {
    let dict = tc.draw(
        generators::fixed_dicts()
            .field("name", generators::text().min_size(1).max_size(10))
            .field(
                "age",
                generators::integers::<i32>().min_value(0).max_value(120),
            )
            .build(),
    );
    // dict is a ciborium::Value::Map
    if let ciborium::Value::Map(entries) = dict {
        assert_eq!(entries.len(), 2);
    } else {
        panic!("expected Value::Map, got {:?}", dict);
    }
}

#[hegel::test]
fn test_fixed_dicts_with_non_basic_field(tc: TestCase) {
    // Use flat_map to force the FixedDict non-basic fallback path
    let dict = tc.draw(
        generators::fixed_dicts()
            .field(
                "dynamic_text",
                generators::integers::<usize>()
                    .min_value(1)
                    .max_value(3)
                    .flat_map(|n| generators::text().min_size(n).max_size(n)),
            )
            .build(),
    );
    if let ciborium::Value::Map(entries) = dict {
        assert_eq!(entries.len(), 1);
    } else {
        panic!("expected Value::Map, got {:?}", dict);
    }
}

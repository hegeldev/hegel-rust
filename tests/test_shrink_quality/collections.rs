use std::collections::HashSet;

use crate::common::utils::{Minimal, minimal};
use hegel::generators::{self, Generator};

#[test]
fn test_minimize_3_set() {
    let result = minimal(
        generators::hashsets(generators::integers::<i64>()),
        |x: &HashSet<i64>| x.len() >= 3,
    );
    assert!(
        result == HashSet::from([0, 1, 2]) || result == HashSet::from([-1, 0, 1]),
        "Expected {{0, 1, 2}} or {{-1, 0, 1}}, got {:?}",
        result
    );
}

#[test]
fn test_minimize_sets_sampled_from() {
    let items: Vec<i64> = (0..10).collect();
    assert_eq!(
        minimal(
            generators::hashsets(generators::sampled_from(items)).min_size(3),
            |_| true
        ),
        HashSet::from([0, 1, 2])
    );
}

#[test]
fn test_minimize_3_set_of_tuples() {
    let result = minimal(
        generators::hashsets(generators::tuples!(generators::integers::<i64>())),
        |x: &HashSet<(i64,)>| x.len() >= 2,
    );
    assert_eq!(result, HashSet::from([(0,), (1,)]));
}

// Containment tests

#[hegel::composite]
fn vec_and_int(tc: hegel::TestCase) -> (Vec<i64>, i64) {
    let v: Vec<i64> = tc.draw(generators::vecs(generators::integers::<i64>()));
    let i: i64 = tc.draw(generators::integers::<i64>());
    (v, i)
}

#[test]
fn test_containment_0() {
    let n: i64 = 0;
    let result = minimal(vec_and_int(), move |x: &(Vec<i64>, i64)| {
        x.1 >= n && x.0.contains(&x.1)
    });
    assert_eq!(result, (vec![n], n));
}

#[test]
fn test_containment_1() {
    let n: i64 = 1;
    let result = minimal(vec_and_int(), move |x: &(Vec<i64>, i64)| {
        x.1 >= n && x.0.contains(&x.1)
    });
    assert_eq!(result, (vec![n], n));
}

#[test]
fn test_containment_10() {
    let n: i64 = 10;
    let result = minimal(vec_and_int(), move |x: &(Vec<i64>, i64)| {
        x.1 >= n && x.0.contains(&x.1)
    });
    assert_eq!(result, (vec![n], n));
}

#[test]
fn test_containment_100() {
    let n: i64 = 100;
    let result = minimal(vec_and_int(), move |x: &(Vec<i64>, i64)| {
        x.1 >= n && x.0.contains(&x.1)
    });
    assert_eq!(result, (vec![n], n));
}

#[test]
fn test_containment_1000() {
    let n: i64 = 1000;
    let result = minimal(vec_and_int(), move |x: &(Vec<i64>, i64)| {
        x.1 >= n && x.0.contains(&x.1)
    });
    assert_eq!(result, (vec![n], n));
}

#[test]
fn test_duplicate_containment() {
    let (ls, i) = minimal(vec_and_int(), |x: &(Vec<i64>, i64)| {
        x.0.iter().filter(|&&v| v == x.1).count() > 1
    });
    assert_eq!(ls, vec![0, 0]);
    assert_eq!(i, 0);
}

// List ordering and structure tests

#[test]
fn test_reordering_bytes() {
    let ls = minimal(
        generators::vecs(generators::integers::<i64>().min_value(0).max_value(1000)),
        |x: &Vec<i64>| x.iter().sum::<i64>() >= 10 && x.len() >= 3,
    );
    let mut sorted = ls.clone();
    sorted.sort();
    assert_eq!(ls, sorted);
}

#[test]
fn test_minimize_long_list() {
    assert_eq!(
        minimal(
            generators::vecs(generators::booleans()).min_size(50),
            |x: &Vec<bool>| x.len() >= 70
        ),
        vec![false; 70]
    );
}

#[test]
fn test_minimize_list_of_longish_lists() {
    let size = 5;
    let xs = minimal(
        generators::vecs(generators::vecs(generators::booleans())),
        move |x: &Vec<Vec<bool>>| {
            x.iter()
                .filter(|t| t.iter().any(|&b| b) && t.len() >= 2)
                .count()
                >= size
        },
    );
    assert_eq!(xs.len(), size);
    for x in &xs {
        assert_eq!(*x, vec![false, true]);
    }
}

#[test]
fn test_minimize_list_of_fairly_non_unique_ints() {
    let xs = minimal(
        generators::vecs(generators::integers::<i64>()),
        |x: &Vec<i64>| {
            let unique: HashSet<_> = x.iter().collect();
            unique.len() < x.len()
        },
    );
    assert_eq!(xs.len(), 2);
}

#[test]
fn test_list_with_complex_sorting_structure() {
    let xs = minimal(
        generators::vecs(generators::vecs(generators::booleans())),
        |x: &Vec<Vec<bool>>| {
            let reversed: Vec<Vec<bool>> = x
                .iter()
                .map(|t| t.iter().rev().cloned().collect())
                .collect();
            reversed > *x && x.len() > 3
        },
    );
    assert_eq!(xs.len(), 4);
}

#[test]
fn test_list_with_wide_gap() {
    let xs = minimal(
        generators::vecs(generators::integers::<i64>()),
        |x: &Vec<i64>| {
            if x.is_empty() {
                return false;
            }
            let max = *x.iter().max().unwrap();
            let min = *x.iter().min().unwrap();
            min.checked_add(10)
                .is_some_and(|min_plus_10| max > min_plus_10 && min_plus_10 > 0)
        },
    );
    assert_eq!(xs.len(), 2);
    let mut sorted = xs.clone();
    sorted.sort();
    assert_eq!(sorted[1], 11 + sorted[0]);
}

// Lists of collections

#[test]
fn test_minimize_list_of_sets() {
    let result = minimal(
        generators::vecs(generators::hashsets(generators::booleans())),
        |x: &Vec<HashSet<bool>>| x.iter().filter(|s| !s.is_empty()).count() >= 3,
    );
    assert_eq!(result, vec![HashSet::from([false]); 3]);
}

#[test]
fn test_minimize_list_of_lists() {
    let result = minimal(
        generators::vecs(generators::vecs(generators::integers::<i64>())),
        |x: &Vec<Vec<i64>>| x.iter().filter(|s| !s.is_empty()).count() >= 3,
    );
    assert_eq!(result, vec![vec![0]; 3]);
}

#[test]
fn test_minimize_list_of_tuples() {
    let result = minimal(
        generators::vecs(generators::tuples!(
            generators::integers::<i64>(),
            generators::integers::<i64>()
        )),
        |x: &Vec<(i64, i64)>| x.len() >= 2,
    );
    assert_eq!(result, vec![(0, 0), (0, 0)]);
}

// Lists forced near top

#[test]
fn test_lists_forced_near_top_0() {
    let n = 0;
    assert_eq!(
        minimal(
            generators::vecs(generators::integers::<i64>())
                .min_size(n)
                .max_size(n + 2),
            move |t: &Vec<i64>| t.len() == n + 2
        ),
        vec![0i64; n + 2]
    );
}

#[test]
fn test_lists_forced_near_top_1() {
    let n = 1;
    assert_eq!(
        minimal(
            generators::vecs(generators::integers::<i64>())
                .min_size(n)
                .max_size(n + 2),
            move |t: &Vec<i64>| t.len() == n + 2
        ),
        vec![0i64; n + 2]
    );
}

#[test]
fn test_lists_forced_near_top_5() {
    let n = 5;
    assert_eq!(
        minimal(
            generators::vecs(generators::integers::<i64>())
                .min_size(n)
                .max_size(n + 2),
            move |t: &Vec<i64>| t.len() == n + 2
        ),
        vec![0i64; n + 2]
    );
}

#[test]
fn test_lists_forced_near_top_10() {
    let n = 10;
    assert_eq!(
        minimal(
            generators::vecs(generators::integers::<i64>())
                .min_size(n)
                .max_size(n + 2),
            move |t: &Vec<i64>| t.len() == n + 2
        ),
        vec![0i64; n + 2]
    );
}

// Dictionaries

#[test]
fn test_dictionary_minimizes_to_empty() {
    let result: std::collections::HashMap<i64, String> = minimal(
        generators::hashmaps(generators::integers::<i64>(), generators::text()),
        |_| true,
    );
    assert!(result.is_empty());
}

#[test]
fn test_dictionary_minimizes_values() {
    let result = minimal(
        generators::hashmaps(generators::integers::<i64>(), generators::text()),
        |t: &std::collections::HashMap<i64, String>| t.len() >= 3,
    );
    assert!(result.len() >= 3);
    let values: HashSet<_> = result.values().collect();
    assert_eq!(values, HashSet::from([&String::from("")]));
    for &k in result.keys() {
        if k < 0 {
            assert!(result.contains_key(&(k + 1)));
        }
        if k > 0 {
            assert!(result.contains_key(&(k - 1)));
        }
    }
}

#[test]
fn test_minimize_multi_key_dicts() {
    use std::collections::HashMap;
    let result: HashMap<String, bool> = minimal(
        generators::hashmaps(
            generators::booleans().map(|b| b.to_string()),
            generators::booleans(),
        ),
        |x: &HashMap<String, bool>| !x.is_empty(),
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result, HashMap::from([("false".to_string(), false)]));
}

#[test]
fn test_find_large_union_list() {
    let size = 10;
    let result = minimal(
        generators::vecs(generators::hashsets(generators::integers::<i64>()).min_size(1))
            .min_size(1),
        move |x: &Vec<HashSet<i64>>| {
            let union: HashSet<_> = x.iter().flat_map(|s| s.iter().cloned()).collect();
            union.len() >= size
        },
    );
    assert_eq!(result.len(), 1);
    let union: HashSet<_> = result.iter().flat_map(|s| s.iter().cloned()).collect();
    assert_eq!(union.len(), size);
    let max = *union.iter().max().unwrap();
    let min = *union.iter().min().unwrap();
    assert_eq!(max, min + union.len() as i64 - 1);
}

#[test]
fn test_find_dictionary() {
    let smallest: std::collections::HashMap<i64, i64> = minimal(
        generators::hashmaps(generators::integers::<i64>(), generators::integers::<i64>()),
        |xs: &std::collections::HashMap<i64, i64>| xs.iter().any(|(&k, &v)| k > v),
    );
    assert_eq!(smallest.len(), 1);
}

#[test]
fn test_can_find_list() {
    let x = minimal(
        generators::vecs(generators::integers::<i64>()),
        |x: &Vec<i64>| {
            x.iter()
                .try_fold(0i64, |acc, &v| acc.checked_add(v))
                .is_some_and(|s| s >= 10)
        },
    );
    assert_eq!(x.iter().sum::<i64>(), 10);
}

// Collectively minimize

#[test]
fn test_can_collectively_minimize_integers() {
    let n = 10;
    let xs = Minimal::new(
        generators::vecs(generators::integers::<i64>())
            .min_size(n)
            .max_size(n),
        |x: &Vec<i64>| {
            let unique: HashSet<_> = x.iter().collect();
            unique.len() >= 2
        },
    )
    .test_cases(2000)
    .run();
    assert_eq!(xs.len(), n);
    let unique: HashSet<_> = xs.iter().collect();
    assert!(unique.len() >= 2 && unique.len() <= 3);
}

#[test]
fn test_can_collectively_minimize_booleans() {
    let n = 10;
    let xs = Minimal::new(
        generators::vecs(generators::booleans())
            .min_size(n)
            .max_size(n),
        |x: &Vec<bool>| {
            let unique: HashSet<_> = x.iter().collect();
            unique.len() >= 2
        },
    )
    .test_cases(2000)
    .run();
    assert_eq!(xs.len(), n);
    let unique: HashSet<_> = xs.iter().collect();
    assert_eq!(unique.len(), 2);
}

#[test]
fn test_can_collectively_minimize_text() {
    let n = 10;
    let xs = Minimal::new(
        generators::vecs(generators::text()).min_size(n).max_size(n),
        |x: &Vec<String>| {
            let unique: HashSet<_> = x.iter().collect();
            unique.len() >= 2
        },
    )
    .test_cases(2000)
    .run();
    assert_eq!(xs.len(), n);
    let unique: HashSet<_> = xs.iter().collect();
    assert!(unique.len() >= 2 && unique.len() <= 3);
}

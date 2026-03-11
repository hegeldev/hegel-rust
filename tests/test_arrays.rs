mod common;

use common::utils::find_any;
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_array_of_integers() {
    let arr: [i32; 5] = hegel::draw(&generators::arrays(generators::integers::<i32>()));
    assert_eq!(arr.len(), 5);
}

#[hegel::test]
fn test_array_of_booleans() {
    let arr: [bool; 3] = hegel::draw(&generators::arrays(generators::booleans()));
    assert_eq!(arr.len(), 3);
}

#[hegel::test]
fn test_array_of_strings() {
    let arr: [String; 2] = hegel::draw(&generators::arrays(generators::text()));
    assert_eq!(arr.len(), 2);
}

#[hegel::test]
fn test_array_size_zero() {
    let arr: [i32; 0] = hegel::draw(&generators::arrays(generators::integers::<i32>()));
    assert_eq!(arr.len(), 0);
}

#[hegel::test]
fn test_array_size_one() {
    let arr: [i32; 1] = hegel::draw(&generators::arrays(
        generators::integers().min_value(10).max_value(20),
    ));
    assert_eq!(arr.len(), 1);
    assert!((10..=20).contains(&arr[0]));
}

#[hegel::test]
fn test_array_respects_element_bounds() {
    let arr: [i32; 4] = hegel::draw(&generators::arrays(
        generators::integers().min_value(0).max_value(100),
    ));
    for &x in &arr {
        assert!((0..=100).contains(&x));
    }
}

#[hegel::test]
fn test_array_with_mapped_elements() {
    let arr: [i32; 3] = hegel::draw(&generators::arrays(
        generators::integers::<i32>()
            .min_value(i32::MIN / 2)
            .max_value(i32::MAX / 2)
            .map(|x| x * 2),
    ));
    for &x in &arr {
        assert!(x % 2 == 0);
    }
}

#[hegel::test]
fn test_array_with_filtered_elements() {
    let arr: [i32; 3] = hegel::draw(&generators::arrays(
        generators::integers::<i32>()
            .min_value(0)
            .max_value(100)
            .filter(|n| n % 2 == 0),
    ));
    for &x in &arr {
        assert!(x % 2 == 0);
    }
}

#[hegel::test]
fn test_array_of_arrays() {
    let arr: [[i32; 2]; 3] = hegel::draw(&generators::arrays(generators::arrays(
        generators::integers::<i32>().min_value(0).max_value(50),
    )));
    assert_eq!(arr.len(), 3);
    for inner in &arr {
        assert_eq!(inner.len(), 2);
        for &x in inner {
            assert!((0..=50).contains(&x));
        }
    }
}

#[test]
fn test_array_generates_varying_values() {
    // An array of 5 integers from a wide range should not always be all the same
    find_any(
        generators::arrays::<_, i32, 5>(generators::integers()),
        |arr| arr.iter().collect::<std::collections::HashSet<_>>().len() > 1,
    );
}

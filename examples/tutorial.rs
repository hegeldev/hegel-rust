// This is a brief introduction to using Hegel in Rust. You can run these examples with `cargo test
// --examples`.
//
// Hegel provides a library of data generators in the `generators` module. Generators are invoked
// using `draw`. For example, `generators::integers::<i32>` is a generic 32 bit signed integer
// generator.
//
// To mark a function as a Hegel test, use the `hegel::test` attribute. Note that `draw` will fail
// if it's ever called outside of a Hegel test.

use hegel::generators::integers;
use hegel::generators::DefaultGenerator;
use hegel::draw;

#[hegel::test]
fn test_i32_addition_associative() {
    let x = draw(&integers::<i32>());
    let y = draw(&integers::<i32>());
    let z = draw(&integers::<i32>());
    let add = i32::wrapping_add;
    assert_eq!(add(x, add(y, z)), add(add(x, y), z));
}

// The `generators::collections` module provides generators for common collection types, such as
// vectors. Here's a test that the vector `sort` in the standard library always preserves the
// length of its input.

use hegel::generators::vecs;

#[hegel::test]
fn test_vector_sort_preserves_length() {
    let mut vector = draw(&vecs(&integers::<i32>()));
    let initial_length = vector.len();
    vector.sort();
    let sorted_length = vector.len();
    assert_eq!(initial_length, sorted_length);
}

// One common family of properties are "round trip" (or "invertibility") properties, where
// composing one function with another should get you back to where you started. Think, for
// example, of the relationship between a serialization function and a deserialization function.
// Here's how you might use Hegel to test a round trip property for JSON serialization.

use serde::{Serialize, Deserialize};
use hegel::generators;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct Point {
    x: i32,
    y: i32,
}

#[hegel::test]
fn test_point_serialization_round_trip() {
    let point = Point {
        x: draw(&integers()),
        y: draw(&integers()),
    };
    let serialized = serde_json::to_string(&point).unwrap();
    let deserialized: Point = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, point);
}

// Another important class of properties are "differential" properties, where two different
// implementations of the same functionality are checked for agreement. For example, you might
// check that an optimized function gives the same output as a simple, easy to understand
// reference implementation. Here's an example testing a quicksort implementation against the sort
// in the standard library.

fn quicksort<T: Ord>(vec: &mut Vec<T>) {
    let l = vec.len();
    if l > 1 {
        quicksort_range(vec, 0, l - 1);
    }
}

fn quicksort_range<T: Ord>(vec: &mut Vec<T>, low: usize, high: usize) {
    if low < high {
        let pivot = partition(vec, low, high);
        if pivot > 0 {
            quicksort_range(vec, low, pivot - 1);
        }
        quicksort_range(vec, pivot + 1, high);
    }
}

fn partition<T: Ord>(vec: &mut Vec<T>, low: usize, high: usize) -> usize {
    vec.swap(low + (high - low) / 2, high);
    let mut i = low;
    for j in low..high {
        if vec[j] <= vec[high] {
            vec.swap(i, j);
            i += 1;
        }
    }
    vec.swap(i, high);
    i
}

#[hegel::test]
fn test_quicksort() {
    let vector = draw(&vecs(&integers::<i32>()));

    let mut sorted = vector.clone();
    sorted.sort();

    let mut quicksorted = vector.clone();
    quicksort(&mut quicksorted);

    assert_eq!(sorted, quicksorted);
}

//! Tests that generators work correctly with non-'static (borrowed) types.
//!
//! These tests exercise the lifetime logic in BasicGenerator<'a, T> and
//! the phantom type parameters on composite generators.

use hegel::generators::{self, Generator};

#[hegel::test]
fn test_sampled_from_references() {
    let options = [10, 20, 30, 40, 50];
    let refs: Vec<&i32> = options.iter().collect();
    let value: &i32 = hegel::draw(&generators::sampled_from(refs));
    assert!(options.contains(value));
}

#[hegel::test]
fn test_sampled_from_str_references() {
    let strings = ["hello", "world", "foo", "bar"];
    let value: &str = hegel::draw(&generators::sampled_from(strings.to_vec()));
    assert!(strings.contains(&value));
}

#[hegel::test]
fn test_tuple_of_references() {
    let xs = [1, 2, 3];
    let ys = ["a", "b", "c"];
    let x_refs: Vec<&i32> = xs.iter().collect();
    let y_refs: Vec<&&str> = ys.iter().collect();
    let (x, y): (&i32, &&str) = hegel::draw(&generators::tuples2(
        generators::sampled_from(x_refs),
        generators::sampled_from(y_refs),
    ));
    assert!(xs.contains(x));
    assert!(ys.contains(y));
}

#[hegel::test]
fn test_optional_of_references() {
    let values = [100, 200, 300];
    let refs: Vec<&i32> = values.iter().collect();
    let result: Option<&i32> = hegel::draw(&generators::optional(generators::sampled_from(refs)));
    if let Some(v) = result {
        assert!(values.contains(v));
    }
}

#[hegel::test]
fn test_vec_of_references() {
    let options = [10, 20, 30];
    let refs: Vec<&i32> = options.iter().collect();
    let result: Vec<&i32> = hegel::draw(
        &generators::vecs(generators::sampled_from(refs))
            .min_size(1)
            .max_size(5),
    );
    assert!(!result.is_empty());
    for v in &result {
        assert!(options.contains(v));
    }
}

#[hegel::test]
fn test_map_over_references() {
    let values = [10, 20, 30];
    let refs: Vec<&i32> = values.iter().collect();
    let doubled: i32 = hegel::draw(&generators::sampled_from(refs).map(|r| r * 2));
    assert!([20, 40, 60].contains(&doubled));
}

#[hegel::test]
fn test_tuple3_of_references() {
    let xs = [1, 2];
    let ys = ["a", "b"];
    let zs = [true, false];
    let xr: Vec<&i32> = xs.iter().collect();
    let yr: Vec<&&str> = ys.iter().collect();
    let zr: Vec<&bool> = zs.iter().collect();
    let (x, y, z): (&i32, &&str, &bool) = hegel::draw(&generators::tuples3(
        generators::sampled_from(xr),
        generators::sampled_from(yr),
        generators::sampled_from(zr),
    ));
    assert!(xs.contains(x));
    assert!(ys.contains(y));
    assert!(zs.contains(z));
}

#[hegel::test]
fn test_nested_optional_tuple_of_references() {
    let names = ["alice", "bob", "carol"];
    let ages = [25u32, 30, 35];
    let name_refs: Vec<&&str> = names.iter().collect();
    let age_refs: Vec<&u32> = ages.iter().collect();
    let result: Option<(&&str, &u32)> = hegel::draw(&generators::optional(generators::tuples2(
        generators::sampled_from(name_refs),
        generators::sampled_from(age_refs),
    )));
    if let Some((name, age)) = result {
        assert!(names.contains(name));
        assert!(ages.contains(age));
    }
}

#[hegel::test]
fn test_vec_of_tuples_of_references() {
    let keys = [1, 2, 3];
    let vals = ["x", "y", "z"];
    let kr: Vec<&i32> = keys.iter().collect();
    let vr: Vec<&&str> = vals.iter().collect();
    let result: Vec<(&i32, &&str)> = hegel::draw(
        &generators::vecs(generators::tuples2(
            generators::sampled_from(kr),
            generators::sampled_from(vr),
        ))
        .max_size(5),
    );
    for (k, v) in &result {
        assert!(keys.contains(k));
        assert!(vals.contains(v));
    }
}

#[hegel::test]
fn test_deeply_nested_reference_composition() {
    // References flowing through: sampled_from -> tuple -> optional -> vec -> map
    let xs = [1i32, 2, 3];
    let ys = [4i32, 5, 6];
    let xr: Vec<&i32> = xs.iter().collect();
    let yr: Vec<&i32> = ys.iter().collect();

    let result: Vec<i32> = hegel::draw(
        &generators::vecs(
            generators::optional(generators::tuples2(
                generators::sampled_from(xr),
                generators::sampled_from(yr),
            ))
            .map(|opt| match opt {
                Some((a, b)) => a + b,
                None => 0,
            }),
        )
        .max_size(5),
    );

    for v in &result {
        assert!(
            *v == 0 || (5..=9).contains(v),
            "Expected 0 or 5..=9, got {}",
            v
        );
    }
}

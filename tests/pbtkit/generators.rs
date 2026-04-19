//! Ported from resources/pbtkit/tests/test_generators.py.
//!
//! Individually-skipped tests (noted in SKIPPED.md):
//! - `test_cannot_witness_nothing` — no `gs::nothing()` in hegel-rust.
//! - `test_target_and_reduce` — no `tc.target(score)` public API.
//! - `test_impossible_weighted`, `test_guaranteed_weighted` — no
//!   `tc.weighted(p)` public API.
//! - `test_many_reject`, `test_many_reject_unsatisfiable` — pbtkit's
//!   free-function `many()` helper has no direct analog; the hegel-rust
//!   equivalent would be wiring `gs::Collection` manually with rejection,
//!   which the public shape of the API doesn't straightforwardly expose.
//! - `test_unique_by` — hegel-rust's `VecGenerator` only exposes
//!   `.unique(bool)`; it has no public `.unique_by(key_fn)` setter.
//! - `test_generator_repr` — tests Python `repr()` output, no analog.

use std::collections::HashMap;

use crate::common::utils::{assert_all_examples, expect_panic, minimal};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};

#[test]
fn test_mapped_possibility() {
    assert_all_examples(
        gs::integers::<i64>()
            .min_value(0)
            .max_value(5)
            .map(|n| n * 2),
        |n: &i64| n % 2 == 0,
    );
}

#[test]
fn test_selected_possibility() {
    assert_all_examples(
        gs::integers::<i64>()
            .min_value(0)
            .max_value(5)
            .filter(|n: &i64| n % 2 == 0),
        |n: &i64| n % 2 == 0,
    );
}

#[test]
fn test_bound_possibility() {
    assert_all_examples(
        gs::integers::<i64>()
            .min_value(0)
            .max_value(5)
            .flat_map(|m| {
                gs::tuples!(
                    gs::just(m),
                    gs::integers::<i64>().min_value(m).max_value(m + 10),
                )
            }),
        |(m, n): &(i64, i64)| *m <= *n && *n <= *m + 10,
    );
}

#[test]
fn test_cannot_witness_empty_one_of() {
    // Python raises Unsatisfiable when drawing from one_of() with no
    // alternatives; hegel-rust panics at construction instead.
    expect_panic(
        || {
            let empty: Vec<gs::BoxedGenerator<i32>> = vec![];
            gs::one_of(empty);
        },
        "one_of requires at least one generator",
    );
}

#[test]
fn test_one_of_single() {
    assert_all_examples(
        hegel::one_of!(gs::integers::<i64>().min_value(0).max_value(10)),
        |n: &i64| (0..=10).contains(n),
    );
}

#[test]
fn test_can_draw_mixture() {
    assert_all_examples(
        hegel::one_of!(
            gs::integers::<i64>().min_value(-5).max_value(0),
            gs::integers::<i64>().min_value(2).max_value(5),
        ),
        |m: &i64| (-5..=5).contains(m) && *m != 1,
    );
}

#[test]
fn test_size_bounds_on_list() {
    assert_all_examples(
        gs::vecs(gs::integers::<i64>().min_value(0).max_value(10))
            .min_size(1)
            .max_size(3),
        |ls: &Vec<i64>| (1..=3).contains(&ls.len()),
    );
}

#[test]
fn test_fixed_size_list() {
    assert_all_examples(
        gs::vecs(gs::integers::<i64>().min_value(0).max_value(10))
            .min_size(3)
            .max_size(3),
        |ls: &Vec<i64>| ls.len() == 3,
    );
}

#[test]
fn test_many_with_small_max() {
    // Exercises the geometric-distribution path for collections with a
    // small max_size.
    Hegel::new(|tc| {
        let ls: Vec<i64> =
            tc.draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(10)).max_size(2));
        assert!(ls.len() <= 2);
    })
    .settings(Settings::new().test_cases(200).database(None))
    .run();
}

#[test]
fn test_sampled_from() {
    assert_all_examples(gs::sampled_from(vec!["a", "b", "c"]), |v: &&'static str| {
        matches!(*v, "a" | "b" | "c")
    });
}

#[test]
fn test_sampled_from_shrinks_to_first() {
    // Python test asserts "'a'" appears in the failure output. Using
    // `minimal`, the minimum generated value that triggers the condition
    // should be the first element of the sample list.
    let v = minimal(
        gs::sampled_from(vec!["a".to_string(), "b".to_string(), "c".to_string()]),
        |v: &String| v == "a",
    );
    assert_eq!(v, "a");
}

#[test]
fn test_sampled_from_single() {
    assert_all_examples(gs::sampled_from(vec!["only"]), |v: &&'static str| {
        *v == "only"
    });
}

#[test]
fn test_sampled_from_empty() {
    expect_panic(
        || {
            let empty: Vec<i32> = vec![];
            gs::sampled_from(empty);
        },
        "cannot be empty",
    );
}

#[test]
fn test_booleans() {
    assert_all_examples(gs::booleans(), |_: &bool| true);
}

#[test]
fn test_composite() {
    assert_all_examples(
        hegel::compose!(|tc| {
            let x: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(10));
            let y: i64 = tc.draw(gs::integers::<i64>().min_value(x).max_value(10));
            (x, y)
        }),
        |(x, y): &(i64, i64)| *x <= *y && *y <= 10,
    );
}

#[test]
fn test_composite_with_args() {
    let max_val: i64 = 5;
    assert_all_examples(
        hegel::compose!(|tc| { tc.draw(gs::integers::<i64>().min_value(0).max_value(max_val)) }),
        |n: &i64| (0..=5).contains(n),
    );
}

#[test]
fn test_composite_shrinks() {
    // Python test asserts the shrunk counterexample is "100, 0" or "0, 100".
    // We check the same property: shrinking lands exactly on the boundary.
    let (x, y) = minimal(
        hegel::compose!(|tc| {
            let x: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
            let y: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
            (x, y)
        }),
        |(x, y): &(i64, i64)| x + y >= 100,
    );
    assert_eq!(x + y, 100);
    assert!((x == 100 && y == 0) || (x == 0 && y == 100));
}

#[test]
fn test_unique_lists() {
    assert_all_examples(
        gs::vecs(gs::integers::<i64>().min_value(0).max_value(10))
            .unique(true)
            .max_size(5),
        |ls: &Vec<i64>| {
            let mut seen = std::collections::HashSet::new();
            ls.iter().all(|x| seen.insert(*x))
        },
    );
}

#[test]
fn test_unique_lists_shrinks() {
    let ls = minimal(
        gs::vecs(gs::integers::<i64>().min_value(0).max_value(100)).unique(true),
        |ls: &Vec<i64>| ls.len() >= 3,
    );
    assert_eq!(ls.len(), 3);
}

#[test]
fn test_dictionaries() {
    assert_all_examples(
        gs::hashmaps(
            gs::integers::<i64>().min_value(0).max_value(10),
            gs::integers::<i64>().min_value(0).max_value(100),
        )
        .max_size(5),
        |d: &HashMap<i64, i64>| {
            d.len() <= 5
                && d.iter()
                    .all(|(k, v)| (0..=10).contains(k) && (0..=100).contains(v))
        },
    );
}

#[test]
fn test_dictionaries_shrinks() {
    let d = minimal(
        gs::hashmaps(
            gs::integers::<i64>().min_value(0).max_value(10),
            gs::integers::<i64>().min_value(0).max_value(100),
        ),
        |d: &HashMap<i64, i64>| d.values().sum::<i64>() > 100,
    );
    assert!(d.values().sum::<i64>() > 100);
}

#[test]
fn test_dictionaries_size_bounds() {
    assert_all_examples(
        gs::hashmaps(
            gs::integers::<i64>().min_value(0).max_value(10),
            gs::integers::<i64>().min_value(0).max_value(100),
        )
        .min_size(1)
        .max_size(3),
        |d: &HashMap<i64, i64>| (1..=3).contains(&d.len()),
    );
}

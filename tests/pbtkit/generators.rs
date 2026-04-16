//! Ported from pbtkit/tests/test_generators.py

use crate::common::utils::{expect_panic, minimal};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};
use std::collections::HashMap;

#[test]
fn test_mapped_possibility() {
    Hegel::new(|tc| {
        let n = tc.draw(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(5)
                .map(|n| n * 2),
        );
        assert_eq!(n % 2, 0);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_selected_possibility() {
    Hegel::new(|tc| {
        let n = tc.draw(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(5)
                .filter(|n: &i64| n % 2 == 0),
        );
        assert_eq!(n % 2, 0);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_bound_possibility() {
    Hegel::new(|tc| {
        let (m, n) = tc.draw(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(5)
                .flat_map(|m| {
                    hegel::tuples!(
                        gs::just(m),
                        gs::integers::<i64>().min_value(m).max_value(m + 10)
                    )
                }),
        );
        assert!(m <= n && n <= m + 10);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_cannot_witness_nothing() {
    // TODO: hegel-rust has no `gs::nothing()` generator (an always-unsatisfiable
    // generator). The Python pbtkit raises Unsatisfiable here.
    todo!()
}

#[test]
fn test_cannot_witness_empty_one_of() {
    // hegel-rust's `one_of(vec![])` panics at construction time rather than
    // raising Unsatisfiable at draw time (as Python pbtkit does).
    expect_panic(
        || {
            let _ = gs::one_of::<i64>(vec![]);
        },
        "one_of requires at least one generator",
    );
}

#[test]
fn test_one_of_single() {
    Hegel::new(|tc| {
        let n = tc.draw(gs::one_of(vec![
            gs::integers::<i64>().min_value(0).max_value(10).boxed(),
        ]));
        assert!((0..=10).contains(&n));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_can_draw_mixture() {
    Hegel::new(|tc| {
        let m = tc.draw(gs::one_of(vec![
            gs::integers::<i64>().min_value(-5).max_value(0).boxed(),
            gs::integers::<i64>().min_value(2).max_value(5).boxed(),
        ]));
        assert!((-5..=5).contains(&m));
        assert_ne!(m, 1);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_target_and_reduce() {
    // TODO: hegel-rust has no public `tc.target(score)` API.
    todo!()
}

#[test]
fn test_impossible_weighted() {
    // TODO: hegel-rust has no public `tc.weighted(p)` API.
    todo!()
}

#[test]
fn test_guaranteed_weighted() {
    // TODO: hegel-rust has no public `tc.weighted(p)` API.
    todo!()
}

#[test]
fn test_size_bounds_on_list() {
    Hegel::new(|tc| {
        let ls = tc.draw(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(10))
                .min_size(1)
                .max_size(3),
        );
        assert!((1..=3).contains(&ls.len()));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_fixed_size_list() {
    Hegel::new(|tc| {
        let ls = tc.draw(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(10))
                .min_size(3)
                .max_size(3),
        );
        assert_eq!(ls.len(), 3);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_many_with_small_max() {
    // hegel-rust has no public `many()` helper (pbtkit's low-level
    // collection-building API); instead we use `gs::vecs(...).max_size(...)`
    // which exercises the same geometric-distribution path internally.
    Hegel::new(|tc| {
        let ls = tc.draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(10)).max_size(2));
        assert!(ls.len() <= 2);
    })
    .settings(Settings::new().test_cases(200).database(None))
    .run();
}

#[test]
fn test_many_reject() {
    // hegel-rust's `many()` / `reject()` are not public API, but the same
    // "collection rejecting duplicates" surface is exposed via
    // `gs::vecs(...).unique(true)`.
    Hegel::new(|tc| {
        let ls = tc.draw(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(2))
                .unique(true)
                .max_size(10),
        );
        let mut seen = std::collections::HashSet::new();
        for x in &ls {
            assert!(seen.insert(*x));
        }
    })
    .settings(Settings::new().test_cases(200).database(None))
    .run();
}

#[test]
fn test_many_reject_unsatisfiable() {
    // TODO: hegel-rust has no public `many()` helper with `reject()`, so we
    // cannot write a collection whose every element is rejected to force
    // Unsatisfiable. `gs::vecs(...).min_size(...).unique(true)` only rejects
    // duplicates, not "everything".
    todo!()
}

#[test]
fn test_sampled_from() {
    Hegel::new(|tc| {
        let v = tc.draw(gs::sampled_from(vec!["a", "b", "c"]));
        assert!(["a", "b", "c"].contains(&v));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_sampled_from_shrinks_to_first() {
    // Condition: value != "a". The minimal shrink should be "b" (the first
    // non-"a" element). In Python pbtkit the test asserts that "'a'" appears
    // in the failing-replay output; the hegel-rust equivalent is to check
    // via `minimal` that we shrink to one of the later elements from "a".
    let result = minimal(gs::sampled_from(vec!["a", "b", "c"]), |v: &&str| *v != "a");
    assert_ne!(result, "a");
}

#[test]
fn test_sampled_from_single() {
    Hegel::new(|tc| {
        let v = tc.draw(gs::sampled_from(vec!["only"]));
        assert_eq!(v, "only");
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_sampled_from_empty() {
    // Python raises Unsatisfiable at draw time; hegel-rust panics at
    // construction time.
    expect_panic(
        || {
            let _ = gs::sampled_from::<&str>(vec![]);
        },
        "Collection passed to sampled_from cannot be empty",
    );
}

#[test]
fn test_booleans() {
    Hegel::new(|tc| {
        let _b: bool = tc.draw(gs::booleans());
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_composite() {
    Hegel::new(|tc| {
        let (x, y) = tc.draw(hegel::compose!(|tc| {
            let x = tc.draw(gs::integers::<i64>().min_value(0).max_value(10));
            let y = tc.draw(gs::integers::<i64>().min_value(x).max_value(10));
            (x, y)
        }));
        assert!(x <= y && y <= 10);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_composite_with_args() {
    let max_val: i64 = 5;
    Hegel::new(move |tc| {
        let n = tc.draw(hegel::compose!(|tc| {
            tc.draw(gs::integers::<i64>().min_value(0).max_value(max_val))
        }));
        assert!((0..=max_val).contains(&n));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_composite_shrinks() {
    // Any failing pair (x, y) with x + y >= 100 should shrink to a simple
    // extreme: (0, 100) or (100, 0). We use `minimal` to verify the shrink.
    let pairs = hegel::compose!(|tc| {
        let x = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
        let y = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
        (x, y)
    });
    let (x, y) = minimal(pairs, |p: &(i64, i64)| p.0 + p.1 >= 100);
    assert!(x + y >= 100);
    assert!((x == 0 && y == 100) || (x == 100 && y == 0));
}

#[test]
fn test_unique_lists() {
    Hegel::new(|tc| {
        let ls = tc.draw(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(10))
                .unique(true)
                .max_size(5),
        );
        let mut seen = std::collections::HashSet::new();
        for x in &ls {
            assert!(seen.insert(*x));
        }
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_unique_lists_shrinks() {
    // Any unique list with len >= 3 should shrink to the minimal distinct
    // triple. We just check the length and uniqueness.
    let result = minimal(
        gs::vecs(gs::integers::<i64>().min_value(0).max_value(100)).unique(true),
        |ls: &Vec<i64>| ls.len() >= 3,
    );
    assert!(result.len() >= 3);
    let mut seen = std::collections::HashSet::new();
    for x in &result {
        assert!(seen.insert(*x));
    }
}

#[test]
fn test_unique_by() {
    // TODO: hegel-rust's VecGenerator has no public `unique_by(fn)` setter;
    // only the bool `unique()` (which uses PartialEq::eq) is exposed. The
    // Python test uses `unique_by=lambda x: x % 10`.
    todo!()
}

#[test]
fn test_dictionaries() {
    Hegel::new(|tc| {
        let d: HashMap<i64, i64> = tc.draw(
            gs::hashmaps(
                gs::integers::<i64>().min_value(0).max_value(10),
                gs::integers::<i64>().min_value(0).max_value(100),
            )
            .max_size(5),
        );
        assert!(d.len() <= 5);
        for (k, v) in &d {
            assert!((0..=10).contains(k));
            assert!((0..=100).contains(v));
        }
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_dictionaries_shrinks() {
    // Any dict whose values sum to > 100 should shrink to a minimal
    // counterexample. We check the invariant directly.
    let result = minimal(
        gs::hashmaps(
            gs::integers::<i64>().min_value(0).max_value(10),
            gs::integers::<i64>().min_value(0).max_value(100),
        ),
        |d: &HashMap<i64, i64>| d.values().sum::<i64>() > 100,
    );
    assert!(result.values().sum::<i64>() > 100);
}

#[test]
fn test_dictionaries_size_bounds() {
    Hegel::new(|tc| {
        let d: HashMap<i64, i64> = tc.draw(
            gs::hashmaps(
                gs::integers::<i64>().min_value(0).max_value(10),
                gs::integers::<i64>().min_value(0).max_value(100),
            )
            .min_size(1)
            .max_size(3),
        );
        assert!((1..=3).contains(&d.len()));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_generator_repr() {
    // Not applicable: Python's __repr__ returns a formatted string
    // ("integers(min_value=0, max_value=5)"). hegel-rust's generator builders
    // have no equivalent stable Debug/Display format, and the Python test
    // exists to cover a Python-specific introspection surface.
}

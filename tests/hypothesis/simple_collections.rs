//! Ported from hypothesis-python/tests/cover/test_simple_collections.py
//!
//! Individually-skipped tests:
//!
//! - `test_find_empty_collection_gives_empty` — partial port. The
//!   `tuples()`, `lists(none(), max_size=0)`, `sets(none(), max_size=0)`,
//!   and `fixed_dictionaries({})` rows are ported below. The remaining
//!   rows rely on public-API features with no hegel-rust counterpart:
//!   `gs::nothing()`, `gs::frozensets()`, `fixed_dictionaries(...,
//!   optional=...)`, and non-string `fixed_dictionaries` keys.
//! - `test_fixed_dictionaries_with_optional_and_empty_keys` — uses the
//!   `optional=` kwarg and `gs::nothing()`, neither of which exists.
//! - `test_minimize_dicts_with_incompatible_keys` — mixes `int` and `str`
//!   keys in one dict; Rust's type system makes this unrepresentable.
//! - `test_lists_unique_by_tuple_funcs` — uses
//!   `unique_by=(key_fn_1, key_fn_2)`; `VecGenerator` exposes only
//!   `.unique(bool)`, no `.unique_by(key_fn)` setter.
//! - `test_can_find_unique_lists_of_non_set_order` — Python retries under
//!   `@flaky` because its predicate depends on process-randomised set
//!   iteration order. hegel-rust's engine classifies a non-deterministic
//!   predicate as a flaky-test bug and raises `Flaky test detected`
//!   inside the property run, so the test can't be stabilised with an
//!   outer retry.
//!
//! `test_find_non_empty_collection_gives_single_zero` and
//! `test_minimizes_to_empty` port the `list` and `set` parametrize rows
//! but drop the `frozenset` row (no `gs::frozensets()`).

use crate::common::utils::{assert_all_examples, find_any, minimal};
use ciborium::Value;
use hegel::generators::{self as gs, Generator};
use std::collections::{HashMap, HashSet};

#[test]
fn test_find_empty_tuple_gives_empty() {
    // Rust's type system guarantees the returned value is `()`; the
    // upstream `assert == ()` is vacuous here — this runs as a smoke test.
    minimal(gs::tuples!(), |_: &()| true);
}

#[test]
fn test_find_empty_list_gives_empty() {
    let xs: Vec<()> = minimal(gs::vecs(gs::unit()).max_size(0), |_| true);
    assert_eq!(xs, Vec::<()>::new());
}

#[test]
fn test_find_empty_set_gives_empty() {
    let xs: HashSet<()> = minimal(gs::hashsets(gs::unit()).max_size(0), |_| true);
    assert!(xs.is_empty());
}

#[test]
fn test_find_empty_fixed_dict_gives_empty() {
    let v: Value = minimal(gs::fixed_dicts().build(), |_| true);
    let Value::Map(entries) = v else {
        panic!("expected Value::Map");
    };
    assert!(entries.is_empty());
}

#[test]
fn test_fixed_dicts_preserve_field_order() {
    // `OrderedDict` in upstream asserts fixed_dictionaries preserves key
    // order. hegel-rust's `gs::fixed_dicts()` only takes string keys, but
    // the underlying `Value::Map` preserves insertion order of `.field()`
    // calls — port with a non-sorted string ordering.
    let keys: Vec<String> = ["k7", "k2", "k0", "k3", "k9", "k1", "k5", "k8", "k4", "k6"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut builder = gs::fixed_dicts();
    for k in &keys {
        builder = builder.field(k, gs::booleans());
    }
    let expected = keys.clone();
    assert_all_examples(builder.build(), move |v: &Value| {
        let Value::Map(entries) = v else {
            return false;
        };
        let got: Vec<String> = entries
            .iter()
            .map(|(k, _)| match k {
                Value::Text(s) => s.clone(),
                _ => panic!("expected text key, got {:?}", k),
            })
            .collect();
        got == expected
    });
}

#[test]
fn test_find_non_empty_list_gives_single_zero() {
    let xs: Vec<i64> = minimal(gs::vecs(gs::integers::<i64>()), |xs: &Vec<i64>| {
        !xs.is_empty()
    });
    assert_eq!(xs, vec![0_i64]);
}

#[test]
fn test_find_non_empty_set_gives_single_zero() {
    let xs: HashSet<i64> = minimal(gs::hashsets(gs::integers::<i64>()), |xs: &HashSet<i64>| {
        !xs.is_empty()
    });
    let expected: HashSet<i64> = std::iter::once(0_i64).collect();
    assert_eq!(xs, expected);
}

#[test]
fn test_minimizes_list_to_empty() {
    let xs: Vec<i64> = minimal(gs::vecs(gs::integers::<i64>()), |_| true);
    assert_eq!(xs, Vec::<i64>::new());
}

#[test]
fn test_minimizes_set_to_empty() {
    let xs: HashSet<i64> = minimal(gs::hashsets(gs::integers::<i64>()), |_| true);
    assert!(xs.is_empty());
}

#[test]
fn test_minimizes_list_of_lists() {
    // Python `any(x) and not all(x)` on a list-of-lists tests non-emptiness
    // of each inner list (empty lists are falsy in Python).
    let mut xs: Vec<Vec<bool>> =
        minimal(gs::vecs(gs::vecs(gs::booleans())), |x: &Vec<Vec<bool>>| {
            x.iter().any(|inner| !inner.is_empty()) && !x.iter().all(|inner| !inner.is_empty())
        });
    xs.sort();
    assert_eq!(xs, vec![vec![], vec![false]]);
}

#[test]
fn test_sets_are_size_bounded() {
    assert_all_examples(
        gs::hashsets(gs::integers::<i64>().min_value(0).max_value(100))
            .min_size(2)
            .max_size(10),
        |xs: &HashSet<i64>| (2..=10).contains(&xs.len()),
    );
}

#[test]
fn test_small_sized_sets() {
    // Just needs to be able to run — upstream body is `pass`.
    assert_all_examples(
        gs::vecs(gs::hashsets(gs::unit())).min_size(10),
        |x: &Vec<HashSet<()>>| x.len() >= 10,
    );
}

#[test]
fn test_lists_of_fixed_length() {
    for n in 0_usize..10 {
        let result: Vec<i64> = minimal(
            gs::vecs(gs::integers::<i64>()).min_size(n).max_size(n),
            |_| true,
        );
        assert_eq!(result, vec![0_i64; n]);
    }
}

#[test]
fn test_sets_of_fixed_length() {
    for n in 0_usize..10 {
        let x: HashSet<i64> = minimal(
            gs::hashsets(gs::integers::<i64>()).min_size(n).max_size(n),
            |_| true,
        );
        assert_eq!(x.len(), n);
        if n == 0 {
            assert!(x.is_empty());
        } else {
            let min = *x.iter().min().unwrap();
            let expected: HashSet<i64> = (min..min + n as i64).collect();
            assert_eq!(x, expected);
        }
    }
}

#[test]
fn test_dictionaries_of_fixed_length() {
    for n in 0_usize..10 {
        let m: HashMap<i64, bool> = minimal(
            gs::hashmaps(gs::integers::<i64>(), gs::booleans())
                .min_size(n)
                .max_size(n),
            |_| true,
        );
        let x: HashSet<i64> = m.keys().copied().collect();
        if n == 0 {
            assert!(x.is_empty());
        } else {
            let min = *x.iter().min().unwrap();
            let expected: HashSet<i64> = (min..min + n as i64).collect();
            assert_eq!(x, expected);
        }
    }
}

#[test]
fn test_lists_of_lower_bounded_length() {
    for n in 0_usize..10 {
        // Use i128 to match Python's unbounded int semantics — raw i64 sums
        // overflow on extreme generated values.
        let l: Vec<i64> = minimal(
            gs::vecs(gs::integers::<i64>()).min_size(n),
            move |x: &Vec<i64>| x.iter().copied().map(i128::from).sum::<i128>() >= 2 * n as i128,
        );
        let expected: Vec<i64> = if n == 0 {
            Vec::new()
        } else {
            let mut v = vec![0_i64; n - 1];
            v.push(n as i64 * 2);
            v
        };
        assert_eq!(l, expected);
    }
}

#[test]
fn test_can_draw_empty_list_from_unsatisfiable_strategy() {
    let xs: Vec<i64> = find_any(
        gs::vecs(gs::integers::<i64>().filter(|_: &i64| false)),
        |_| true,
    );
    assert_eq!(xs, Vec::<i64>::new());
}

#[test]
fn test_can_draw_empty_set_from_unsatisfiable_strategy() {
    let xs: HashSet<i64> = find_any(
        gs::hashsets(gs::integers::<i64>().filter(|_: &i64| false)),
        |_| true,
    );
    assert!(xs.is_empty());
}

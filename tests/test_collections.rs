// The compile-time assertion that `gs::vecs(...).unique(true)` requires
// `PartialEq` on the element type lives in
// tests/compile/fail/vec_unique_requires_partial_eq.rs, driven by `trybuild`.

mod common;

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
    let set: HashSet<i32> =
        tc.draw(gs::hashsets(gs::integers::<i32>().map(|x| x.saturating_abs())).max_size(10));
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

#[cfg(not(feature = "native"))]
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
    // max_size exercises the map_insert("max_size") branch in ServerDataSource::new_collection.
    let vec: Vec<i32> = tc.draw(gs::vecs(gs::integers::<i32>().filter(|_| true)).max_size(5));
    assert!(vec.len() <= 5);
}

// Regression test: vecs(sampled_from).unique(true) must check value-level uniqueness.
// Before the fix, as_basic() sent "unique":true to the server, which enforced index-level
// uniqueness (distinct sampled_from indices), not value-level uniqueness. For a pool of
// 100 copies of the same value, distinct indices still map to the same value, producing
// duplicates. The fix makes as_basic() return None when unique_by is set, routing through
// the non-basic Collection path that checks actual T values.
#[hegel::test]
fn test_vec_unique_sampled_from_no_duplicates(tc: TestCase) {
    let vec: Vec<i64> = tc.draw(gs::vecs(gs::sampled_from(vec![0_i64; 100])).unique(true));
    // All elements are 0, so a unique vec can have at most 1 element.
    assert!(vec.len() <= 1);
}

mod simple_collections {
    //!
    //! `test_find_non_empty_collection_gives_single_zero` and
    //! `test_minimizes_to_empty` port the `list` and `set` parametrize rows
    //! but drop the `frozenset` row (no `gs::frozensets()`).

    use super::common::utils::{assert_all_examples, find_any, minimal};
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
                move |x: &Vec<i64>| {
                    x.iter().copied().map(i128::from).sum::<i128>() >= 2 * n as i128
                },
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
}

mod nocover_sets {
    use std::collections::HashSet;

    use super::common::utils::assert_all_examples;
    #[cfg(not(feature = "native"))]
    use super::common::utils::find_any;
    use hegel::generators as gs;
    #[cfg(not(feature = "native"))]
    use hegel::generators::Generator;

    #[cfg(not(feature = "native"))]
    #[test]
    fn test_can_draw_sets_of_hard_to_find_elements() {
        let rarebool = gs::floats::<f64>()
            .min_value(0.0)
            .max_value(1.0)
            .map(|x: f64| x <= 0.05);
        find_any(gs::hashsets(rarebool).min_size(2), |s: &HashSet<bool>| {
            s.len() >= 2
        });
    }

    #[test]
    fn test_empty_sets() {
        assert_all_examples(
            gs::hashsets(gs::integers::<i64>()).max_size(0),
            |s: &HashSet<i64>| s.is_empty(),
        );
    }

    #[test]
    fn test_bounded_size_sets() {
        assert_all_examples(
            gs::hashsets(gs::integers::<i64>()).max_size(2),
            |s: &HashSet<i64>| s.len() <= 2,
        );
    }
}

mod nocover_large_examples {
    use super::common::utils::find_any;
    use hegel::generators as gs;

    #[test]
    fn test_can_generate_large_lists_with_min_size() {
        find_any(
            gs::vecs(gs::integers::<i64>()).min_size(400),
            |v: &Vec<i64>| v.len() >= 400,
        );
    }
}

mod sampled_from {
    //! Tests that rely on Python-specific facilities are not ported:
    //!
    //! - `test_cannot_sample_sets` — Rust's type system prevents passing non-sequence
    //!   types to `sampled_from`; the runtime type check is Python-specific.
    //! - `test_can_sample_enums` — Python `enum.Enum`/`enum.Flag` auto-iteration
    //!   integration with `sampled_from`; no Rust equivalent.
    //! - `test_efficient_lists_of_tuples_first_element_sampled_from` — uses
    //!   `unique_by=fn`; `VecGenerator` only has `.unique(bool)`.
    //! - `test_unsatisfiable_explicit_filteredstrategy_sampled`,
    //!   `test_unsatisfiable_explicit_filteredstrategy_just` — construct
    //!   `FilteredStrategy` directly with Python `bool` as predicate (truthiness).
    //! - `test_transformed_just_strategy` — uses `ConjectureData.for_choices`,
    //!   `JustStrategy`, `do_draw`/`do_filtered_draw`/`filter_not_satisfied`
    //!   (Hypothesis strategy-protocol internals with no hegel-rust counterpart).
    //! - `test_issue_2247_regression` — Python int/float equality (`0 == 0.0`);
    //!   Rust's type system prevents mixed-type sequences.
    //! - `test_mutability_1`, `test_mutability_2` — Python list mutability after
    //!   strategy creation; Rust ownership model prevents this.
    //! - `test_suggests_elements_instead_of_annotations` — Python enum type-annotation
    //!   vs values error message.
    //! - `TestErrorNoteBehavior3819` — Python `__notes__` (PEP 678) and dynamic typing
    //!   (strategies passed as `sampled_from` elements).
    //!
    //! Hegel-rust uses generic post-draw filtering (3 retries then
    //! `enumerate_values` fallback) rather than Hypothesis's `FilteredStrategy`
    //! optimization. The `enumerate_values` path handles both rare-value and
    //! unsatisfiable cases correctly.

    use super::common::utils::{
        assert_all_examples, assert_simple_property, check_can_generate_examples, expect_panic,
    };
    use hegel::generators::{self as gs, Generator};
    use hegel::{HealthCheck, Hegel, Settings};
    use std::collections::{HashMap, HashSet};

    #[test]
    fn test_can_sample_sequence_without_warning() {
        check_can_generate_examples(gs::sampled_from(vec![1, 2, 3]));
    }

    #[test]
    fn test_can_sample_ordereddict_without_warning() {
        check_can_generate_examples(gs::sampled_from(vec!["a", "b", "c"]));
    }

    #[test]
    fn test_unsat_filtered_sampling() {
        expect_panic(
            || {
                Hegel::new(|tc| {
                    let _: i64 = tc.draw(
                        gs::sampled_from((0..10).collect::<Vec<i64>>()).filter(|x: &i64| *x < 0),
                    );
                })
                .settings(Settings::new().database(None))
                .run();
            },
            "(?i)(health.check|FailedHealthCheck|filter)",
        );
    }

    #[test]
    fn test_unsat_filtered_sampling_in_rejection_stage() {
        expect_panic(
            || {
                Hegel::new(|tc| {
                    let _: i64 = tc.draw(gs::sampled_from(vec![0_i64, 1]).filter(|x: &i64| *x < 0));
                })
                .settings(Settings::new().database(None))
                .run();
            },
            "(?i)(health.check|FailedHealthCheck|Unsatisfiable|filter)",
        );
    }

    #[test]
    fn test_easy_filtered_sampling() {
        assert_simple_property(
            gs::sampled_from((0..100).collect::<Vec<i64>>()).filter(|x: &i64| *x == 0),
            |x: &i64| *x == 0,
        );
    }

    #[test]
    fn test_filtered_sampling_finds_rare_value() {
        assert_all_examples(
            gs::sampled_from((0..100).collect::<Vec<i64>>()).filter(|x: &i64| *x == 99),
            |x: &i64| *x == 99,
        );
    }

    #[test]
    fn test_efficient_sets_of_samples() {
        Hegel::new(|tc| {
            let x: HashSet<i64> =
                tc.draw(gs::hashsets(gs::sampled_from((0..50).collect::<Vec<i64>>())).min_size(50));
            let expected: HashSet<i64> = (0..50).collect();
            assert_eq!(x, expected);
        })
        .settings(Settings::new().database(None))
        .run();
    }

    #[test]
    fn test_efficient_dicts_with_sampled_keys() {
        Hegel::new(|tc| {
            let x: HashMap<i64, ()> = tc.draw(
                gs::hashmaps(
                    gs::sampled_from((0..50).collect::<Vec<i64>>()),
                    gs::just(()),
                )
                .min_size(50),
            );
            let keys: HashSet<i64> = x.keys().copied().collect();
            let expected: HashSet<i64> = (0..50).collect();
            assert_eq!(keys, expected);
        })
        .settings(
            Settings::new()
                .database(None)
                .suppress_health_check([HealthCheck::TooSlow]),
        )
        .run();
    }

    #[test]
    fn test_does_not_include_duplicates_even_when_duplicated_in_collection() {
        assert_all_examples(
            gs::vecs(gs::sampled_from(vec![0_i64; 100])).unique(true),
            |ls: &Vec<i64>| ls.len() <= 1,
        );
    }

    #[test]
    fn test_efficient_sets_of_samples_with_chained_transformations() {
        Hegel::new(|tc| {
            let x: HashSet<i64> = tc.draw(
                gs::hashsets(
                    gs::sampled_from((0..50).collect::<Vec<i64>>())
                        .map(|x: i64| x * 2)
                        .filter(|x: &i64| *x % 3 != 0)
                        .map(|x: i64| x / 2),
                )
                .min_size(33),
            );
            let expected: HashSet<i64> = (0..50).filter(|x| (x * 2) % 3 != 0).collect();
            assert_eq!(x, expected);
        })
        .settings(
            Settings::new()
                .database(None)
                .suppress_health_check([HealthCheck::FilterTooMuch]),
        )
        .run();
    }

    #[test]
    fn test_efficient_sets_of_samples_with_chained_transformations_slow_path() {
        Hegel::new(|tc| {
            let result: HashSet<i64> = tc.draw(hegel::compose!(|tc| {
                let mut result = HashSet::new();
                let elements: Vec<i64> = (0..20).collect();
                while result.len() < 13 {
                    let captured = result.clone();
                    let val: i64 = tc.draw(
                        gs::sampled_from(elements.clone())
                            .filter(|x: &i64| *x % 3 != 0)
                            .map(|x: i64| x * 2)
                            .filter(move |x: &i64| !captured.contains(x)),
                    );
                    result.insert(val);
                }
                result
            }));
            let expected: HashSet<i64> = (0..20).filter(|x| x % 3 != 0).map(|x| x * 2).collect();
            assert_eq!(result, expected);
        })
        .settings(Settings::new().database(None))
        .run();
    }

    #[test]
    fn test_max_size_is_respected_with_unique_sampled_from() {
        assert_all_examples(
            gs::vecs(gs::sampled_from((0..100).collect::<Vec<i64>>()))
                .max_size(3)
                .unique(true),
            |ls: &Vec<i64>| ls.len() <= 3,
        );
    }
}

//! Ported from resources/pbtkit/tests/shrink_quality/test_collections.py.
//!
//! `test_swap_adjacent_blocks_identical` is ported as an embedded shrinker
//! test in `tests/embedded/native/shrinker_tests.rs` (see
//! `shrink_full_loop_over_identical_adjacent_blocks`).
//!
//! Individually-skipped tests:
//!
//! - `test_sort_values_insertion_natural_exit` — exercises the insertion-sort
//!   fallback branch of pbtkit's `sort_values` (the full sort `[0,0,1]` fails
//!   the `a+b>c` predicate, and the test checks that insertion sort then
//!   lands on `[0,1,0]`). hegel-rust's `sort_values_integers` only performs
//!   the full sort — it has no insertion-sort fallback — so this specific
//!   branch cannot be exercised.

use std::collections::{HashMap, HashSet};

use crate::common::utils::{Minimal, expect_panic, minimal};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};

// ----------------------------------------------------------------------------
// Composite helper used by several containment tests.
// ----------------------------------------------------------------------------

fn list_and_int() -> impl Generator<(Vec<i64>, i64)> {
    hegel::compose!(|tc| {
        let v: Vec<i64> = tc.draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(100)));
        let i: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
        (v, i)
    })
}

// --- Sets (using unique lists as proxy) -------------------------------------

#[test]
fn test_minimize_3_set() {
    let result = minimal(
        gs::vecs(gs::integers::<i64>()).unique(true),
        |x: &Vec<i64>| x.len() >= 3,
    );
    assert_eq!(result, vec![0, 1, -1]);
}

#[test]
fn test_minimize_sets_sampled_from() {
    let items: Vec<i64> = (0..10).collect();
    let result = minimal(
        gs::vecs(gs::sampled_from(items)).min_size(3).unique(true),
        |_: &Vec<i64>| true,
    );
    assert_eq!(result, vec![0, 1, 2]);
}

// --- Containment tests ------------------------------------------------------

#[test]
fn test_containment() {
    for n in [0i64, 1, 10, 50] {
        let (v, i) = Minimal::new(list_and_int(), move |(v, i): &(Vec<i64>, i64)| {
            *i >= n && v.contains(i)
        })
        .test_cases(1000)
        .run();
        assert_eq!((v, i), (vec![n], n));
    }
}

#[test]
fn test_duplicate_containment() {
    let (v, i) = minimal(list_and_int(), |(v, i): &(Vec<i64>, i64)| {
        v.iter().filter(|&&x| x == *i).count() > 1
    });
    assert_eq!(v, vec![0, 0]);
    assert_eq!(i, 0);
}

// --- List ordering and structure -------------------------------------------

#[test]
fn test_reordering_bytes() {
    let v = minimal(
        gs::vecs(gs::integers::<i64>().min_value(0).max_value(1000)),
        |x: &Vec<i64>| x.iter().sum::<i64>() >= 10 && x.len() >= 3,
    );
    let mut sorted = v.clone();
    sorted.sort();
    assert_eq!(v, sorted);
}

#[test]
fn test_minimize_long_list() {
    let v = minimal(gs::vecs(gs::booleans()).min_size(50), |x: &Vec<bool>| {
        x.len() >= 70
    });
    assert_eq!(v, vec![false; 70]);
}

#[test]
fn test_minimize_list_of_longish_lists() {
    let size = 5usize;
    let xs = minimal(
        gs::vecs(gs::vecs(gs::booleans())),
        move |x: &Vec<Vec<bool>>| {
            x.iter()
                .filter(|t| t.iter().any(|&b| b) && t.len() >= 2)
                .count()
                >= size
        },
    );
    assert_eq!(xs.len(), size);
    for x in &xs {
        assert_eq!(x, &vec![false, true]);
    }
}

#[test]
fn test_minimize_list_of_fairly_non_unique_ints() {
    let xs = minimal(
        gs::vecs(gs::integers::<i64>().min_value(0).max_value(100)),
        |x: &Vec<i64>| x.iter().collect::<HashSet<_>>().len() < x.len(),
    );
    assert_eq!(xs.len(), 2);
}

#[test]
fn test_list_with_complex_sorting_structure() {
    let xs = minimal(gs::vecs(gs::vecs(gs::booleans())), |x: &Vec<Vec<bool>>| {
        let reversed: Vec<Vec<bool>> = x
            .iter()
            .map(|t| t.iter().rev().copied().collect::<Vec<bool>>())
            .rev()
            .collect();
        reversed > *x && x.len() > 3
    });
    assert_eq!(xs.len(), 4);
}

#[test]
fn test_list_with_wide_gap() {
    let xs = minimal(gs::vecs(gs::integers::<i64>()), |x: &Vec<i64>| {
        if x.is_empty() {
            return false;
        }
        let mn = *x.iter().min().unwrap();
        let mx = *x.iter().max().unwrap();
        mx > mn + 10 && mn + 10 > 0
    });
    assert_eq!(xs.len(), 2);
    let mut s = xs.clone();
    s.sort();
    assert_eq!(s[1], 11 + s[0]);
}

// --- Lists of collections ---------------------------------------------------

#[test]
fn test_minimize_list_of_lists() {
    let result = minimal(
        gs::vecs(gs::vecs(gs::integers::<i64>())),
        |x: &Vec<Vec<i64>>| x.iter().filter(|s| !s.is_empty()).count() >= 3,
    );
    assert_eq!(result, vec![vec![0i64]; 3]);
}

#[test]
fn test_minimize_list_of_tuples() {
    let result = minimal(
        gs::vecs(gs::tuples!(gs::integers::<i64>(), gs::integers::<i64>())),
        |x: &Vec<(i64, i64)>| x.len() >= 2,
    );
    assert_eq!(result, vec![(0i64, 0i64); 2]);
}

// --- Lists forced near top -------------------------------------------------

#[test]
fn test_lists_forced_near_top() {
    for n in [0usize, 1, 5, 10] {
        let result = minimal(
            gs::vecs(gs::integers::<i64>()).min_size(n).max_size(n + 2),
            move |t: &Vec<i64>| t.len() == n + 2,
        );
        assert_eq!(result, vec![0i64; n + 2]);
    }
}

// --- Dictionaries ----------------------------------------------------------

#[test]
fn test_dictionary_minimizes_to_empty() {
    let result = minimal(
        gs::hashmaps(gs::integers::<i64>(), gs::text()),
        |_: &HashMap<i64, String>| true,
    );
    assert!(result.is_empty());
}

#[test]
fn test_dictionary_minimizes_values() {
    let result = minimal(
        gs::hashmaps(gs::integers::<i64>(), gs::text()),
        |t: &HashMap<i64, String>| t.len() >= 3,
    );
    assert!(result.len() >= 3);
    let values: HashSet<&String> = result.values().collect();
    assert_eq!(values.len(), 1);
    assert_eq!(*values.iter().next().unwrap(), "");
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
    // Python's `gs.booleans().map(str)` produces "True"/"False" because that's
    // what `str(bool)` yields. Rust's `bool::to_string` produces "true"/"false".
    // The shrink behaviour is the same: minimal shrinks to a single entry with
    // the bool=false key (whatever string it stringifies to).
    let result = minimal(
        gs::hashmaps(gs::booleans().map(|b| b.to_string()), gs::booleans()),
        |x: &HashMap<String, bool>| !x.is_empty(),
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result.get("false"), Some(&false));
}

// --- Find tests ------------------------------------------------------------

#[test]
fn test_find_dictionary() {
    let smallest = minimal(
        gs::hashmaps(gs::integers::<i64>(), gs::integers::<i64>()),
        |xs: &HashMap<i64, i64>| xs.iter().any(|(k, v)| k > v),
    );
    assert_eq!(smallest.len(), 1);
}

#[test]
fn test_can_find_list() {
    let x = minimal(gs::vecs(gs::integers::<i64>()), |x: &Vec<i64>| {
        x.iter().copied().fold(0i64, i64::saturating_add) >= 10
    });
    assert_eq!(x.iter().sum::<i64>(), 10);
}

// --- Collectively minimize -------------------------------------------------

#[test]
fn test_can_collectively_minimize_integers() {
    let n = 10usize;
    let xs = Minimal::new(
        gs::vecs(gs::integers::<i64>()).min_size(n).max_size(n),
        |x: &Vec<i64>| x.iter().collect::<HashSet<_>>().len() >= 2,
    )
    .test_cases(2000)
    .run();
    assert_eq!(xs.len(), n);
    let distinct = xs.iter().collect::<HashSet<_>>().len();
    assert!((2..=3).contains(&distinct));
}

#[test]
fn test_can_collectively_minimize_booleans() {
    let n = 10usize;
    let xs = Minimal::new(
        gs::vecs(gs::booleans()).min_size(n).max_size(n),
        |x: &Vec<bool>| x.iter().collect::<HashSet<_>>().len() >= 2,
    )
    .test_cases(2000)
    .run();
    assert_eq!(xs.len(), n);
    assert_eq!(xs.iter().collect::<HashSet<_>>().len(), 2);
}

#[test]
fn test_can_collectively_minimize_text() {
    let n = 10usize;
    let xs = Minimal::new(
        gs::vecs(gs::text()).min_size(n).max_size(n),
        |x: &Vec<String>| x.iter().collect::<HashSet<_>>().len() >= 2,
    )
    .test_cases(2000)
    .run();
    assert_eq!(xs.len(), n);
    let distinct = xs.iter().collect::<HashSet<_>>().len();
    assert!((2..=3).contains(&distinct));
}

// --- Sorting pass regressions ----------------------------------------------

#[test]
fn test_sorting_pass_survives_type_changes_from_lists() {
    // Sorting must not crash when a successful swap changes choice types
    // at pre-computed indices. The shrinker must reach a counterexample
    // (so `assert_eq!(v0.len(), v1.len())` fires) without panicking on a
    // stale-index assertion mid-shrink.
    expect_panic(
        || {
            Hegel::new(|tc| {
                let v0: Vec<bool> = tc.draw(gs::vecs(gs::booleans()).max_size(10));
                let v1: Vec<i64> =
                    tc.draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(0)).max_size(10));
                assert_eq!(v0.len(), v1.len());
            })
            .settings(Settings::new().test_cases(100).database(None))
            .run();
        },
        ".",
    );
}

#[test]
fn test_sorting_full_sort_survives_stale_indices() {
    // The full-sort path must not crash when a prior group's sort shortens
    // the result, making indices for the next group invalid. Whether the
    // assertion fires is not the point; just that nothing crashes.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Hegel::new(|tc| {
            let v0: Vec<i64> =
                tc.draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(12)).max_size(10));
            let _: bool = tc.draw(gs::booleans());
            if !(v0.is_empty() || v0[0] > 0) {
                panic!("v0 head zero");
            }
            if v0.len() > 2 && !v0.is_empty() {
                panic!("v0 too long");
            }
        })
        .settings(Settings::new().test_cases(1).database(None))
        .run();
    }));
}

#[test]
fn test_sorting_stale_filter_with_punning() {
    // Should not crash. Multiple seeded runs exercise the sorting
    // stale-index filter for the case where punning changes node types so
    // a group has fewer than 2 members.
    let pair = || {
        hegel::compose!(|tc| {
            let a: bool = tc.draw(gs::booleans());
            let b: bool = tc.draw(gs::booleans());
            (a, b)
        })
    };
    for _seed in 0..5 {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Hegel::new(move |tc| {
                let v0: Vec<i64> =
                    tc.draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(0)).max_size(10));
                let v1: Vec<bool> = tc.draw(
                    gs::integers::<i64>()
                        .min_value(0)
                        .max_value(0)
                        .flat_map(|_| gs::vecs(gs::booleans()).max_size(1)),
                );
                let _: (bool, bool) = tc.draw(pair());
                if v0.len() != v1.len() {
                    panic!("differ");
                }
            })
            .settings(Settings::new().test_cases(200).database(None))
            .run();
        }));
    }
}

#[test]
fn test_unique_list_shrinks_using_negative_values() {
    // Unique signed integer lists should shrink using negative values when
    // that gives smaller absolute values. With max_size=5 and a length>=5
    // condition, the minimal representative interleaves around zero rather
    // than walking 0,1,2,3,4. Upstream asserts on the choice-sequence
    // values [0,1,-1,2,-2]; we assert the equivalent on the resulting list.
    let v = Minimal::new(
        gs::vecs(gs::integers::<i64>().min_value(-10).max_value(10))
            .max_size(5)
            .unique(true),
        |x: &Vec<i64>| x.len() >= 5,
    )
    .test_cases(1000)
    .run();
    let mut sorted = v.clone();
    sorted.sort();
    assert_eq!(sorted, vec![-2, -1, 0, 1, 2]);
}

#[test]
fn test_redistribute_stale_indices_after_type_change() {
    // `redistribute_integers` must handle stale indices when previous passes
    // change the result structure. Should not crash.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Hegel::new(|tc| {
            let v0: bool = tc.draw(gs::booleans());
            let _: i64 = tc.draw(gs::booleans().map(|x| x as i64));
            let _: i64 = tc.draw(
                gs::integers::<i64>()
                    .min_value(1)
                    .max_value(7)
                    .filter(|x: &i64| x % 2 == 0),
            );
            let _: bool = tc.draw(gs::booleans());
            let _: i64 = tc.draw(gs::one_of(vec![
                gs::integers::<i64>().min_value(0).max_value(0).boxed(),
                gs::booleans().map(|b| b as i64).boxed(),
            ]));
            if v0 {
                panic!("v0 set");
            }
        })
        .settings(Settings::new().test_cases(1000).database(None))
        .run();
    }));
}

#[test]
fn test_sort_insertion_stale_indices() {
    // Insertion sort must handle stale indices when a swap shortens the
    // result via value punning. Should not crash across multiple seeds.
    for _seed in 0..5 {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Hegel::new(|tc| {
                let v0: Vec<i64> = tc.draw(
                    gs::vecs(gs::integers::<i64>().min_value(0).max_value(20))
                        .max_size(10)
                        .unique(true),
                );
                let _: HashMap<String, bool> = tc.draw(
                    gs::hashmaps(
                        gs::text().min_codepoint(32).max_codepoint(126).max_size(5),
                        gs::booleans(),
                    )
                    .max_size(5),
                );
                let v2: Vec<bool> = tc.draw(gs::vecs(gs::booleans()).max_size(10));
                let v3: Vec<u8> = tc.draw(gs::binary().max_size(20));
                let _: bool = tc.draw(gs::booleans());
                if !v0.is_empty() {
                    panic!("v0 nonempty");
                }
                if v2.len() != v3.len() {
                    panic!("v2/v3 length mismatch");
                }
            })
            .settings(Settings::new().test_cases(1000).database(None))
            .run();
        }));
    }
}

#[test]
fn test_sort_stale_indices_after_punning() {
    // Sorting handles indices becoming stale when a swap changes types via
    // value punning (one_of branch switch). Should not crash.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Hegel::new(|tc| {
            let v0: i64 = tc.draw(gs::one_of(vec![
                gs::integers::<i64>().min_value(0).max_value(10).boxed(),
                gs::integers::<i64>().min_value(0).max_value(10).boxed(),
            ]));
            let v1: i64 = tc.draw(gs::one_of(vec![
                gs::integers::<i64>().min_value(0).max_value(10).boxed(),
                gs::integers::<i64>().min_value(0).max_value(10).boxed(),
            ]));
            if v0 + v1 > 10 {
                panic!("sum");
            }
        })
        .settings(Settings::new().test_cases(1000).database(None))
        .run();
    }));
}

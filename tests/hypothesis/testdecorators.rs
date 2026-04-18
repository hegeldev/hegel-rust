//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_testdecorators.py.
//!
//! Tests that rely on Python-specific facilities are not ported:
//!
//! - `test_does_not_catch_interrupt_during_falsify` — uses Python
//!   `KeyboardInterrupt`, which has no Rust counterpart.
//! - `test_can_test_kwargs_only_methods` — uses Python `**kwargs`-only function
//!   signature syntax.
//! - `test_prints_on_failure_by_default`, `test_does_not_print_on_success`,
//!   `test_does_not_print_notes_if_all_succeed` — use `capture_out` and
//!   `assert_falsifying_output`, Python-specific output-capture helpers.
//! - `test_can_be_used_with_none_module` — tests Python `__module__ = None`
//!   attribute on functions.
//! - `test_prints_notes_once_on_failure` — checks Python exception `__notes__`.
//! - `test_given_usable_inline_on_lambdas` — applies `@given` to a Python lambda.
//! - `test_notes_high_filter_rates_in_unsatisfiable_error`,
//!   `test_notes_high_overrun_rates_in_unsatisfiable_error` — test Hypothesis
//!   `Unsatisfiable` error messages and `buffer_size_limit`, Python-only.
//! - `test_when_set_to_no_simplifies_runs_failing_example_twice` — uses
//!   `phases=no_shrink` and checks Python `__notes__` on the exception.
//! - `test_removing_an_element_from_a_non_unique_list` — uses `data()` draw
//!   inline; ported via `Hegel::new` directly instead.
//! - `TestCases.test_abs_non_negative_varargs`,
//!   `TestCases.test_abs_non_negative_varargs_kwargs`,
//!   `TestCases.test_abs_non_negative_varargs_kwargs_only` — rely on Python
//!   `*args`/`**kwargs` variadic dispatch.

use crate::common::utils::{assert_all_examples, expect_panic, find_any, minimal};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};

#[test]
fn test_int_addition_is_commutative() {
    assert_all_examples(
        gs::tuples!(gs::integers::<i64>(), gs::integers::<i64>()),
        |(x, y): &(i64, i64)| x.wrapping_add(*y) == y.wrapping_add(*x),
    );
}

#[test]
fn test_str_addition_is_commutative() {
    find_any(
        gs::tuples!(gs::text().min_size(1), gs::text().min_size(1)),
        |(x, y): &(String, String)| {
            let mut xy = x.clone();
            xy.push_str(y);
            let mut yx = y.clone();
            yx.push_str(x);
            xy != yx
        },
    );
}

#[test]
fn test_bytes_addition_is_commutative() {
    find_any(
        gs::tuples!(gs::binary().min_size(1), gs::binary().min_size(1)),
        |(x, y): &(Vec<u8>, Vec<u8>)| {
            let mut xy = x.clone();
            xy.extend_from_slice(y);
            let mut yx = y.clone();
            yx.extend_from_slice(x);
            xy != yx
        },
    );
}

#[test]
fn test_int_addition_is_associative() {
    assert_all_examples(
        gs::tuples!(
            gs::integers::<i64>(),
            gs::integers::<i64>(),
            gs::integers::<i64>()
        ),
        |(x, y, z): &(i64, i64, i64)| {
            // wrapping arithmetic mirrors Python's arbitrary-precision int semantics for the
            // commutativity and associativity properties (both hold in Z/2^64).
            x.wrapping_add(y.wrapping_add(*z)) == (x.wrapping_add(*y)).wrapping_add(*z)
        },
    );
}

#[test]
fn test_float_addition_is_associative() {
    find_any(
        gs::tuples!(
            gs::floats::<f64>().allow_nan(false).allow_infinity(false),
            gs::floats::<f64>().allow_nan(false).allow_infinity(false),
            gs::floats::<f64>().allow_nan(false).allow_infinity(false)
        ),
        |(x, y, z): &(f64, f64, f64)| x + (y + z) != (x + y) + z,
    );
}

#[test]
fn test_reversing_preserves_integer_addition() {
    assert_all_examples(gs::vecs(gs::integers::<i64>()), |xs: &Vec<i64>| {
        let sum: i64 = xs.iter().fold(0i64, |acc, &x| acc.wrapping_add(x));
        let rev_sum: i64 = xs.iter().rev().fold(0i64, |acc, &x| acc.wrapping_add(x));
        sum == rev_sum
    });
}

#[test]
fn test_still_minimizes_on_non_assertion_failures() {
    let result = minimal(gs::integers::<i64>(), |x: &i64| *x >= 10);
    assert_eq!(result, 10);
}

#[test]
fn test_integer_division_shrinks_positive_integers() {
    assert_all_examples(
        gs::integers::<i64>().filter(|x: &i64| *x > 0),
        |n: &i64| n / 2 < *n,
    );
}

// Tests from TestCases — ported as top-level fns (no class/self in Rust).

#[test]
fn test_abs_non_negative() {
    // Python integers don't overflow; use saturating_abs() so i64::MIN doesn't panic.
    assert_all_examples(gs::integers::<i64>(), |x: &i64| x.saturating_abs() >= 0);
}

#[test]
fn test_int_is_always_negative() {
    find_any(gs::integers::<i64>(), |x: &i64| *x >= 0);
}

#[test]
fn test_float_addition_cancels() {
    find_any(
        gs::tuples!(
            gs::floats::<f64>().allow_nan(false),
            gs::floats::<f64>().allow_nan(false)
        ),
        |(x, y): &(f64, f64)| x + (y - x) != *y,
    );
}

#[test]
fn test_can_be_given_keyword_args() {
    // @fails: find (x, name) with x > 0 and len(name) >= x.
    find_any(
        gs::tuples!(
            gs::integers::<i64>().min_value(1).max_value(3),
            gs::text()
        ),
        |(x, name): &(i64, String)| name.chars().count() as i64 >= *x,
    );
}

#[test]
fn test_one_of_produces_different_values() {
    find_any(
        gs::tuples!(
            gs::one_of(vec![
                gs::floats::<f64>().map(|f| f as i64 * 0 + i64::MAX).boxed(),
                gs::booleans().map(|_| i64::MIN).boxed(),
            ]),
            gs::one_of(vec![
                gs::floats::<f64>().map(|f| f as i64 * 0 + i64::MAX).boxed(),
                gs::booleans().map(|_| i64::MIN).boxed(),
            ])
        ),
        |(x, y): &(i64, i64)| x != y,
    );
}

#[test]
fn test_is_the_answer() {
    assert_all_examples(gs::just(42_i32), |x: &i32| *x == 42);
}

#[test]
fn test_integers_are_in_range() {
    assert_all_examples(
        gs::integers::<i64>().min_value(1).max_value(10),
        |x: &i64| 1 <= *x && *x <= 10,
    );
}

#[test]
fn test_integers_from_are_from() {
    assert_all_examples(gs::integers::<i64>().min_value(100), |x: &i64| *x >= 100);
}

#[test]
fn test_removing_an_element_from_a_unique_list() {
    assert_all_examples(
        gs::tuples!(
            gs::vecs(gs::integers::<i64>()).unique(true),
            gs::integers::<i64>()
        ),
        |(xs, y): &(Vec<i64>, i64)| {
            let mut xs = xs.clone();
            if let Some(pos) = xs.iter().position(|v| v == y) {
                xs.remove(pos);
            }
            !xs.contains(y)
        },
    );
}

#[test]
fn test_removing_an_element_from_a_non_unique_list() {
    // @fails: [1, 1] with y=1 — removing first 1 leaves 1 still in list.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Hegel::new(|tc| {
            let xs: Vec<i64> = tc.draw(&gs::vecs(gs::integers::<i64>()).min_size(2));
            let y: i64 = tc.draw(&gs::sampled_from(xs.clone()));
            let mut xs_mut = xs;
            if let Some(pos) = xs_mut.iter().position(|&v| v == y) {
                xs_mut.remove(pos);
            }
            assert!(!xs_mut.contains(&y));
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }));
    assert!(result.is_err());
}

#[test]
fn test_can_test_sets_sampled_from() {
    assert_all_examples(
        gs::hashsets(gs::sampled_from(vec![0_i64, 1, 2, 3, 4, 5, 6, 7, 8, 9])),
        |xs: &std::collections::HashSet<i64>| {
            xs.iter().all(|&x| (0..10).contains(&x))
        },
    );
}

#[test]
fn test_can_mix_sampling_with_generating() {
    find_any(
        gs::tuples!(
            gs::one_of(vec![
                gs::sampled_from(vec![1_i64, 2, 3]).boxed(),
                gs::text().map(|_| -1_i64).boxed(),
            ]),
            gs::one_of(vec![
                gs::sampled_from(vec![1_i64, 2, 3]).boxed(),
                gs::text().map(|_| -1_i64).boxed(),
            ])
        ),
        |(x, y): &(i64, i64)| (*x >= 1) != (*y >= 1),
    );
}

#[test]
fn test_can_find_large_sum_frozenset() {
    find_any(
        gs::hashsets(gs::integers::<i64>()),
        |xs: &std::collections::HashSet<i64>| xs.iter().sum::<i64>() >= 100,
    );
}

#[test]
fn test_can_sample_from_single_element() {
    assert_all_examples(gs::sampled_from(vec![1_i32]), |x: &i32| *x == 1);
}

#[test]
fn test_list_is_sorted() {
    find_any(gs::vecs(gs::integers::<i64>()), |xs: &Vec<i64>| {
        let mut sorted = xs.clone();
        sorted.sort();
        *xs != sorted
    });
}

#[test]
fn test_is_an_endpoint() {
    find_any(
        gs::floats::<f64>().min_value(1.0).max_value(2.0),
        |x: &f64| *x != 1.0 && *x != 2.0,
    );
}

#[test]
fn test_breaks_bounds() {
    for t in [1_i64, 10, 100, 1000] {
        find_any(gs::integers::<i64>(), move |x: &i64| *x >= t);
    }
}

#[test]
fn test_is_ascii() {
    // @fails_with(UnicodeEncodeError): text() can produce non-ASCII characters.
    find_any(gs::text(), |x: &String| !x.is_ascii());
}

#[test]
fn test_is_not_ascii() {
    // @fails: the test asserts x is not ascii, failing when x IS ascii.
    find_any(gs::text(), |x: &String| x.is_ascii());
}

#[test]
fn test_can_find_string_with_duplicates() {
    find_any(gs::text().min_size(2), |s: &String| {
        let chars: Vec<char> = s.chars().collect();
        let unique: std::collections::HashSet<char> = chars.iter().copied().collect();
        unique.len() < chars.len()
    });
}

#[test]
fn test_has_ascii() {
    // @fails: the test asserts at least one char is ASCII; fails when none are.
    let ascii_chars = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ \t\n";
    find_any(gs::text().min_size(1), move |x: &String| {
        !x.is_empty() && !x.chars().any(|c| ascii_chars.contains(c))
    });
}

#[test]
fn test_can_derandomize() {
    use std::sync::{Arc, Mutex};

    let values1: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(Vec::new()));
    let values2: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(Vec::new()));

    let run = |values: Arc<Mutex<Vec<i64>>>| {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            Hegel::new(move |tc| {
                let x: i64 = tc.draw(&gs::integers::<i64>());
                values.lock().unwrap().push(x);
                assert!(x > 0);
            })
            .settings(Settings::new().derandomize(true).database(None))
            .run();
        }));
    };

    run(Arc::clone(&values1));
    run(Arc::clone(&values2));

    let v1 = values1.lock().unwrap();
    let v2 = values2.lock().unwrap();
    assert!(!v1.is_empty());
    assert_eq!(*v1, *v2);
}

#[test]
fn test_can_run_without_database() {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Hegel::new(|_tc| {
            panic!("AssertionError");
        })
        .settings(Settings::new().database(None))
        .run();
    }));
    assert!(result.is_err());
}

#[test]
fn test_can_run_with_database_in_thread() {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let results: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

    {
        let results = Arc::clone(&results);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Hegel::new(|_tc| panic!("ValueError"))
                .settings(Settings::new().database(None))
                .run();
        }));
        if res.is_err() {
            results.lock().unwrap().push("success");
        }
    }
    assert_eq!(*results.lock().unwrap(), vec!["success"]);

    {
        let results = Arc::clone(&results);
        let handle = thread::spawn(move || {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Hegel::new(|_tc| panic!("ValueError"))
                    .settings(Settings::new().database(None))
                    .run();
            }));
            if res.is_err() {
                results.lock().unwrap().push("success");
            }
        });
        handle.join().unwrap();
    }
    assert_eq!(*results.lock().unwrap(), vec!["success", "success"]);
}

#[test]
fn test_can_call_an_argument_f() {
    // Regression: argument named `f` should not conflict with test framework internals.
    assert_all_examples(gs::integers::<i64>(), |_f: &i64| true);
}

#[test]
fn test_named_tuples_are_of_right_type() {
    #[derive(Debug)]
    struct Litter {
        kitten1: i64,
        kitten2: i64,
    }
    assert_all_examples(
        gs::tuples!(gs::integers::<i64>(), gs::integers::<i64>())
            .map(|(k1, k2)| Litter { kitten1: k1, kitten2: k2 }),
        |litter: &Litter| litter.kitten1 >= i64::MIN && litter.kitten2 >= i64::MIN,
    );
}

#[test]
fn test_fails_in_reify() {
    // @fails_with(AttributeError): .map() closure panics, propagating the error.
    expect_panic(
        || {
            Hegel::new(|tc| {
                let _: i64 = tc.draw(&gs::integers::<i64>().map(|_x| panic!("AttributeError")));
            })
            .settings(
                Settings::new()
                    .test_cases(1)
                    .database(None)
                    .suppress_health_check([hegel::HealthCheck::TooSlow]),
            )
            .run();
        },
        "AttributeError",
    );
}

#[test]
fn test_a_text() {
    assert_all_examples(gs::text().alphabet("a"), |x: &String| {
        x.chars().all(|c| c == 'a')
    });
}

#[test]
fn test_empty_text() {
    // text("") in Python generates only empty strings; max_size(0) is the Rust equivalent
    // since an empty alphabet causes a server InvalidArgument error.
    assert_all_examples(gs::text().max_size(0), |x: &String| x.is_empty());
}

#[test]
fn test_mixed_text() {
    assert_all_examples(gs::text().alphabet("abcdefg"), |x: &String| {
        x.chars().all(|c| "abcdefg".contains(c))
    });
}

#[test]
fn test_filtered_values_satisfy_condition() {
    assert_all_examples(
        gs::integers::<i64>().filter(|x: &i64| x % 4 == 0),
        |i: &i64| i % 4 == 0,
    );
}

#[test]
fn test_can_map_nameless() {
    // nameless_const(2) returns a partial that always returns 2 regardless of input.
    assert_all_examples(
        gs::hashsets(gs::booleans()).map(|_set| 2_i32),
        |x: &i32| *x == 2,
    );
}

#[test]
fn test_can_flatmap_nameless() {
    // integers(0, 10).flatmap(nameless_const(just(3))) always yields 3.
    assert_all_examples(
        gs::integers::<i64>()
            .min_value(0)
            .max_value(10)
            .flat_map(|_x| gs::just(3_i32)),
        |x: &i32| *x == 3,
    );
}

#[test]
fn test_empty_lists() {
    assert_all_examples(
        gs::vecs(gs::integers::<i64>()).max_size(0),
        |xs: &Vec<i64>| xs.is_empty(),
    );
}

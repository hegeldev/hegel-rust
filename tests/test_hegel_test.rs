// Compile-time error behaviour of #[hegel::test] (duplicate #[test], zero or
// two parameters) lives in tests/compile/fail/hegel_test_*.rs, driven by
// `trybuild`.
//
// Below: ported tests merged from hypothesis/{testdecorators, flakiness,
// nocover_baseexception, nocover_nesting, nocover_limits,
// nocover_unusual_settings_configs, pytest_runs, nocover_completion,
// core} and pbtkit/core, each wrapped in its own private module.

mod common;

use common::not_supported_on_native;
use common::utils::expect_panic;
use hegel::TestCase;
use hegel::generators as gs;

#[not_supported_on_native]
#[test]
fn test_text_invalid_codec_panics() {
    expect_panic(
        || {
            hegel::Hegel::new(|tc: hegel::TestCase| {
                tc.draw(gs::text().codec("not-a-real-codec"));
            })
            .run();
        },
        "not-a-real-codec",
    );
}

#[hegel::test]
fn test_basic_usage(tc: TestCase) {
    tc.draw(gs::booleans());
}

#[not_supported_on_native]
#[hegel::test]
fn test_characters(tc: TestCase) {
    let c: char = tc.draw(gs::characters());
    assert!(c.len_utf8() > 0);
}

#[hegel::test(test_cases = 10)]
fn test_with_named_arg(tc: TestCase) {
    tc.draw(gs::booleans());
}

#[hegel::test(hegel::Settings::new().test_cases(10))]
fn test_with_positional_settings(tc: TestCase) {
    tc.draw(gs::booleans());
}

#[hegel::test(hegel::Settings::new(), test_cases = 10)]
fn test_with_positional_and_named(tc: TestCase) {
    tc.draw(gs::booleans());
}

#[hegel::test(test_cases = 10, derandomize = true)]
fn test_with_multiple_named_args(tc: TestCase) {
    tc.draw(gs::booleans());
}

#[hegel::test(seed = Some(42))]
fn test_with_seed(tc: TestCase) {
    tc.draw(gs::booleans());
}

#[test]
fn test_database_persists_failing_examples() {
    let db_path = tempfile::tempdir().unwrap();
    let db_str = db_path.path().to_str().unwrap().to_string();

    assert!(std::fs::read_dir(db_path.path()).unwrap().next().is_none());

    expect_panic(
        || {
            hegel::Hegel::new(|_tc: hegel::TestCase| {
                panic!("");
            })
            .settings(hegel::Settings::new().database(Some(db_str)))
            .__database_key("test_database_persists".to_string())
            .run();
        },
        "Property test failed",
    );

    let entries: Vec<_> = std::fs::read_dir(db_path.path()).unwrap().collect();
    assert!(!entries.is_empty());
}

mod testdecorators {
    use super::common::utils::{assert_all_examples, expect_panic, find_any, minimal};
    #[allow(unused_imports)]
    use super::not_supported_on_native;
    use hegel::generators::{self as gs, Generator};
    use hegel::{Hegel, Settings};

    #[test]
    fn test_int_addition_is_commutative() {
        assert_all_examples(
            gs::tuples!(gs::integers::<i64>(), gs::integers::<i64>()),
            |(x, y): &(i64, i64)| x.wrapping_add(*y) == y.wrapping_add(*x),
        );
    }

    #[not_supported_on_native]
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

    #[not_supported_on_native]
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
        assert_all_examples(gs::integers::<i64>().filter(|x: &i64| *x > 0), |n: &i64| {
            n / 2 < *n
        });
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

    #[not_supported_on_native]
    #[test]
    fn test_can_be_given_keyword_args() {
        // @fails: find (x, name) with x > 0 and len(name) >= x.
        find_any(
            gs::tuples!(gs::integers::<i64>().min_value(1).max_value(3), gs::text()),
            |(x, name): &(i64, String)| name.chars().count() as i64 >= *x,
        );
    }

    #[test]
    fn test_one_of_produces_different_values() {
        find_any(
            gs::tuples!(
                gs::one_of(vec![
                    gs::floats::<f64>().map(|_| i64::MAX).boxed(),
                    gs::booleans().map(|_| i64::MIN).boxed(),
                ]),
                gs::one_of(vec![
                    gs::floats::<f64>().map(|_| i64::MAX).boxed(),
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
                let xs: Vec<i64> = tc.draw(gs::vecs(gs::integers::<i64>()).min_size(2));
                let y: i64 = tc.draw(gs::sampled_from(xs.clone()));
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
            |xs: &std::collections::HashSet<i64>| xs.iter().all(|&x| (0..10).contains(&x)),
        );
    }

    #[not_supported_on_native]
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
        // HashSet iteration order is randomised per instance, so `iter().sum::<i64>()`
        // is order-dependent under overflow: in debug builds, a partial-sum overflow
        // panics, which can flip the predicate between runs with identical choices
        // (one iteration order triggers it, another doesn't). `wrapping_add` is
        // associative and commutative mod 2^64, so the final value is the same
        // regardless of order.
        find_any(
            gs::hashsets(gs::integers::<i64>()),
            |xs: &std::collections::HashSet<i64>| {
                xs.iter().copied().fold(0_i64, i64::wrapping_add) >= 100
            },
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

    #[not_supported_on_native]
    #[test]
    fn test_is_ascii() {
        // @fails_with(UnicodeEncodeError): text() can produce non-ASCII characters.
        find_any(gs::text(), |x: &String| !x.is_ascii());
    }

    #[not_supported_on_native]
    #[test]
    fn test_is_not_ascii() {
        // @fails: the test asserts x is not ascii, failing when x IS ascii.
        find_any(gs::text(), |x: &String| x.is_ascii());
    }

    #[not_supported_on_native]
    #[test]
    fn test_can_find_string_with_duplicates() {
        find_any(gs::text().min_size(2), |s: &String| {
            let chars: Vec<char> = s.chars().collect();
            let unique: std::collections::HashSet<char> = chars.iter().copied().collect();
            unique.len() < chars.len()
        });
    }

    #[not_supported_on_native]
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
                    let x: i64 = tc.draw(gs::integers::<i64>());
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
        #[allow(dead_code)]
        struct Litter {
            kitten1: i64,
            kitten2: i64,
        }
        assert_all_examples(
            gs::tuples!(gs::integers::<i64>(), gs::integers::<i64>()).map(|(k1, k2)| Litter {
                kitten1: k1,
                kitten2: k2,
            }),
            |_: &Litter| true,
        );
    }

    #[test]
    fn test_fails_in_reify() {
        // @fails_with(AttributeError): .map() closure panics, propagating the error.
        expect_panic(
            || {
                Hegel::new(|tc| {
                    let _: i64 = tc.draw(gs::integers::<i64>().map(|_x| panic!("AttributeError")));
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

    #[not_supported_on_native]
    #[test]
    fn test_a_text() {
        assert_all_examples(gs::text().alphabet("a"), |x: &String| {
            x.chars().all(|c| c == 'a')
        });
    }

    #[not_supported_on_native]
    #[test]
    fn test_empty_text() {
        // text("") in Python generates only empty strings; max_size(0) is the Rust equivalent
        // since an empty alphabet causes a server InvalidArgument error.
        assert_all_examples(gs::text().max_size(0), |x: &String| x.is_empty());
    }

    #[not_supported_on_native]
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
        assert_all_examples(gs::hashsets(gs::booleans()).map(|_set| 2_i32), |x: &i32| {
            *x == 2
        });
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
}

mod flakiness {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use super::common::utils::expect_panic;
    use hegel::generators as gs;
    use hegel::{Hegel, Settings, TestCase, Verbosity};

    #[test]
    fn test_fails_only_once_is_flaky() {
        let first_call = Arc::new(AtomicBool::new(true));
        expect_panic(
            move || {
                Hegel::new(move |tc: TestCase| {
                    let _: i64 = tc.draw(gs::integers());
                    if first_call.swap(false, Ordering::SeqCst) {
                        panic!("Nope");
                    }
                })
                .settings(Settings::new().database(None))
                .run();
            },
            "Flaky test detected",
        );
    }

    #[test]
    fn test_gives_flaky_error_if_assumption_is_flaky() {
        let seen: Arc<Mutex<HashSet<i64>>> = Arc::new(Mutex::new(HashSet::new()));
        expect_panic(
            move || {
                Hegel::new(move |tc: TestCase| {
                    let s: i64 = tc.draw(gs::integers());
                    let is_unseen = !seen.lock().unwrap().contains(&s);
                    tc.assume(is_unseen);
                    seen.lock().unwrap().insert(s);
                    panic!("AssertionError");
                })
                .settings(Settings::new().verbosity(Verbosity::Quiet).database(None))
                .run();
            },
            "Flaky test detected",
        );
    }

    #[test]
    fn test_does_not_attempt_to_shrink_flaky_errors() {
        let values: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(Vec::new()));
        expect_panic(
            move || {
                Hegel::new(move |tc: TestCase| {
                    let x: i64 = tc.draw(gs::integers());
                    // Lock is released before assert fires, so no mutex poisoning.
                    values.lock().unwrap().push(x);
                    let n = values.lock().unwrap().len();
                    assert!(n != 1);
                })
                .settings(Settings::new().database(None))
                .run();
            },
            "Flaky test detected",
        );
    }
}

mod nocover_baseexception {
    use super::common::utils::expect_panic;
    use hegel::generators as gs;
    use hegel::{Hegel, Settings};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_exception_propagates_fine() {
        expect_panic(
            || {
                Hegel::new(|tc| {
                    let _x: i64 = tc.draw(gs::integers());
                    panic!("test_exception_propagates_fine_payload");
                })
                .settings(Settings::new().test_cases(100).database(None))
                .run();
            },
            "test_exception_propagates_fine_payload",
        );
    }

    #[test]
    fn test_exception_propagates_fine_from_strategy() {
        expect_panic(
            || {
                Hegel::new(|tc| {
                    // black_box keeps the macro's trailing stop_span/result
                    // reachable from the compiler's view. Python's original
                    // uses a dead `return draw(none())` for the same reason.
                    let _: () = tc.draw(&hegel::compose!(|_tc| {
                        if std::hint::black_box(true) {
                            panic!("test_exception_propagates_fine_from_strategy_payload");
                        }
                    }));
                })
                .settings(Settings::new().test_cases(100).database(None))
                .run();
            },
            "test_exception_propagates_fine_from_strategy_payload",
        );
    }

    #[test]
    fn test_baseexception_no_rerun_no_flaky() {
        let runs = Arc::new(AtomicUsize::new(0));
        let runs_outer = Arc::clone(&runs);
        expect_panic(
            move || {
                Hegel::new(move |tc| {
                    let _x: i64 = tc.draw(gs::integers());
                    let r = runs_outer.fetch_add(1, Ordering::SeqCst) + 1;
                    if r == 3 {
                        panic!("baseexception_no_rerun_payload");
                    }
                })
                .settings(Settings::new().test_cases(100).database(None))
                .run();
            },
            "Flaky test detected",
        );
    }

    #[test]
    fn test_baseexception_in_strategy_no_rerun_no_flaky() {
        let runs = Arc::new(AtomicUsize::new(0));
        let runs_outer = Arc::clone(&runs);
        expect_panic(
            move || {
                Hegel::new(move |tc| {
                    let runs_gen = Arc::clone(&runs_outer);
                    let _: i64 = tc.draw(&hegel::compose!(|tc| {
                        let r = runs_gen.fetch_add(1, Ordering::SeqCst) + 1;
                        if r == 3 {
                            panic!("baseexception_in_strategy_payload");
                        }
                        tc.draw(gs::integers::<i64>())
                    }));
                })
                .settings(Settings::new().test_cases(100).database(None))
                .run();
            },
            "Flaky test detected",
        );
    }
}

mod nocover_nesting {
    use super::common::utils::expect_panic;
    use hegel::generators as gs;
    use hegel::{HealthCheck, Hegel, Settings};

    #[test]
    fn test_nesting_1() {
        // Each outer test case runs an *entire* inner `Hegel::new(...).run()` to
        // exhaustion before yielding back. With 100 inner cases and the system
        // under concurrent load (other test binaries running in the same
        // `cargo test`), one outer iteration can comfortably exceed the
        // 200 ms / case TooSlow threshold — that's the point of this test, not
        // a bug. Suppress the check on both runners. Mirrors the upstream
        // Python `suppress_health_check=[HealthCheck.nested_given]` (Hegel
        // doesn't have a `nested_given` variant, so TooSlow + FilterTooMuch is
        // the equivalent set: nested_given covered both shapes upstream).
        let outer_settings = Settings::new()
            .test_cases(5)
            .database(None)
            .suppress_health_check([HealthCheck::TooSlow, HealthCheck::FilterTooMuch]);
        Hegel::new(|tc| {
            let x: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
            expect_panic(
                move || {
                    Hegel::new(move |tc_inner| {
                        let y: i64 = tc_inner.draw(gs::integers::<i64>());
                        if y >= x {
                            panic!("inner_panic");
                        }
                    })
                    .settings(
                        Settings::new()
                            .test_cases(100)
                            .database(None)
                            .suppress_health_check([
                                HealthCheck::TooSlow,
                                HealthCheck::FilterTooMuch,
                            ]),
                    )
                    .run();
                },
                "inner_panic",
            );
        })
        .settings(outer_settings)
        .run();
    }
}

mod nocover_limits {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use hegel::generators as gs;
    use hegel::{Hegel, Settings, TestCase};

    #[test]
    fn test_max_examples_are_respected() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&counter);
        Hegel::new(move |tc: TestCase| {
            tc.draw(gs::integers::<i64>());
            c.fetch_add(1, Ordering::Relaxed);
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
        assert_eq!(counter.load(Ordering::Relaxed), 100);
    }
}

mod nocover_unusual_settings_configs {
    use hegel::generators as gs;
    use hegel::{HealthCheck, Hegel, Settings, TestCase, Verbosity};

    #[test]
    fn test_single_example() {
        Hegel::new(|tc: TestCase| {
            let _: i64 = tc.draw(gs::integers());
        })
        .settings(Settings::new().test_cases(1).database(None))
        .run();
    }

    #[test]
    fn test_hard_to_find_single_example() {
        Hegel::new(|tc: TestCase| {
            let n: i64 = tc.draw(gs::integers());
            // Numbers are arbitrary, just deliberately unlikely to hit this too soon.
            tc.assume(n.rem_euclid(50) == 11);
        })
        .settings(
            Settings::new()
                .test_cases(1)
                .database(None)
                .suppress_health_check([HealthCheck::FilterTooMuch, HealthCheck::TooSlow])
                .verbosity(Verbosity::Debug),
        )
        .run();
    }
}

mod pytest_runs {
    use hegel::generators::{self as gs};
    use hegel::{Hegel, Settings};

    #[test]
    fn test_ints_are_ints() {
        Hegel::new(|tc| {
            tc.draw(gs::integers::<i64>());
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_ints_are_floats() {
        // @fails in the original: `isinstance(x, float)` is always False for ints.
        // Rust's type system makes the isinstance check a no-op, so the faithful
        // port is a guaranteed-failing property; we verify Hegel reports failure.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Hegel::new(|tc| {
                tc.draw(gs::integers::<i64>());
                panic!("x is not a float");
            })
            .settings(Settings::new().test_cases(100).database(None))
            .run();
        }));
        assert!(result.is_err());
    }
}

mod nocover_completion {
    use hegel::Hegel;

    #[test]
    fn test_never_draw_anything() {
        Hegel::new(|_tc| {}).run();
    }
}

mod hypothesis_core {
    use super::common::utils::minimal;
    #[allow(unused_imports)]
    use super::not_supported_on_native;
    use hegel::generators as gs;
    use hegel::{Hegel, Settings};

    #[test]
    fn test_given_shrinks_pytest_helper_errors() {
        let value = minimal(gs::integers::<i64>(), |x: &i64| *x > 100);
        assert_eq!(value, 101);
    }

    #[test]
    fn test_can_find_with_db_eq_none() {
        Hegel::new(|tc| {
            let _: i64 = tc.draw(gs::integers::<i64>());
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    // test_characters_codec parametrize rows: each row drives one assertion that
    // the codec / max_codepoint / categories / exclude_categories constraint is
    // honoured by every drawn character. The Python original asserts the full
    // codec round-trip (`example.encode(codec).decode(codec) == example`); Rust
    // `char` is always a Unicode scalar, so for "ascii" the round-trip reduces to
    // `c.is_ascii()` and for "utf-8" it is trivially true.

    #[not_supported_on_native]
    #[test]
    fn test_characters_codec_ascii_unbounded() {
        Hegel::new(|tc| {
            let c: char = tc.draw(gs::characters().codec("ascii"));
            assert!(c.is_ascii());
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[not_supported_on_native]
    #[test]
    fn test_characters_codec_ascii_max_codepoint_128() {
        Hegel::new(|tc| {
            let c: char = tc.draw(gs::characters().codec("ascii").max_codepoint(128));
            assert!(c.is_ascii());
            assert!(c as u32 <= 128);
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[not_supported_on_native]
    #[test]
    fn test_characters_codec_ascii_max_codepoint_100() {
        Hegel::new(|tc| {
            let c: char = tc.draw(gs::characters().codec("ascii").max_codepoint(100));
            assert!(c.is_ascii());
            assert!(c as u32 <= 100);
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[not_supported_on_native]
    #[test]
    fn test_characters_codec_utf8_unbounded() {
        Hegel::new(|tc| {
            let _: char = tc.draw(gs::characters().codec("utf-8"));
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[not_supported_on_native]
    #[test]
    fn test_characters_codec_utf8_exclude_cs() {
        // Rust `char` already excludes the surrogate range by construction, so
        // exclude_categories=["Cs"] is a no-op for the round-trip property; we
        // still exercise the schema path to make sure it doesn't reject.
        Hegel::new(|tc| {
            let _: char = tc.draw(gs::characters().codec("utf-8").exclude_categories(&["Cs"]));
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }
}

mod pbtkit_core {
    use super::common::utils::expect_panic;
    use hegel::generators::{self as gs, Generator};
    use hegel::{Hegel, Settings, TestCase};

    #[hegel::test]
    fn test_test_cases_satisfy_preconditions(tc: TestCase) {
        let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(10));
        tc.assume(n != 0);
        assert!(n != 0);
    }

    #[hegel::test]
    fn test_can_choose_full_64_bits(tc: TestCase) {
        // pbtkit's `tc.choice(2**64 - 1)` samples the full unsigned 64-bit
        // range. hegel-rust's typed equivalent is `gs::integers::<u64>()`.
        let _: u64 = tc.draw(gs::integers::<u64>());
    }

    #[test]
    fn test_flat_map_core() {
        Hegel::new(|tc| {
            let (m, n): (i64, i64) = tc.draw(
                gs::integers::<i64>()
                    .min_value(0)
                    .max_value(5)
                    .flat_map(|m: i64| {
                        gs::tuples!(
                            gs::just(m),
                            gs::integers::<i64>().min_value(m).max_value(m + 10),
                        )
                    }),
            );
            assert!(m <= n && n <= m + 10);
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_filter_core() {
        Hegel::new(|tc| {
            let n: i64 = tc.draw(
                gs::integers::<i64>()
                    .min_value(0)
                    .max_value(10)
                    .filter(|n: &i64| n % 2 == 0),
            );
            assert!(n % 2 == 0);
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_one_of_empty_core() {
        // pbtkit raises Unsatisfiable when drawing from one_of() with no
        // alternatives; hegel-rust panics at construction.
        expect_panic(
            || {
                let empty: Vec<gs::BoxedGenerator<i32>> = vec![];
                gs::one_of(empty);
            },
            "one_of requires at least one generator",
        );
    }

    #[test]
    fn test_one_of_single_core() {
        Hegel::new(|tc| {
            let n: i64 = tc.draw(hegel::one_of!(
                gs::integers::<i64>().min_value(0).max_value(10)
            ));
            assert!((0..=10).contains(&n));
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_sampled_from_core() {
        Hegel::new(|tc| {
            let v: &'static str = tc.draw(gs::sampled_from(vec!["a", "b", "c"]));
            assert!(matches!(v, "a" | "b" | "c"));
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_sampled_from_empty_core() {
        expect_panic(
            || {
                let empty: Vec<i32> = vec![];
                gs::sampled_from(empty);
            },
            "cannot be empty",
        );
    }

    #[test]
    fn test_sampled_from_single_core() {
        Hegel::new(|tc| {
            let v: &'static str = tc.draw(gs::sampled_from(vec!["only"]));
            assert_eq!(v, "only");
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_just_core() {
        Hegel::new(|tc| {
            let v: i64 = tc.draw(gs::just(42_i64));
            assert_eq!(v, 42);
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_map_core() {
        Hegel::new(|tc| {
            let n: i64 = tc.draw(
                gs::integers::<i64>()
                    .min_value(0)
                    .max_value(5)
                    .map(|n: i64| n * 2),
            );
            assert!(n % 2 == 0);
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_weighted_forced_true() {
        // pbtkit: `tc.weighted(1.0)` deterministically returns True. hegel-rust
        // has no `tc.weighted(p)` public API, but `gs::booleans().map(|_| true)`
        // combined with a forced-to-true predicate produces the same shape:
        // the test body unconditionally panics.
        expect_panic(
            || {
                Hegel::new(|tc| {
                    if tc.draw(gs::just(true)) {
                        tc.draw(gs::integers::<i64>().min_value(0).max_value(1));
                        panic!("forced-true branch reached");
                    }
                })
                .settings(Settings::new().test_cases(1).database(None))
                .run();
            },
            "forced-true branch reached",
        );
    }

    // Port of pbtkit/tests/test_core.py::test_error_on_too_strict_precondition.
}

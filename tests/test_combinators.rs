#![cfg_attr(feature = "native", allow(unused_imports, dead_code))]

mod common;

use common::not_supported_on_native;
use common::utils::{assert_all_examples, expect_panic, find_any};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings, TestCase};

#[hegel::test]
fn test_sampled_from_returns_element_from_list(tc: TestCase) {
    let options = tc.draw(gs::vecs(gs::integers::<i32>()).min_size(1));
    let value = tc.draw(gs::sampled_from(options.clone()));
    assert!(options.contains(&value));
}

#[not_supported_on_native]
#[hegel::test]
fn test_sampled_from_strings(tc: TestCase) {
    let options = tc.draw(gs::vecs(gs::text()).min_size(1));
    let value = tc.draw(gs::sampled_from(options.clone()));
    assert!(options.contains(&value));
}

#[test]
fn test_optional_can_generate_some() {
    find_any(gs::optional(gs::integers::<i32>()), |v| v.is_some());
}

#[test]
fn test_optional_can_generate_none() {
    find_any(gs::optional(gs::integers::<i32>()), |v| v.is_none());
}

#[hegel::test]
fn test_optional_respects_inner_generator_bounds(tc: TestCase) {
    let value = tc.draw(gs::optional(gs::integers().min_value(10).max_value(20)));
    if let Some(n) = value {
        assert!((10..=20).contains(&n));
    }
}

// Exercises the non-basic OptionalGenerator::do_draw path (flat_map produces a
// non-basic generator, so optional falls back to compositional generation
// rather than schema-based generation).
#[hegel::test]
fn test_optional_with_non_basic_inner(tc: TestCase) {
    let inner = gs::integers::<i32>()
        .min_value(1)
        .max_value(5)
        .flat_map(|n| gs::just(n * 10));
    let value = tc.draw(gs::optional(inner));
    if let Some(n) = value {
        assert!([10, 20, 30, 40, 50].contains(&n));
    }
}

#[hegel::test]
fn test_one_of_returns_value_from_one_generator(tc: TestCase) {
    let value = tc.draw(hegel::one_of!(
        gs::integers().min_value(0).max_value(10),
        gs::integers().min_value(100).max_value(110),
    ));
    assert!((0..=10).contains(&value) || (100..=110).contains(&value));
}

#[not_supported_on_native]
#[hegel::test]
fn test_one_of_with_different_types_via_map(tc: TestCase) {
    let value = tc.draw(hegel::one_of!(
        gs::integers::<i32>()
            .min_value(0)
            .max_value(100)
            .map(|n| format!("number: {}", n)),
        gs::text()
            .min_size(1)
            .max_size(10)
            .map(|s| format!("text: {}", s)),
    ));
    assert!(value.starts_with("number: ") || value.starts_with("text: "));
}

#[hegel::test]
fn test_one_of_many(tc: TestCase) {
    let value = tc.draw(gs::one_of((0..10).map(|i| gs::just(i).boxed())));
    assert!((0..10).contains(&value));
}

#[not_supported_on_native]
#[hegel::test]
fn test_flat_map(tc: TestCase) {
    let value = tc.draw(
        gs::integers::<usize>()
            .min_value(1)
            .max_value(5)
            .flat_map(|len| gs::text().min_size(len).max_size(len)),
    );
    assert!(!value.is_empty());
    assert!(value.chars().count() <= 5);
}

#[hegel::test]
fn test_filter(tc: TestCase) {
    let value = tc.draw(
        gs::integers::<i32>()
            .min_value(0)
            .max_value(100)
            .filter(|n| n % 2 == 0),
    );
    assert!(value % 2 == 0);
    assert!((0..=100).contains(&value));
}

#[hegel::test]
fn test_boxed_generator_clone(tc: TestCase) {
    let gen1 = gs::integers::<i32>().min_value(0).max_value(10).boxed();
    let gen2 = gen1.clone();
    let v1 = tc.draw(gen1);
    let v2 = tc.draw(gen2);
    assert!((0..=10).contains(&v1));
    assert!((0..=10).contains(&v2));
}

#[hegel::test]
fn test_boxed_generator_double_boxed(tc: TestCase) {
    // Calling .boxed() on an already-boxed generator should not re-wrap
    let gen1 = gs::integers::<i32>().min_value(0).max_value(10).boxed();
    let gen2 = gen1.boxed();
    let value = tc.draw(gen2);
    assert!((0..=10).contains(&value));
}

#[hegel::test]
fn test_sampled_from_accepts_slice(tc: TestCase) {
    // Pass a borrowed slice directly — no `.to_vec()` or `.iter().collect()` needed.
    const NAMES: &[&str] = &["alice", "bob", "carol"];
    let value = tc.draw(gs::sampled_from(NAMES));
    assert!(NAMES.contains(&value));
}

#[hegel::test]
fn test_sampled_from_accepts_array(tc: TestCase) {
    // Pass a borrowed fixed-size array — coerces to &[T].
    let options = [1i32, 2, 3, 4, 5];
    let value = tc.draw(gs::sampled_from(&options));
    assert!(options.contains(&value));
}

#[hegel::test]
fn test_sampled_from_non_primitive(tc: TestCase) {
    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct Point {
        x: i32,
        y: i32,
    }

    let options = vec![
        Point { x: 1, y: 2 },
        Point { x: 3, y: 4 },
        Point { x: 5, y: 6 },
    ];
    let value = tc.draw(gs::sampled_from(options.clone()));
    assert!(options.contains(&value));
}

#[hegel::test]
fn test_optional_mapped(tc: TestCase) {
    let value = tc.draw(gs::optional(
        gs::integers::<i32>()
            .min_value(0)
            .max_value(100)
            .map(|n| format!("value: {}", n)),
    ));
    if let Some(s) = value {
        assert!(s.starts_with("value: "));
    }
}

#[hegel::test]
fn test_draw_silent_non_debug(tc: TestCase) {
    // Closure is not Debug, so this can only work with draw_silent
    let f = tc.draw_silent(
        gs::integers::<i32>()
            .min_value(0)
            .max_value(1000)
            .map(|n| move |x: i32| x + n),
    );
    assert_eq!(f(10), 10 + f(0));
}

#[test]
fn test_optional_mapped_find_any() {
    find_any(
        gs::optional(gs::integers::<i32>().map(|n| n.wrapping_mul(2))),
        |v| v.is_some(),
    );

    find_any(
        gs::optional(gs::integers::<i32>().map(|n| n.wrapping_mul(2))),
        |v| v.is_none(),
    );
}

// Tests for enumerate_values / filtered sampled_from optimization.

/// A rare value (x == 0) should always be found via the enumerate_values fallback.
#[test]
fn test_sampled_from_filter_rare_value() {
    assert_all_examples(
        gs::sampled_from((0..100_i64).collect::<Vec<i64>>()).filter(|x: &i64| *x == 0),
        |x: &i64| *x == 0,
    );
}

/// A selective filter on sampled_from should only produce values satisfying
/// the predicate, not trigger a FilterTooMuch health check.
#[test]
fn test_sampled_from_filter_produces_only_valid_values() {
    assert_all_examples(
        gs::sampled_from(vec![1_i64, 2, 3, 4, 5]).filter(|x: &i64| *x > 2),
        |x: &i64| *x > 2,
    );
}

/// When all elements are rejected, panic immediately with a clear message
/// rather than triggering FilterTooMuch or silently passing vacuously.
#[test]
fn test_sampled_from_unsatisfiable_filter_panics() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let _: i64 =
                    tc.draw(gs::sampled_from((0..10_i64).collect::<Vec<i64>>()).filter(|x| *x < 0));
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "(?i)(unsatisfiable|filter)",
    );
}

/// Chained .map().filter() on sampled_from should also use enumerate_values.
#[test]
fn test_sampled_from_mapped_then_filtered() {
    assert_all_examples(
        gs::sampled_from(vec![1_i64, 2, 3, 4, 5])
            .map(|x: i64| x * 2)
            .filter(|x: &i64| *x > 4),
        |x: &i64| *x > 4,
    );
}

/// Boxed filtered sampled_from forwards enumerate_values through the box.
#[test]
fn test_sampled_from_filtered_boxed() {
    assert_all_examples(
        gs::sampled_from(vec![1_i64, 2, 3, 4, 5])
            .filter(|x: &i64| *x % 2 == 0)
            .boxed(),
        |x: &i64| *x % 2 == 0,
    );
}

mod control {
    use hegel::TestCase;
    use hegel::generators as gs;
    use hegel::{Hegel, Settings};

    #[test]
    fn test_not_currently_in_hypothesis() {
        assert!(!hegel::currently_in_test_context());
    }

    #[test]
    fn test_currently_in_hypothesis() {
        Hegel::new(|tc: TestCase| {
            let _: i64 = tc.draw(gs::integers());
            assert!(hegel::currently_in_test_context());
        })
        .settings(Settings::new().test_cases(10).database(None))
        .run();
    }

    struct ContextMachine;

    #[hegel::state_machine]
    impl ContextMachine {
        #[rule]
        fn step(&mut self, _tc: TestCase) {
            assert!(hegel::currently_in_test_context());
        }
    }

    #[test]
    fn test_currently_in_stateful_test() {
        Hegel::new(|tc: TestCase| {
            let m = ContextMachine;
            hegel::stateful::run(m, tc);
        })
        .settings(Settings::new().test_cases(10).database(None))
        .run();
    }
}

mod find {
    //! Python's `find()` and `phases` setting have no public hegel-rust
    //! counterparts. The original test pins down that `find(..., random=Random(13))`
    //! is deterministic across runs — we express the same property by driving
    //! `Hegel::new(...)` with `seed(Some(13))` and recording the first value
    //! that matches the predicate.

    #[allow(unused_imports)]
    use super::not_supported_on_native;
    use hegel::generators as gs;
    use hegel::{Hegel, Settings};
    use std::panic::AssertUnwindSafe;
    use std::sync::{Arc, Mutex};

    #[not_supported_on_native]
    #[test]
    fn test_find_uses_provided_seed() {
        let mut prev: Option<String> = None;

        for _ in 0..3 {
            let found: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            let found_clone = Arc::clone(&found);

            std::panic::catch_unwind(AssertUnwindSafe(|| {
                Hegel::new(move |tc| {
                    let v: String = tc.draw(gs::text());
                    if v.chars().count() > 5 {
                        let mut g = found_clone.lock().unwrap();
                        if g.is_none() {
                            *g = Some(v);
                        }
                        drop(g);
                        panic!("HEGEL_FOUND");
                    }
                })
                .settings(
                    Settings::new()
                        .test_cases(1000)
                        .database(None)
                        .seed(Some(13)),
                )
                .run();
            }))
            .ok();

            let value = found.lock().unwrap().take().unwrap();

            if let Some(ref p) = prev {
                assert_eq!(p, &value);
            } else {
                prev = Some(value);
            }
        }
    }
}

mod nothing {
    use std::collections::HashSet;

    use super::common::utils::minimal;
    use hegel::generators::{self as gs, Generator};

    #[test]
    fn test_resampling() {
        let x = minimal(
            gs::vecs(gs::integers::<i64>())
                .min_size(1)
                .flat_map(|xs| gs::vecs(gs::sampled_from(xs))),
            |xs: &Vec<i64>| xs.len() >= 10 && xs.iter().collect::<HashSet<_>>().len() == 1,
        );
        assert_eq!(x, vec![0_i64; 10]);
    }
}

mod one_of {
    //! Omitted (Python-specific, no Rust counterpart):
    //! - test_one_of_single_strategy_is_noop: Python `is` identity check
    //! - test_one_of_without_strategies_suggests_sampled_from: Python dynamic typing error
    //! - test_one_of_unwrapping: Python `repr()` output

    use super::common::utils::{assert_all_examples, expect_panic};
    use hegel::generators::{self as gs, Generator};

    #[test]
    fn test_one_of_empty() {
        expect_panic(
            || {
                gs::one_of::<i64, _>(vec![]);
            },
            "one_of requires at least one generator",
        );
    }

    #[test]
    fn test_one_of_filtered() {
        assert_all_examples(
            gs::one_of(vec![gs::integers::<i64>().filter(|i| *i != 0).boxed()]),
            |i: &i64| *i != 0,
        );
    }

    #[test]
    fn test_one_of_flatmapped() {
        assert_all_examples(
            gs::one_of(vec![
                gs::just(100i64)
                    .flat_map(|n| gs::integers::<i64>().min_value(n))
                    .boxed(),
            ]),
            |i: &i64| *i >= 100,
        );
    }
}

mod searchstrategy {
    //! Tests that rely on Python-specific facilities are not ported:
    //!
    //! - `test_or_errors_when_given_non_strategy` — Python `|` operator overloading.
    //! - `test_just_strategy_uses_repr`, `test_can_map_nameless`,
    //!   `test_can_flatmap_nameless` — Python `__repr__` and `functools.partial`.
    //! - `test_flatmap_with_invalid_expand` — Python dynamic typing; Rust's
    //!   `flat_map` requires its closure to return a generator at compile time.
    //! - `test_use_of_global_random_is_deprecated_in_given`,
    //!   `test_use_of_global_random_is_deprecated_in_interactive_draws` — Python
    //!   global `random` module and `@checks_deprecated_behaviour`.
    //! - `test_jsonable*`, `test_to_jsonable_handles_reference_cycles` — test
    //!   `hypothesis.strategies._internal.utils.to_jsonable`, a Python-only
    //!   observability helper with no hegel-rust counterpart.
    //! - `test_deferred_strategy_draw` — `st.deferred()` has no hegel-rust analog;
    //!   Rust's static types don't support forward-referenced recursive strategies.

    use super::common::utils::{assert_simple_property, expect_panic};
    use hegel::generators::{self as gs, Generator};
    use hegel::{Hegel, Settings};

    #[test]
    fn test_can_map() {
        assert_simple_property(gs::integers::<i64>().map(|_| "foo"), |v: &&str| *v == "foo");
    }

    #[test]
    fn test_example_raises_unsatisfiable_when_too_filtered() {
        expect_panic(
            || {
                Hegel::new(|tc| {
                    let _: i64 = tc.draw(gs::integers::<i64>().filter(|_: &i64| false));
                })
                .settings(Settings::new().database(None))
                .run();
            },
            "(?i)(health.check|FailedHealthCheck|filter|unsatisfiable)",
        );
    }
}

mod arbitrary_data {
    //! Python's `st.data()` strategy returns a data object that exposes a
    //! `.draw()` method for dynamic draws inside a test. In hegel-rust, every
    //! test body already receives a `tc: TestCase` with the same surface
    //! (`tc.draw(...)`), so there is no separate `data()` strategy — the
    //! "conditional draw" pattern ports as a normal `Hegel::new(|tc| …).run()`,
    //! and the "dynamic draw inside `find()`" pattern ports as a `compose!`
    //! generator passed to `minimal()`.

    use super::common::project::TempRustProject;
    use super::common::utils::minimal;
    use hegel::generators as gs;
    use hegel::{Hegel, Settings};

    #[test]
    fn test_conditional_draw() {
        Hegel::new(|tc| {
            let x: i64 = tc.draw(gs::integers::<i64>());
            let y: i64 = tc.draw(gs::integers::<i64>().min_value(x));
            assert!(y >= x);
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_prints_on_failure() {
        // Python: asserts "Draw 1: [0, 0]" and "Draw 2: 0" are in the failure
        // output's PEP 678 `__notes__`. hegel-rust writes drawn values to
        // stderr as `let draw_N = …;` when they aren't bound to a named
        // variable, so the equivalent assertion is on those lines.
        const CODE: &str = r#"
use hegel::generators as gs;
use hegel::{Hegel, Settings};

fn main() {
    Hegel::new(|tc| {
        let xs: Vec<i64> = tc.draw(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(10)).min_size(2),
        );
        let y: i64 = tc.draw(gs::sampled_from(xs.clone()));
        let mut xs = xs;
        if let Some(pos) = xs.iter().position(|v| *v == y) {
            xs.remove(pos);
        }
        if xs.contains(&y) {
            panic!("PRINTS_ON_FAILURE");
        }
    })
    .settings(Settings::new().database(None))
    .run();
}
"#;

        let output = TempRustProject::new()
            .main_file(CODE)
            .expect_failure("PRINTS_ON_FAILURE")
            .cargo_run(&[]);

        assert!(
            output.stderr.contains("let draw_1 = [0, 0];"),
            "expected `let draw_1 = [0, 0];` in stderr:\n{}",
            output.stderr
        );
        assert!(
            output.stderr.contains("let draw_2 = 0;"),
            "expected `let draw_2 = 0;` in stderr:\n{}",
            output.stderr
        );
    }

    #[test]
    fn test_prints_labels_if_given_on_failure() {
        // Python: `data.draw(strategy, label="Some numbers")` attaches a label
        // used in the failure output as `Draw 1 (Some numbers): …`. The
        // hegel-rust equivalent is `tc.__draw_named(generator, name, false)`,
        // which renders as `let name = value;` — we assert on those lines.
        const CODE: &str = r#"
use hegel::generators as gs;
use hegel::{Hegel, Settings};

fn main() {
    Hegel::new(|tc| {
        let xs: Vec<i64> = tc.__draw_named(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(10)).min_size(2),
            "some_numbers",
            false,
        );
        let y: i64 = tc.__draw_named(gs::sampled_from(xs.clone()), "a_number", false);
        let mut xs = xs;
        if let Some(pos) = xs.iter().position(|v| *v == y) {
            xs.remove(pos);
        }
        if xs.contains(&y) {
            panic!("PRINTS_LABELS_ON_FAILURE");
        }
    })
    .settings(Settings::new().database(None))
    .run();
}
"#;

        let output = TempRustProject::new()
            .main_file(CODE)
            .expect_failure("PRINTS_LABELS_ON_FAILURE")
            .cargo_run(&[]);

        assert!(
            output.stderr.contains("let some_numbers = [0, 0];"),
            "expected `let some_numbers = [0, 0];` in stderr:\n{}",
            output.stderr
        );
        assert!(
            output.stderr.contains("let a_number = 0;"),
            "expected `let a_number = 0;` in stderr:\n{}",
            output.stderr
        );
    }

    #[test]
    fn test_given_twice_is_same() {
        // Python: `@given(st.data(), st.data())` with `data1.draw(...)` and
        // `data2.draw(...)` asserts `Draw 1: 0` / `Draw 2: 0` appear in the
        // failure's `__notes__`. hegel-rust has a single `tc`, so the port is
        // two consecutive `tc.draw()` calls; the same Draw-N numbering appears
        // as `let draw_N = ...;` lines in stderr.
        const CODE: &str = r#"
use hegel::generators as gs;
use hegel::{Hegel, Settings};

fn main() {
    Hegel::new(|tc| {
        tc.draw(gs::integers::<i64>());
        tc.draw(gs::integers::<i64>());
        panic!("TWICE_IS_SAME");
    })
    .settings(Settings::new().database(None))
    .run();
}
"#;

        let output = TempRustProject::new()
            .main_file(CODE)
            .expect_failure("TWICE_IS_SAME")
            .cargo_run(&[]);

        assert!(
            output.stderr.contains("let draw_1 = 0;"),
            "expected `let draw_1 = 0;` in stderr:\n{}",
            output.stderr
        );
        assert!(
            output.stderr.contains("let draw_2 = 0;"),
            "expected `let draw_2 = 0;` in stderr:\n{}",
            output.stderr
        );
    }

    #[test]
    fn test_data_supports_find() {
        // Python: `find(st.data(), lambda data: data.draw(st.integers()) >= 10)`
        // then `assert data.conjecture_data.choices == (10,)`. In hegel-rust,
        // `compose!` plays the role of `st.data()` (dynamic draws inside a
        // generator) and `minimal()` plays the role of `find()`; the
        // engine-internal `choices` accessor has no public counterpart, so we
        // assert on the returned minimal value instead.
        let value: i64 = minimal(
            hegel::compose!(|tc| { tc.draw(gs::integers::<i64>()) }),
            |x: &i64| *x >= 10,
        );
        assert_eq!(value, 10);
    }
}

mod filtered_strategy {}

mod nocover_filtering {
    use super::common::utils::assert_all_examples;
    use hegel::generators::{self as gs, BoxedGenerator, Generator};
    use hegel::{Hegel, Settings};
    use std::collections::HashSet;

    #[test]
    fn test_filter_correctly_integers_gt_one() {
        assert_all_examples(gs::integers::<i64>().filter(|x: &i64| *x > 1), |x: &i64| {
            *x > 1
        });
    }

    #[test]
    fn test_filter_correctly_nonempty_lists() {
        assert_all_examples(
            gs::vecs(gs::integers::<i64>()).filter(|xs: &Vec<i64>| !xs.is_empty()),
            |xs: &Vec<i64>| !xs.is_empty(),
        );
    }

    fn run_chained_filters_agree(base: BoxedGenerator<'static, i64>) {
        Hegel::new(move |tc| {
            let forbidden: HashSet<i64> = tc
                .draw(gs::hashsets(gs::integers::<i64>().min_value(1).max_value(20)).max_size(19));

            let mut s: BoxedGenerator<'static, i64> = base.clone();
            for f in &forbidden {
                let f = *f;
                s = s.filter(move |x: &i64| *x != f).boxed();
            }

            let x: i64 = tc.draw(&s);
            assert!((1..=20).contains(&x));
            assert!(!forbidden.contains(&x));
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_chained_filters_agree_integers_1_20() {
        run_chained_filters_agree(gs::integers::<i64>().min_value(1).max_value(20).boxed());
    }

    #[test]
    fn test_chained_filters_agree_integers_0_19_mapped() {
        run_chained_filters_agree(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(19)
                .map(|x| x + 1)
                .boxed(),
        );
    }

    #[test]
    fn test_chained_filters_agree_sampled_from_1_20() {
        let values: Vec<i64> = (1..=20).collect();
        run_chained_filters_agree(gs::sampled_from(values).boxed());
    }

    #[test]
    fn test_chained_filters_agree_sampled_from_0_19_mapped() {
        let values: Vec<i64> = (0..20).collect();
        run_chained_filters_agree(gs::sampled_from(values).map(|x| x + 1).boxed());
    }
}

mod nocover_flatmap {
    #[allow(unused_imports)]
    use super::not_supported_on_native;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    use super::common::utils::{Minimal, minimal};
    use hegel::generators::{self as gs, Generator};
    use hegel::{HealthCheck, Hegel, Settings, TestCase};

    #[test]
    fn test_constant_lists_are_constant() {
        Hegel::new(|tc: TestCase| {
            let x: Vec<i64> = tc.draw(gs::integers::<i64>().flat_map(|i| gs::vecs(gs::just(i))));
            tc.assume(x.len() >= 3);
            let first = x[0];
            assert!(x.iter().all(|&v| v == first));
        })
        .settings(
            Settings::new()
                .test_cases(100)
                .database(None)
                .suppress_health_check([HealthCheck::FilterTooMuch]),
        )
        .run();
    }

    #[test]
    fn test_in_order() {
        Hegel::new(|tc: TestCase| {
            let (a, b): (i64, i64) = tc.draw(
                gs::integers::<i64>()
                    .min_value(1)
                    .max_value(200)
                    .flat_map(|e| {
                        gs::tuples!(
                            gs::integers::<i64>().min_value(0).max_value(e - 1),
                            gs::just(e),
                        )
                    }),
            );
            assert!(a < b);
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    #[test]
    fn test_flatmap_retrieve_from_db() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().to_str().unwrap().to_string();

        let track: Arc<Mutex<Vec<Vec<f64>>>> = Arc::new(Mutex::new(Vec::new()));

        let run_test = || {
            let track = Arc::clone(&track);
            let db_path = db_path.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                Hegel::new(move |tc: TestCase| {
                    let xs: Vec<f64> = tc.draw(
                        gs::floats::<f64>()
                            .min_value(0.0)
                            .max_value(1.0)
                            .flat_map(|x| gs::vecs(gs::just(x))),
                    );
                    if xs.iter().sum::<f64>() >= 1.0 {
                        track.lock().unwrap().push(xs);
                        panic!("expected failure");
                    }
                })
                .settings(Settings::new().database(Some(db_path)).derandomize(false))
                .__database_key("test_flatmap_retrieve_from_db".to_string())
                .run();
            }));
            assert!(result.is_err(), "expected the test to fail");
        };

        run_test();
        let example = {
            let guard = track.lock().unwrap();
            assert!(!guard.is_empty());
            guard.last().unwrap().clone()
        };

        track.lock().unwrap().clear();

        run_test();

        let guard = track.lock().unwrap();
        assert_eq!(guard[0], example);
    }

    #[not_supported_on_native]
    #[test]
    fn test_mixed_list_flatmap() {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        enum Value {
            Bool(bool),
            Text(String),
        }

        let result = Minimal::new(
            gs::vecs(gs::booleans().flat_map(|b| {
                if b {
                    gs::booleans().map(Value::Bool).boxed()
                } else {
                    gs::text().map(Value::Text).boxed()
                }
            })),
            |ls: &Vec<Value>| {
                let bools = ls.iter().filter(|x| matches!(x, Value::Bool(_))).count();
                let texts = ls.iter().filter(|x| matches!(x, Value::Text(_))).count();
                bools >= 3 && texts >= 3
            },
        )
        .test_cases(10000)
        .run();
        assert_eq!(result.len(), 6);
        let as_set: HashSet<_> = result.into_iter().collect();
        assert_eq!(
            as_set,
            HashSet::from([Value::Bool(false), Value::Text(String::new())])
        );
    }

    fn shrink_through_a_binding_case(n: usize) {
        let result = minimal(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(100)
                .flat_map(|k| {
                    gs::vecs(gs::booleans())
                        .min_size(k as usize)
                        .max_size(k as usize)
                }),
            move |x: &Vec<bool>| x.iter().filter(|&&b| b).count() >= n,
        );
        assert_eq!(result, vec![true; n]);
    }

    #[test]
    fn test_can_shrink_through_a_binding_1() {
        shrink_through_a_binding_case(1);
    }

    #[test]
    fn test_can_shrink_through_a_binding_2() {
        shrink_through_a_binding_case(2);
    }

    #[test]
    fn test_can_shrink_through_a_binding_3() {
        shrink_through_a_binding_case(3);
    }

    #[test]
    fn test_can_shrink_through_a_binding_4() {
        shrink_through_a_binding_case(4);
    }

    #[test]
    fn test_can_shrink_through_a_binding_5() {
        shrink_through_a_binding_case(5);
    }

    #[test]
    fn test_can_shrink_through_a_binding_6() {
        shrink_through_a_binding_case(6);
    }

    #[test]
    fn test_can_shrink_through_a_binding_7() {
        shrink_through_a_binding_case(7);
    }

    #[test]
    fn test_can_shrink_through_a_binding_8() {
        shrink_through_a_binding_case(8);
    }

    #[test]
    fn test_can_shrink_through_a_binding_9() {
        shrink_through_a_binding_case(9);
    }

    fn delete_in_middle_of_a_binding_case(n: usize) {
        let result = minimal(
            gs::integers::<i64>()
                .min_value(1)
                .max_value(100)
                .flat_map(|k| {
                    gs::vecs(gs::booleans())
                        .min_size(k as usize)
                        .max_size(k as usize)
                }),
            move |x: &Vec<bool>| {
                x.len() >= 2 && x[0] && *x.last().unwrap() && x.iter().filter(|&&b| !b).count() >= n
            },
        );
        let mut expected = vec![false; n + 2];
        expected[0] = true;
        expected[n + 1] = true;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_can_delete_in_middle_of_a_binding_1() {
        delete_in_middle_of_a_binding_case(1);
    }

    #[test]
    fn test_can_delete_in_middle_of_a_binding_2() {
        delete_in_middle_of_a_binding_case(2);
    }

    #[test]
    fn test_can_delete_in_middle_of_a_binding_3() {
        delete_in_middle_of_a_binding_case(3);
    }

    #[test]
    fn test_can_delete_in_middle_of_a_binding_4() {
        delete_in_middle_of_a_binding_case(4);
    }

    #[test]
    fn test_can_delete_in_middle_of_a_binding_5() {
        delete_in_middle_of_a_binding_case(5);
    }

    #[test]
    fn test_can_delete_in_middle_of_a_binding_6() {
        delete_in_middle_of_a_binding_case(6);
    }

    #[test]
    fn test_can_delete_in_middle_of_a_binding_7() {
        delete_in_middle_of_a_binding_case(7);
    }

    #[test]
    fn test_can_delete_in_middle_of_a_binding_8() {
        delete_in_middle_of_a_binding_case(8);
    }

    #[test]
    fn test_can_delete_in_middle_of_a_binding_9() {
        delete_in_middle_of_a_binding_case(9);
    }
}

mod nocover_deferred_errors {
    //! The upstream file pins down Hypothesis's deferred-validation semantics:
    //! invalid strategy construction (bad bounds, empty `sampled_from`, etc.)
    //! does NOT raise on the construction call — it raises only when the
    //! strategy is actually used to generate data. Hegel-rust mostly matches
    //! this: bound checks in `IntegerGenerator` / `FloatGenerator` /
    //! `VecGenerator` fire from `do_draw`, not the builder methods.
    //!
    //! The `st.sampled_from([])` line inside
    //! `test_does_not_error_on_initial_calculation` is also omitted:
    //! hegel-rust's `gs::sampled_from` asserts non-empty *at construction*
    //! (see `tests/hypothesis/direct_strategies.rs::test_sampled_from_rejects_empty`),
    //! so the empty case cannot be used as a deferred-error witness here.

    use super::common::utils::{check_can_generate_examples, expect_panic, minimal};
    use hegel::generators as gs;
    use hegel::{Hegel, Settings};

    #[test]
    fn test_does_not_error_on_initial_calculation() {
        // Constructing generators with bounds that will fail on draw must not
        // panic at construction time. Mirrors upstream's "creating a broken
        // strategy is allowed; using it is not" contract.
        gs::floats::<f64>().max_value(f64::NAN);
        gs::vecs(gs::integers::<i64>()).min_size(5).max_size(2);
        gs::floats::<f64>().min_value(2.0).max_value(1.0);
    }

    #[test]
    fn test_errors_each_time() {
        for _ in 0..2 {
            expect_panic(
                || {
                    check_can_generate_examples(gs::integers::<i64>().max_value(1).min_value(3));
                },
                "max_value < min_value",
            );
        }
    }

    #[test]
    fn test_errors_on_test_invocation() {
        expect_panic(
            || {
                Hegel::new(|tc| {
                    tc.draw(gs::integers::<i64>().max_value(1).min_value(3));
                })
                .settings(Settings::new().test_cases(1).database(None))
                .run();
            },
            "max_value < min_value",
        );
    }

    #[test]
    fn test_errors_on_find() {
        // Python: `find(s, lambda x: True)` with an invalid strategy. Hegel
        // has no public `find()`; the nearest helper is `minimal()`, which
        // also drives a `Hegel::new(...).run()` under the hood and surfaces
        // the deferred bounds panic.
        expect_panic(
            || {
                minimal(
                    gs::vecs(gs::integers::<i64>()).min_size(5).max_size(2),
                    |_: &Vec<i64>| true,
                );
            },
            "max_size < min_size",
        );
    }

    #[test]
    fn test_errors_on_example() {
        expect_panic(
            || {
                check_can_generate_examples(gs::floats::<f64>().min_value(2.0).max_value(1.0));
            },
            "max_value < min_value",
        );
    }
}

mod nocover_imports {
    //! The original checks `from hypothesis import *` and
    //! `from hypothesis.strategies import *` both work. The Rust analog is
    //! glob-importing from `hegel` and `hegel::generators`; this test
    //! exercises both glob imports (scoped inside the function so that the
    //! `hegel::test` proc macro doesn't shadow the built-in `#[test]`).

    #[test]
    fn test_can_glob_import_from_hegel() {
        use hegel::generators::*;
        use hegel::*;

        Hegel::new(|tc| {
            let xs: Vec<i32> = tc.draw(vecs(integers::<i32>()));
            let _ = xs.iter().map(|&x| x as i64).sum::<i64>() > 1;
        })
        .settings(
            Settings::new()
                .test_cases(10)
                .verbosity(Verbosity::Quiet)
                .database(None),
        )
        .run();
    }
}

mod nocover_given_reuse {
    //! The Python original tests that a `@given(st.booleans())` decorator value
    //! can be re-bound and re-applied to multiple test functions with different
    //! argument names, and that failures in one application don't bleed into
    //! another. hegel-rust has no `@given` decorator; the analog is sharing a
    //! generator value across multiple `Hegel::new(...)` invocations.

    #[allow(unused_imports)]
    use super::not_supported_on_native;
    use hegel::generators::{self as gs};
    use hegel::{Hegel, Settings};

    #[test]
    fn test_has_an_arg_named_x() {
        let g = gs::booleans();
        Hegel::new(|tc| {
            let _x: bool = tc.draw(&g);
        })
        .settings(Settings::new().database(None))
        .run();
    }

    #[test]
    fn test_has_an_arg_named_y() {
        let g = gs::booleans();
        Hegel::new(|tc| {
            let _y: bool = tc.draw(&g);
        })
        .settings(Settings::new().database(None))
        .run();
    }

    #[not_supported_on_native]
    #[test]
    fn test_fail_independently() {
        let g = gs::text();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Hegel::new(|tc| {
                let _z: String = tc.draw(&g);
                panic!("AssertionError");
            })
            .settings(Settings::new().database(None))
            .run();
        }));
        assert!(result.is_err());

        Hegel::new(|tc| {
            let _z: String = tc.draw(&g);
        })
        .settings(Settings::new().database(None))
        .run();
    }
}

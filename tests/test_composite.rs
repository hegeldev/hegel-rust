mod common;

use hegel::TestCase;
use hegel::generators as gs;

#[hegel::composite]
fn composite_integer_generator(tc: &TestCase, lower: i32, upper: i32, offset: i32) -> i32 {
    let x = tc.draw(gs::integers::<i32>().min_value(lower).max_value(upper));
    x + offset
}

#[hegel::test]
fn test_passing_composite_generation(tc: TestCase) {
    let x = tc.draw(composite_integer_generator(0, 100, 1));
    assert!(x > 0);
}

mod composite {
    //! Tests that target Python-specific facilities have no Rust counterpart and
    //! are not ported:
    //!
    //! - `test_uses_definitions_for_reprs` — Python `__repr__`.
    //! - `test_errors_given_default_for_draw`, `test_errors_given_function_of_no_arguments`,
    //!   `test_errors_given_kwargs_only`, `test_warning_given_no_drawfn_call` —
    //!   Python-syntax validation of `@st.composite`. The Rust equivalent is
    //!   enforced at compile time by the macro.
    //! - `test_can_use_pure_args` — relies on Python `*args` variadic composites.
    //! - `test_does_not_change_arguments` — relies on Python `data().draw()` and
    //!   object identity (`is`).
    //! - `test_applying_composite_decorator_to_methods` — Python decorator
    //!   ordering with `@classmethod`/`@staticmethod`.
    //! - `test_drawfn_cannot_be_instantiated`, `test_warns_on_strategy_annotation`,
    //!   `test_composite_allows_overload_without_draw` — Python `DrawFn`,
    //!   strategy return-type warnings, and `typing.overload` respectively.

    use super::common::utils::minimal;
    use hegel::TestCase;
    use hegel::generators as gs;
    use hegel::{HealthCheck, Hegel, Settings};

    #[hegel::composite]
    fn badly_draw_lists(tc: &TestCase, m: i32) -> Vec<i32> {
        let length = tc.draw(gs::integers::<i32>().min_value(m).max_value(m + 10));
        let mut out = Vec::with_capacity(length.max(0) as usize);
        for _ in 0..length {
            out.push(tc.draw(gs::integers::<i32>()));
        }
        out
    }

    #[test]
    fn test_simplify_draws() {
        assert_eq!(
            minimal(badly_draw_lists(0), |xs: &Vec<i32>| xs.len() >= 3),
            vec![0; 3]
        );
    }

    #[test]
    fn test_can_pass_through_arguments_5() {
        assert_eq!(
            minimal(badly_draw_lists(5), |_: &Vec<i32>| true),
            vec![0; 5]
        );
    }

    #[test]
    fn test_can_assume_in_draw() {
        Hegel::new(|tc| {
            let (x, y) = tc.draw(&hegel::compose!(|tc| {
                let x = tc.draw(gs::floats::<f64>());
                let y = tc.draw(gs::floats::<f64>());
                tc.assume(x < y);
                (x, y)
            }));
            assert!(x < y);
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
    fn test_composite_of_lists() {
        let f = || {
            hegel::compose!(|tc| {
                tc.draw(gs::integers::<i32>())
                    .wrapping_add(tc.draw(gs::integers::<i32>()))
            })
        };
        assert_eq!(
            minimal(gs::vecs(f()), |xs: &Vec<i32>| xs.len() >= 10),
            vec![0; 10]
        );
    }

    #[test]
    fn test_can_shrink_matrices_with_length_param() {
        let value = minimal(
            hegel::compose!(|tc| {
                let rows = tc.draw(gs::integers::<usize>().min_value(1).max_value(10));
                let columns = tc.draw(gs::integers::<usize>().min_value(1).max_value(10));
                (0..rows)
                    .map(|_| {
                        (0..columns)
                            .map(|_| tc.draw(gs::integers::<i32>().min_value(0).max_value(10000)))
                            .collect::<Vec<i32>>()
                    })
                    .collect::<Vec<Vec<i32>>>()
            }),
            |m: &Vec<Vec<i32>>| {
                let n = m.len();
                if m[0].len() != n {
                    return false;
                }
                (0..n).any(|i| (i + 1..n).any(|j| m[i][j] != m[j][i]))
            },
        );
        assert_eq!(value.len(), 2);
        assert_eq!(value[0].len(), 2);
        let mut combined: Vec<i32> = value[0].iter().chain(value[1].iter()).copied().collect();
        combined.sort();
        assert_eq!(combined, vec![0, 0, 0, 1]);
    }
}

mod composite_structs {
    //! The #[composite] attribute expands to a named generator struct, so
    //! composite generators can be referred to by type, stored, cloned, and
    //! passed as arguments to other composites.

    use hegel::TestCase;
    use hegel::generators::{self as gs, BoxedGenerator, Generator};

    #[hegel::composite]
    fn small_int(tc: &TestCase) -> i64 {
        tc.draw(gs::integers::<i64>().min_value(0).max_value(10))
    }

    #[test]
    fn test_composite_returns_a_nameable_type() {
        let generator: SmallIntCompositeGenerator = small_int();
        let cloned: SmallIntCompositeGenerator = generator.clone();
        hegel::Hegel::new(move |tc: TestCase| {
            let x = tc.draw(&generator);
            let y = tc.draw(&cloned);
            assert!((0..=10).contains(&x));
            assert!((0..=10).contains(&y));
        })
        .settings(hegel::Settings::new().test_cases(10))
        .run();
    }

    #[hegel::composite]
    fn int_pair(tc: &TestCase, element: SmallIntCompositeGenerator) -> (i64, i64) {
        (tc.draw(&element), tc.draw(&element))
    }

    #[hegel::test(test_cases = 10)]
    fn test_composites_can_be_arguments_to_composites(tc: TestCase) {
        let (a, b) = tc.draw(int_pair(small_int()));
        assert!((0..=10).contains(&a));
        assert!((0..=10).contains(&b));
    }

    #[hegel::composite]
    fn doubled<G: Generator<i64>>(tc: &TestCase, inner: G) -> i64 {
        tc.draw_silent(&inner).wrapping_mul(2)
    }

    #[hegel::test(test_cases = 10)]
    fn test_generic_composite_arguments(tc: TestCase) {
        let x = tc.draw(doubled(gs::integers::<i64>().boxed()));
        assert_eq!(x % 2, 0);
    }

    #[hegel::composite]
    fn from_boxed(tc: &TestCase, inner: BoxedGenerator<'static, i64>) -> i64 {
        tc.draw_silent(&inner)
    }

    #[hegel::test(test_cases = 10)]
    fn test_boxed_generator_arguments(tc: TestCase) {
        let x = tc.draw(from_boxed(
            gs::integers::<i64>().min_value(0).max_value(5).boxed(),
        ));
        assert!((0..=5).contains(&x));
    }

    #[hegel::composite]
    fn even(tc: &TestCase) -> i64 {
        let n = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
        tc.assume(n % 2 == 0);
        n
    }

    #[hegel::test]
    fn test_assume_inside_composite(tc: TestCase) {
        let n = tc.draw(even());
        assert_eq!(n % 2, 0);
    }
}

mod composite_kwonlyargs {
    //! Tests that composite generators with parameters work when used in collection generators.
    //! Python's keyword-only args have no Rust counterpart; regular function parameters
    //! cover the same semantics.

    use super::common::utils::check_can_generate_examples;
    use hegel::TestCase;
    use hegel::generators as gs;

    #[hegel::composite]
    fn kwonlyargs_composites(tc: &TestCase, kwarg1: &'static str) -> (String, i64) {
        let i = tc.draw(gs::integers::<i64>());
        (kwarg1.to_string(), i)
    }

    #[test]
    fn test_composite_with_keyword_only_args() {
        check_can_generate_examples(gs::vecs(kwonlyargs_composites("test")));
    }
}

mod composite_borrowed_data {
    //! Composite generators can take borrowed parameters. A `&T` parameter may
    //! leave its lifetime elided, and the macro supplies one for the generated
    //! generator struct. If the lifetime is used in the result it must be specified.

    use super::*;

    #[derive(Debug)]
    struct Object {
        name: String,
        x: u8,
        y: u8,
    }

    hegel::pretty_print_as_debug!(Object);

    #[hegel::composite]
    fn object_from_borrowed_name(tc: &TestCase, name: &str) -> Object {
        let x = tc.draw(gs::integers().max_value(3));
        let y = tc.draw(gs::integers().max_value(3));

        Object {
            name: name.to_owned(),
            x,
            y,
        }
    }

    #[hegel::test]
    fn test_elided_borrow_can_be_copied_into_generated_value(tc: TestCase) {
        let object = tc.draw(object_from_borrowed_name("hello"));
        assert_eq!(object.name, "hello");
        assert!(object.x <= 3 && object.y <= 3);
    }

    #[derive(Debug)]
    struct Config {
        min: u8,
        max: u8,
    }

    #[hegel::composite]
    fn int_bounded_by_borrowed_config(tc: &TestCase, config: &Config) -> u8 {
        tc.draw(gs::integers().min_value(config.min).max_value(config.max))
    }

    #[hegel::test]
    fn test_elided_borrow_can_configure_draws(tc: TestCase) {
        let config = Config { min: 5, max: 10 };
        let x = tc.draw(int_bounded_by_borrowed_config(&config));
        assert!((5..=10).contains(&x));
    }

    #[derive(Debug)]
    struct BorrowedObject<'a> {
        name: &'a str,
        x: u8,
    }

    hegel::pretty_print_as_debug!(BorrowedObject<'_>);

    #[hegel::composite]
    fn borrowed_object_from_name<'a>(
        tc: &TestCase,
        name: &'a str,
        config: &Config,
    ) -> BorrowedObject<'a> {
        BorrowedObject {
            name,
            x: tc.draw(gs::integers().min_value(config.min).max_value(config.max)),
        }
    }

    #[hegel::test]
    fn test_explicit_lifetime_borrow_is_stored_in_generated_value(tc: TestCase) {
        let config = Config { min: 5, max: 10 };
        let object = tc.draw(borrowed_object_from_name("hello", &config));

        assert_eq!(object.name, "hello");
        assert!((5..=10).contains(&object.x));
    }
}

#[deny(clippy::items_after_statements)]
#[allow(unused)]
mod item_after_statement_lint {
    use super::*;

    #[hegel::composite]
    fn link_metric(tc: &TestCase) -> u32 {
        const MAX_METRIC: u32 = 1000;

        let x = tc.draw(gs::integers().max_value(MAX_METRIC));
        x + 1
    }
}

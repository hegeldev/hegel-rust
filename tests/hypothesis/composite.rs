//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_composite.py.
//!
//! Tests that target Python-specific facilities have no Rust counterpart and
//! are not ported:
//!
//! - `test_uses_definitions_for_reprs` — Python `__repr__`.
//! - `test_errors_given_default_for_draw`, `test_errors_given_function_of_no_arguments`,
//!   `test_errors_given_kwargs_only`, `test_warning_given_no_drawfn_call` —
//!   Python-syntax validation of `@st.composite`. The Rust equivalent is
//!   compile-time and is covered in `tests/compile/fail/composite_*.rs`.
//! - `test_can_use_pure_args` — relies on Python `*args` variadic composites.
//! - `test_does_not_change_arguments` — relies on Python `data().draw()` and
//!   object identity (`is`).
//! - `test_applying_composite_decorator_to_methods` — Python decorator
//!   ordering with `@classmethod`/`@staticmethod`.
//! - `test_drawfn_cannot_be_instantiated`, `test_warns_on_strategy_annotation`,
//!   `test_composite_allows_overload_without_draw` — Python `DrawFn`,
//!   strategy return-type warnings, and `typing.overload` respectively.

use crate::common::utils::minimal;
use hegel::generators as gs;
use hegel::{HealthCheck, Hegel, Settings, TestCase};

#[hegel::composite]
fn badly_draw_lists(tc: TestCase, m: i32) -> Vec<i32> {
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
fn test_can_pass_through_arguments_6() {
    assert_eq!(
        minimal(badly_draw_lists(6), |_: &Vec<i32>| true),
        vec![0; 6]
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

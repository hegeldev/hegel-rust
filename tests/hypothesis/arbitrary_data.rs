//! Ported from hypothesis-python/tests/cover/test_arbitrary_data.py
//!
//! Python's `st.data()` strategy returns a data object that exposes a
//! `.draw()` method for dynamic draws inside a test. In hegel-rust, every
//! test body already receives a `tc: TestCase` with the same surface
//! (`tc.draw(...)`), so there is no separate `data()` strategy — the
//! "conditional draw" pattern ports as a normal `Hegel::new(|tc| …).run()`.
//!
//! Individually-skipped tests:
//!
//! - `test_given_twice_is_same` — hegel-rust tests take a single `tc`
//!   argument; the "two independent `data()` arguments" shape has no
//!   counterpart.
//! - `test_data_supports_find` — uses `hypothesis.find(st.data(), …)` and
//!   asserts on `data.conjecture_data.choices`, a Python engine-internal
//!   attribute; hegel-rust has no standalone `find()` and no public
//!   "choices" accessor on a returned value.
//! - `test_errors_when_normal_strategy_functions_are_used` — asserts
//!   `st.data().filter(...)` / `.map(...)` / `.flatmap(...)` raise
//!   `InvalidArgument`; there is no `st.data()` strategy object in
//!   hegel-rust to apply those transforms to.
//! - `test_nice_repr` — tests `repr(st.data()) == "data()"`; Python `repr`
//!   has no Rust counterpart.

use crate::common::project::TempRustProject;
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

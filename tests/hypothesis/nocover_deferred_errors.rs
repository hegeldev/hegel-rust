//! Ported from hypothesis-python/tests/nocover/test_deferred_errors.py
//!
//! The upstream file pins down Hypothesis's deferred-validation semantics:
//! invalid strategy construction (bad bounds, empty `sampled_from`, etc.)
//! does NOT raise on the construction call — it raises only when the
//! strategy is actually used to generate data. Hegel-rust mostly matches
//! this: bound checks in `IntegerGenerator` / `FloatGenerator` /
//! `VecGenerator` fire from `do_draw`, not the builder methods.
//!
//! Individually-skipped tests (see SKIPPED.md):
//!
//! - `test_does_not_recalculate_the_strategy` — uses Python's
//!   `hypothesis.strategies._internal.core.defines_strategy` decorator,
//!   which wraps a factory in a `LazyStrategy` that memoizes the
//!   underlying `SearchStrategy` after the first use. Hegel-rust has no
//!   equivalent memoising-factory decorator in its public API, and Rust
//!   closures have no introspectable call-count surface.
//!
//! The `st.sampled_from([])` line inside
//! `test_does_not_error_on_initial_calculation` is also omitted:
//! hegel-rust's `gs::sampled_from` asserts non-empty *at construction*
//! (see `tests/hypothesis/direct_strategies.rs::test_sampled_from_rejects_empty`),
//! so the empty case cannot be used as a deferred-error witness here.

use crate::common::utils::{check_can_generate_examples, expect_panic, minimal};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};

fn run_draw_once<T, G>(generator: G)
where
    G: Generator<T> + 'static,
    T: std::fmt::Debug + Send + 'static,
{
    Hegel::new(move |tc| {
        tc.draw(&generator);
    })
    .settings(Settings::new().test_cases(1).database(None))
    .run();
}

#[test]
fn test_does_not_error_on_initial_calculation() {
    // Constructing generators with bounds that will fail on draw must not
    // panic at construction time. Mirrors upstream's "creating a broken
    // strategy is allowed; using it is not" contract.
    let _ = gs::floats::<f64>().max_value(f64::NAN);
    let _ = gs::vecs(gs::integers::<i64>()).min_size(5).max_size(2);
    let _ = gs::floats::<f64>().min_value(2.0).max_value(1.0);
}

#[test]
fn test_errors_each_time() {
    for _ in 0..2 {
        expect_panic(
            || {
                run_draw_once(gs::integers::<i64>().max_value(1).min_value(3));
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
                let _: i64 = tc.draw(gs::integers::<i64>().max_value(1).min_value(3));
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

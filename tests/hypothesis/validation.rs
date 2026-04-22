//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_validation.py
//!
//! Individually-skipped tests (see SKIPPED.md):
//!
//! - `test_errors_when_given_varargs`,
//!   `test_varargs_without_positional_arguments_allowed`,
//!   `test_errors_when_given_varargs_and_kwargs_with_positional_arguments`,
//!   `test_varargs_and_kwargs_without_positional_arguments_allowed`,
//!   `test_bare_given_errors`, `test_errors_on_unwanted_kwargs`,
//!   `test_errors_on_too_many_positional_args`, `test_errors_on_any_varargs`,
//!   `test_can_put_arguments_in_the_middle`, `test_stuff_keyword`,
//!   `test_stuff_positional`, `test_too_many_positional`,
//!   `test_given_warns_on_use_of_non_strategies`,
//!   `test_given_warns_when_mixing_positional_with_keyword` — all exercise
//!   Python `@given(*args, **kwargs)` argument-passing semantics (varargs,
//!   default kwargs, mixed positional/keyword, type-as-strategy via `@given(bool)`).
//!   `#[hegel::test]` takes generators directly, so this validation surface has
//!   no Rust counterpart.
//! - `test_list_unique_and_unique_by_cannot_both_be_enabled` — uses
//!   `unique_by=key_fn`; hegel-rust's `VecGenerator::unique` only accepts a
//!   `bool`, so the conflict can't be expressed.
//! - `test_recursion_validates_base_case`,
//!   `test_recursion_validates_recursive_step` — `st.recursive()` has no
//!   hegel-rust equivalent (already covered by the whole-file skip of
//!   `test_recursive.py`).
//! - `test_cannot_find_non_strategies` — uses Python `find()` and treats
//!   `bool` as a type-as-strategy; neither has a Rust counterpart.
//! - `test_valid_sizes` — passes `min_size="0"` (a string) and
//!   `max_size="10"`; Rust's typed `min_size: usize` rejects this at
//!   compile time, so there is nothing to assert at runtime.
//! - `test_check_type_with_tuple_of_length_two`,
//!   `test_check_type_suggests_check_strategy`,
//!   `test_check_strategy_might_suggest_sampled_from` — exercise Python-only
//!   internal helpers (`hypothesis.internal.validation.check_type`,
//!   `hypothesis.strategies._internal.strategies.check_strategy`).
//! - `test_warn_on_strings_matching_common_codecs` — exercises a Hypothesis
//!   warning fired when `st.text('ascii')` is called with a codec-like
//!   positional alphabet string. hegel-rust's `gs::text()` separates
//!   `.alphabet()` and `.codec()` into distinct methods, so the codec/alphabet
//!   ambiguity the warning targets doesn't exist.

use crate::common::utils::{check_can_generate_examples, expect_panic};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};

fn expect_draw_panic<T, G>(generator: G, pattern: &str)
where
    G: Generator<T> + 'static + std::panic::UnwindSafe,
    T: std::fmt::Debug + Send + 'static,
{
    expect_panic(
        move || {
            Hegel::new(move |tc| {
                tc.draw(&generator);
            })
            .settings(Settings::new().test_cases(1).database(None))
            .run();
        },
        pattern,
    );
}

#[test]
fn test_float_ranges() {
    // floats(float("nan"), 0): NaN min compares as `!(min <= max)`, tripping
    // the bound check.
    expect_draw_panic(
        gs::floats::<f64>().min_value(f64::NAN).max_value(0.0),
        "max_value < min_value",
    );
    expect_draw_panic(
        gs::floats::<f64>().min_value(1.0).max_value(-1.0),
        "max_value < min_value",
    );
}

#[test]
fn test_float_range_and_allow_nan_cannot_both_be_enabled() {
    expect_draw_panic(
        gs::floats::<f64>().min_value(1.0).allow_nan(true),
        "allow_nan=true with min_value or max_value",
    );
    expect_draw_panic(
        gs::floats::<f64>().max_value(1.0).allow_nan(true),
        "allow_nan=true with min_value or max_value",
    );
}

#[test]
fn test_float_finite_range_and_allow_infinity_cannot_both_be_enabled() {
    expect_draw_panic(
        gs::floats::<f64>()
            .min_value(0.0)
            .max_value(1.0)
            .allow_infinity(true),
        "allow_infinity=true with both min_value and max_value",
    );
}

#[test]
fn test_does_not_error_if_min_size_is_bigger_than_default_size() {
    check_can_generate_examples(gs::vecs(gs::integers::<i64>()).min_size(50));
    check_can_generate_examples(gs::hashsets(gs::integers::<i64>()).min_size(50));
    // Python also tests `frozensets(...)`; hegel-rust has no `gs::frozensets()`,
    // but `hashsets` covers the same set-shaped case.
    check_can_generate_examples(gs::vecs(gs::integers::<i64>()).min_size(50).unique(true));
}

#[test]
fn test_min_before_max() {
    expect_draw_panic(
        gs::integers::<i64>().min_value(1).max_value(0),
        "max_value < min_value",
    );
}

#[test]
fn test_filter_validates() {
    // Python: integers(min_value=1, max_value=0).filter(bool).validate().
    // The bad bounds inside the filter wrapper still surface when we draw.
    expect_draw_panic(
        gs::integers::<i64>()
            .min_value(1)
            .max_value(0)
            .filter(|x: &i64| *x != 0),
        "max_value < min_value",
    );
}

#[test]
fn test_validation_happens_on_draw() {
    // Python port uses `nothing()` inside flatmap; hegel-rust has no
    // `gs::nothing()`, so we use invalid integer bounds as the always-bad
    // inner generator. The point is the same: the inner strategy produced
    // by the flat_map callback is only validated when it is drawn.
    expect_draw_panic(
        gs::integers::<i64>().flat_map(|_| gs::integers::<i64>().min_value(1).max_value(0)),
        "max_value < min_value",
    );
}

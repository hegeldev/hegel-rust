mod common;

use hegel::TestCase;
use hegel::generators::{self as gs, Generator};

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_integers_min_greater_than_max() {
    let g = gs::integers::<i32>().min_value(10).max_value(5);
    g.as_basic();
}

#[test]
#[should_panic(expected = "allow_nan=true")]
fn test_floats_allow_nan_with_min_value() {
    let g = gs::floats::<f64>().allow_nan(true).min_value(0.0);
    g.as_basic();
}

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_floats_min_greater_than_max() {
    let g = gs::floats::<f64>().min_value(10.0).max_value(5.0);
    g.as_basic();
}

#[test]
#[should_panic(expected = "InvalidArgument")]
fn test_floats_pos_zero_min_neg_zero_max() {
    let g = gs::floats::<f64>().min_value(0.0).max_value(-0.0);
    g.as_basic();
}

#[test]
#[should_panic(expected = "InvalidArgument")]
fn test_floats_pos_zero_min_neg_zero_max_f32() {
    let g = gs::floats::<f32>().min_value(0.0).max_value(-0.0);
    g.as_basic();
}

#[test]
#[should_panic(expected = "allow_infinity=true")]
fn test_floats_allow_infinity_with_both_bounds() {
    let g = gs::floats::<f64>()
        .allow_infinity(true)
        .min_value(0.0)
        .max_value(1.0);
    g.as_basic();
}

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_text_min_greater_than_max() {
    let g = gs::text().min_size(5).max_size(3);
    g.as_basic();
}

#[test]
fn test_text_character_params_build_schema() {
    let g = gs::text().codec("ascii");
    assert!(g.as_basic().is_some());

    let g = gs::text().min_codepoint(0x20).max_codepoint(0x7E);
    assert!(g.as_basic().is_some());

    let g = gs::text().categories(&["L", "Nd"]);
    assert!(g.as_basic().is_some());

    let g = gs::text().exclude_categories(&["Cc"]);
    assert!(g.as_basic().is_some());

    let g = gs::text().include_characters("abc");
    assert!(g.as_basic().is_some());

    let g = gs::text().exclude_characters("xyz");
    assert!(g.as_basic().is_some());
}

#[test]
#[should_panic(expected = "\"Cs\" includes surrogate codepoints")]
fn test_text_categories_including_cs_panics() {
    let g = gs::text().categories(&["L", "Cs"]);
    g.as_basic();
}

#[test]
#[should_panic(expected = "\"C\" includes surrogate codepoints")]
fn test_text_categories_including_cs_supercat_panics() {
    let g = gs::text().categories(&["C"]);
    g.as_basic();
}

#[test]
fn test_characters_as_basic() {
    let g = gs::characters();
    assert!(g.as_basic().is_some());
}

#[test]
fn test_characters_params_build_schema() {
    let g = gs::characters().codec("ascii");
    assert!(g.as_basic().is_some());

    let g = gs::characters().min_codepoint(0x20).max_codepoint(0x7E);
    assert!(g.as_basic().is_some());

    let g = gs::characters().categories(&["L", "Nd"]);
    assert!(g.as_basic().is_some());

    let g = gs::characters().exclude_categories(&["Cc"]);
    assert!(g.as_basic().is_some());

    let g = gs::characters().include_characters("abc");
    assert!(g.as_basic().is_some());

    let g = gs::characters().exclude_characters("xyz");
    assert!(g.as_basic().is_some());
}

#[test]
#[should_panic(expected = "\"Cs\" includes surrogate codepoints")]
fn test_characters_categories_including_cs_panics() {
    let g = gs::characters().categories(&["Cs"]);
    g.as_basic();
}

#[test]
#[should_panic(expected = "\"C\" includes surrogate codepoints")]
fn test_characters_categories_including_cs_supercat_panics() {
    let g = gs::characters().categories(&["C"]);
    g.as_basic();
}

#[test]
#[should_panic(expected = "Cannot combine .alphabet() with character methods")]
fn test_text_alphabet_with_codec() {
    let g = gs::text().alphabet("abc").codec("ascii");
    g.as_basic();
}

#[test]
#[should_panic(expected = "Cannot combine .alphabet() with character methods")]
fn test_text_codec_with_alphabet() {
    let g = gs::text().codec("ascii").alphabet("abc");
    g.as_basic();
}

#[test]
#[should_panic(expected = "Cannot combine .alphabet() with character methods")]
fn test_text_alphabet_with_categories() {
    let g = gs::text().alphabet("abc").categories(&["Lu"]);
    g.as_basic();
}

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_binary_min_greater_than_max() {
    let g = gs::binary().min_size(5).max_size(3);
    g.as_basic();
}

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_vecs_min_greater_than_max() {
    let g = gs::vecs(gs::booleans()).min_size(5).max_size(3);
    g.as_basic();
}

// --- hashsets ---

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_hashsets_min_greater_than_max() {
    let g = gs::hashsets(gs::booleans()).min_size(5).max_size(3);
    g.as_basic();
}

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_hashmaps_min_greater_than_max() {
    let g = gs::hashmaps(gs::text(), gs::booleans())
        .min_size(5)
        .max_size(3);
    g.as_basic();
}

#[test]
#[should_panic(expected = "max_length must be between 4 and 255")]
fn test_domains_max_length_too_small() {
    let g = gs::domains().max_length(2);
    g.as_basic();
}

#[test]
#[should_panic(expected = "sampled_from cannot be empty")]
fn test_sampled_from_empty() {
    let _g = gs::sampled_from(Vec::<i32>::new());
}

#[test]
#[should_panic(expected = "one_of requires at least one generator")]
fn test_one_of_empty() {
    let _g = gs::one_of(Vec::<hegel::generators::BoxedGenerator<'_, i32>>::new());
}

// --- server-side error handling ---

#[hegel::test]
#[should_panic(expected = "InvalidArgument")]
fn test_server_invalid_argument_is_reported(tc: TestCase) {
    // The surrogate codepoint range (0xD800..=0xDFFF) has no valid characters.
    // The client doesn't catch this, but the server returns InvalidArgument.
    let _: char = tc.draw(gs::characters().min_codepoint(0xD800).max_codepoint(0xD800));
}

// ── hypothesis/test_validation.py ───────────────────────────────────────────

mod validation {
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

    use super::common::utils::{check_can_generate_examples, expect_panic};
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
}

// ── hypothesis/test_given_error_conditions.py ───────────────────────────────

mod given_error_conditions {
    //! Individually-skipped tests (see SKIPPED.md):
    //!
    //! - `test_raises_unsatisfiable_if_passed_explicit_nothing` — uses `nothing()`,
    //!   the empty-generator strategy; hegel-rust has no `gs::nothing()` public API.
    //! - `test_error_if_has_no_hints`, `test_error_if_infer_all_and_has_no_hints`,
    //!   `test_error_if_infer_is_posarg`, `test_error_if_infer_is_posarg_mixed_with_kwarg`
    //!   — exercise Python's `@given(a=...)` / `@given(...)` ellipsis syntax for
    //!   type-hint-based strategy inference. `#[hegel::test]` takes generators
    //!   directly, so this inference mechanism has no Rust counterpart.
    //! - `test_given_twice_is_an_error` — stacks two `@given` decorators on one
    //!   function; `#[hegel::test]` doesn't compose that way.
    //! - `test_given_is_not_a_class_decorator` — applies `@given` to a Python
    //!   class; Rust has no analogous class/macro composition.
    //! - `test_specific_error_for_coroutine_functions` — asserts a specific error
    //!   for Python `async def` tests; hegel-rust has no async-test dispatch.
    //! - `test_suggests_at_settings_if_extra_kwarg_matches_setting_name` —
    //!   inspects `@given` kwarg handling against Python setting names. hegel-rust
    //!   uses the `.settings(Settings::new()...)` builder rather than kwargs on
    //!   the test macro.

    use hegel::generators as gs;
    use hegel::{Hegel, Settings, TestCase};

    // Port of `test_raises_unsatisfiable_if_all_false_in_finite_set`. In native
    // mode, a test that always rejects trips the `FilterTooMuch` health check
    // (hegel-rust's analog of Hypothesis's `Unsatisfiable`). In server mode, the
    // runner silently passes on all-rejected runs, so this assertion is
    // native-only.

    #[test]
    fn test_does_not_raise_unsatisfiable_if_some_false_in_finite_set() {
        Hegel::new(|tc: TestCase| {
            let x: bool = tc.draw(gs::booleans());
            tc.assume(x);
        })
        .settings(Settings::new().database(None))
        .run();
    }
}

mod common;

use common::utils::expect_panic;
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
                tc.draw_silent(&generator);
            })
            .settings(Settings::new().test_cases(1).database(None))
            .run();
        },
        pattern,
    );
}

fn check_draws<T, G>(generator: G)
where
    G: Generator<T> + 'static,
    T: std::fmt::Debug + Send + 'static,
{
    Hegel::new(move |tc| {
        tc.draw_silent(&generator);
    })
    .settings(Settings::new().test_cases(5).database(None))
    .run();
}

#[test]
fn test_integers_min_greater_than_max() {
    expect_draw_panic(
        gs::integers::<i32>().min_value(10).max_value(5),
        "max_value < min_value",
    );
}

#[test]
fn test_floats_allow_nan_with_min_value() {
    expect_draw_panic(
        gs::floats::<f64>().allow_nan(true).min_value(0.0),
        "allow_nan=true",
    );
}

#[test]
fn test_floats_min_greater_than_max() {
    expect_draw_panic(
        gs::floats::<f64>().min_value(10.0).max_value(5.0),
        "max_value < min_value",
    );
}

#[test]
fn test_floats_pos_zero_min_neg_zero_max() {
    expect_draw_panic(
        gs::floats::<f64>().min_value(0.0).max_value(-0.0),
        "InvalidArgument",
    );
}

#[test]
fn test_floats_pos_zero_min_neg_zero_max_f32() {
    expect_draw_panic(
        gs::floats::<f32>().min_value(0.0).max_value(-0.0),
        "InvalidArgument",
    );
}

#[test]
fn test_floats_allow_infinity_with_both_bounds() {
    expect_draw_panic(
        gs::floats::<f64>()
            .allow_infinity(true)
            .min_value(0.0)
            .max_value(1.0),
        "allow_infinity=true",
    );
}

#[test]
fn test_text_min_greater_than_max() {
    expect_draw_panic(gs::text().min_size(5).max_size(3), "max_size < min_size");
}

#[test]
fn test_text_character_params_draw() {
    check_draws(gs::text().codec("ascii"));
    check_draws(gs::text().min_codepoint(0x20).max_codepoint(0x7E));
    check_draws(gs::text().categories(&["L", "Nd"]));
    check_draws(gs::text().exclude_categories(&["Cc"]));
    check_draws(gs::text().include_characters("abc"));
    check_draws(gs::text().exclude_characters("xyz"));
}

#[test]
fn test_text_categories_including_cs_panics() {
    expect_draw_panic(
        gs::text().categories(&["L", "Cs"]),
        "\"Cs\" includes surrogate codepoints",
    );
}

#[test]
fn test_text_categories_including_cs_supercat_panics() {
    expect_draw_panic(
        gs::text().categories(&["C"]),
        "\"C\" includes surrogate codepoints",
    );
}

#[test]
fn test_characters_draws() {
    check_draws(gs::characters());
}

#[test]
fn test_characters_params_draw() {
    check_draws(gs::characters().codec("ascii"));
    check_draws(gs::characters().min_codepoint(0x20).max_codepoint(0x7E));
    check_draws(gs::characters().categories(&["L", "Nd"]));
    check_draws(gs::characters().exclude_categories(&["Cc"]));
    check_draws(gs::characters().include_characters("abc"));
    check_draws(gs::characters().exclude_characters("xyz"));
}

#[test]
fn test_characters_categories_including_cs_panics() {
    expect_draw_panic(
        gs::characters().categories(&["Cs"]),
        "\"Cs\" includes surrogate codepoints",
    );
}

#[test]
fn test_characters_categories_including_cs_supercat_panics() {
    expect_draw_panic(
        gs::characters().categories(&["C"]),
        "\"C\" includes surrogate codepoints",
    );
}

/// Unknown codec names are rejected eagerly, when `.codec(...)` is called,
/// rather than on first draw.
#[test]
fn test_text_unknown_codec_rejected_eagerly() {
    expect_panic(
        || {
            let _ = gs::text().codec("not-a-real-codec");
        },
        "invalid codec: not-a-real-codec",
    );
}

/// Same eager rejection for `characters()`.
#[test]
fn test_characters_unknown_codec_rejected_eagerly() {
    expect_panic(
        || {
            let _ = gs::characters().codec("not-a-real-codec");
        },
        "invalid codec: not-a-real-codec",
    );
}

/// All engine-supported codec names pass the client-side check.
#[test]
fn test_supported_codecs_accepted() {
    for codec in ["ascii", "latin-1", "iso-8859-1", "utf-8"] {
        check_draws(gs::text().codec(codec));
        check_draws(gs::characters().codec(codec));
    }
}

#[test]
fn test_text_alphabet_with_codec() {
    expect_draw_panic(gs::text().alphabet("abc").codec("ascii"), "Cannot combine");
}

#[test]
fn test_text_codec_with_alphabet() {
    expect_draw_panic(gs::text().codec("ascii").alphabet("abc"), "Cannot combine");
}

#[test]
fn test_text_alphabet_with_categories() {
    expect_draw_panic(
        gs::text().alphabet("abc").categories(&["Lu"]),
        "Cannot combine",
    );
}

#[test]
fn test_binary_min_greater_than_max() {
    expect_draw_panic(gs::binary().min_size(5).max_size(3), "max_size < min_size");
}

#[test]
fn test_vecs_min_greater_than_max() {
    expect_draw_panic(
        gs::vecs(gs::booleans()).min_size(5).max_size(3),
        "max_size < min_size",
    );
}

#[test]
fn test_hashsets_min_greater_than_max() {
    expect_draw_panic(
        gs::hashsets(gs::booleans()).min_size(5).max_size(3),
        "max_size < min_size",
    );
}

#[test]
fn test_hashmaps_min_greater_than_max() {
    expect_draw_panic(
        gs::hashmaps(gs::text(), gs::booleans())
            .min_size(5)
            .max_size(3),
        "max_size < min_size",
    );
}

#[test]
fn test_domains_max_length_too_small() {
    expect_draw_panic(
        gs::domains().max_length(2),
        "max_length must be between 4 and 255",
    );
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

#[hegel::test]
#[should_panic(expected = "InvalidArgument")]
fn test_surrogate_only_character_range_is_invalid_argument(tc: hegel::TestCase) {
    let _: char = tc.draw(gs::characters().min_codepoint(0xD800).max_codepoint(0xD800));
}

mod validation {
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
                    tc.draw_silent(&generator);
                })
                .settings(Settings::new().test_cases(1).database(None))
                .run();
            },
            pattern,
        );
    }

    #[test]
    fn test_float_ranges() {
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
        expect_draw_panic(
            gs::integers::<i64>().flat_map(|_| gs::integers::<i64>().min_value(1).max_value(0)),
            "max_value < min_value",
        );
    }
}

mod given_error_conditions {
    use hegel::generators as gs;
    use hegel::{Hegel, Settings, TestCase};

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

#[test]
fn test_explain_phase_requires_shrink_phase() {
    expect_panic(
        || {
            Settings::new().phases([hegel::Phase::Explain]);
        },
        "Phase::Explain requires Phase::Shrink",
    );
}

#[test]
fn test_explain_phase_with_shrink_phase_is_accepted() {
    Settings::new().phases([hegel::Phase::Shrink, hegel::Phase::Explain]);
}

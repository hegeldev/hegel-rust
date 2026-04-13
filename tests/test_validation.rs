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
    let _g = gs::sampled_from::<i32>(vec![]);
}

#[test]
#[should_panic(expected = "one_of requires at least one generator")]
fn test_one_of_empty() {
    let _g = gs::one_of::<i32>(vec![]);
}

// --- server-side error handling ---

#[hegel::test]
#[should_panic(expected = "InvalidArgument")]
fn test_server_invalid_argument_is_reported(tc: TestCase) {
    // The surrogate codepoint range (0xD800..=0xDFFF) has no valid characters.
    // The client doesn't catch this, but the server returns InvalidArgument.
    let _: char = tc.draw(gs::characters().min_codepoint(0xD800).max_codepoint(0xD800));
}

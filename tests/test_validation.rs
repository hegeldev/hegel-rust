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

use hegel::generators;
use hegel::generators::Generate;

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_integers_min_greater_than_max() {
    let gen = generators::integers::<i32>().min_value(10).max_value(5);
    gen.as_basic();
}

#[test]
#[should_panic(expected = "allow_nan=true")]
fn test_floats_allow_nan_with_min_value() {
    let gen = generators::floats::<f64>().allow_nan(true).min_value(0.0);
    gen.as_basic();
}

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_floats_min_greater_than_max() {
    let gen = generators::floats::<f64>().min_value(10.0).max_value(5.0);
    gen.as_basic();
}

#[test]
#[should_panic(expected = "allow_infinity=true")]
fn test_floats_allow_infinity_with_both_bounds() {
    let gen = generators::floats::<f64>()
        .allow_infinity(true)
        .min_value(0.0)
        .max_value(1.0);
    gen.as_basic();
}

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_text_min_greater_than_max() {
    let gen = generators::text().min_size(5).max_size(3);
    gen.as_basic();
}

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_binary_min_greater_than_max() {
    let gen = generators::binary().min_size(5).max_size(3);
    gen.as_basic();
}

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_vecs_min_greater_than_max() {
    let gen = generators::vecs(generators::booleans())
        .min_size(5)
        .max_size(3);
    gen.as_basic();
}

// --- hashsets ---

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_hashsets_min_greater_than_max() {
    let gen = generators::hashsets(generators::booleans())
        .min_size(5)
        .max_size(3);
    gen.as_basic();
}

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_hashmaps_min_greater_than_max() {
    let gen = generators::hashmaps(generators::text(), generators::booleans())
        .min_size(5)
        .max_size(3);
    gen.as_basic();
}

#[test]
#[should_panic(expected = "max_length must be between 4 and 255")]
fn test_domains_max_length_too_small() {
    let gen = generators::domains().max_length(2);
    gen.as_basic();
}

#[test]
#[should_panic(expected = "sampled_from cannot be empty")]
fn test_sampled_from_empty() {
    let _gen = generators::sampled_from::<i32>(vec![]);
}

#[test]
#[should_panic(expected = "one_of requires at least one generator")]
fn test_one_of_empty() {
    let _gen = generators::one_of::<i32>(vec![]);
}

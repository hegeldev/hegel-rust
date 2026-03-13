use hegel::generators;
use hegel::generators::Generator;

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_integers_min_greater_than_max() {
    let generator = generators::integers::<i32>().min_value(10).max_value(5);
    generator.as_basic();
}

#[test]
#[should_panic(expected = "allow_nan=true")]
fn test_floats_allow_nan_with_min_value() {
    let generator = generators::floats::<f64>().allow_nan(true).min_value(0.0);
    generator.as_basic();
}

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_floats_min_greater_than_max() {
    let generator = generators::floats::<f64>().min_value(10.0).max_value(5.0);
    generator.as_basic();
}

#[test]
#[should_panic(expected = "allow_infinity=true")]
fn test_floats_allow_infinity_with_both_bounds() {
    let generator = generators::floats::<f64>()
        .allow_infinity(true)
        .min_value(0.0)
        .max_value(1.0);
    generator.as_basic();
}

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_text_min_greater_than_max() {
    let generator = generators::text().min_size(5).max_size(3);
    generator.as_basic();
}

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_binary_min_greater_than_max() {
    let generator = generators::binary().min_size(5).max_size(3);
    generator.as_basic();
}

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_vecs_min_greater_than_max() {
    let generator = generators::vecs(generators::booleans())
        .min_size(5)
        .max_size(3);
    generator.as_basic();
}

// --- hashsets ---

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_hashsets_min_greater_than_max() {
    let generator = generators::hashsets(generators::booleans())
        .min_size(5)
        .max_size(3);
    generator.as_basic();
}

#[test]
#[should_panic(expected = "max_size < min_size")]
fn test_hashmaps_min_greater_than_max() {
    let generator = generators::hashmaps(generators::text(), generators::booleans())
        .min_size(5)
        .max_size(3);
    generator.as_basic();
}

#[test]
#[should_panic(expected = "max_length must be between 4 and 255")]
fn test_domains_max_length_too_small() {
    let generator = generators::domains().max_length(2);
    generator.as_basic();
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

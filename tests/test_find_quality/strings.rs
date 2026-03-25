use crate::common::utils::find_any;
use hegel::generators;

#[test]
fn test_can_produce_unstripped_strings() {
    find_any(generators::text(), |x: &String| x != x.trim());
}

#[test]
fn test_can_produce_stripped_strings() {
    find_any(generators::text(), |x: &String| x == x.trim());
}

#[test]
fn test_can_produce_multi_line_strings() {
    find_any(generators::text(), |x: &String| x.contains('\n'));
}

#[test]
fn test_can_produce_ascii_strings() {
    find_any(generators::text(), |x: &String| x.is_ascii());
}

#[test]
fn test_can_produce_long_strings_with_no_ascii() {
    find_any(generators::text().min_size(5), |x: &String| {
        x.chars().all(|c| c as u32 > 127)
    });
}

#[test]
fn test_can_produce_short_strings_with_some_non_ascii() {
    find_any(generators::text(), |x: &String| {
        x.chars().count() <= 3 && x.chars().any(|c| c as u32 > 127)
    });
}

#[test]
fn test_can_produce_large_binary_strings() {
    find_any(generators::binary(), |x: &Vec<u8>| x.len() > 10);
}

#[test]
fn test_long_duplicates_strings() {
    find_any(
        generators::tuples!(generators::text(), generators::text()),
        |(a, b): &(String, String)| a.chars().count() >= 5 && a == b,
    );
}

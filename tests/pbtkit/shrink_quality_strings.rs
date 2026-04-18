//! Ported from resources/pbtkit/tests/shrink_quality/test_strings.py.

use crate::common::utils::{Minimal, minimal};
use hegel::generators as gs;

#[test]
fn test_minimize_string_to_empty() {
    let s: String = minimal(gs::text(), |_: &String| true);
    assert_eq!(s, "");
}

#[test]
fn test_minimize_longer_string() {
    let s = minimal(gs::text().max_size(50), |x: &String| {
        x.chars().count() >= 10
    });
    assert_eq!(s, "0".repeat(10));
}

#[test]
fn test_minimize_longer_list_of_strings() {
    let v = minimal(gs::vecs(gs::text()), |x: &Vec<String>| x.len() >= 10);
    assert_eq!(v, vec![String::new(); 10]);
}

#[test]
fn test_string_sorts_characters_when_possible() {
    // String shrinking should sort characters by codepoint.
    // Sorting "0e0" produces "00e" (smaller codepoints first).
    let s = Minimal::new(
        gs::text().min_codepoint(32).max_codepoint(126).max_size(20),
        |v0: &String| v0.chars().count() >= 3 && v0.contains('e'),
    )
    .test_cases(1000)
    .run();
    assert_eq!(s, "00e");
}

#[test]
fn test_string_insertion_sort_swap_succeeds() {
    // Fixed-length 2-char string over {'a','b'} where the condition requires
    // both letters. Starting from "ba" the insertion-sort swap produces "ab".
    let s = Minimal::new(
        gs::text()
            .min_codepoint(b'a' as u32)
            .max_codepoint(b'b' as u32)
            .min_size(2)
            .max_size(2),
        |s: &String| s.contains('a') && s.contains('b'),
    )
    .test_cases(1000)
    .run();
    assert_eq!(s, "ab");
}

#[test]
fn test_string_length_redistribution() {
    // When two strings share a total-length constraint (len(v0)+len(v1) >= 30),
    // the shrinker should redistribute length so v0 is as short as possible
    // (10 chars, since v1 caps at 20). Regression for shrink quality found
    // by pbtsmith.
    let (v0, _v1) = Minimal::new(
        gs::tuples!(
            gs::text().min_codepoint(32).max_codepoint(126).max_size(20),
            gs::text().min_codepoint(32).max_codepoint(126).max_size(20),
        ),
        |(a, b): &(String, String)| a.chars().count() + b.chars().count() >= 30,
    )
    .test_cases(100)
    .run();
    assert_eq!(v0.chars().count(), 10);
}
